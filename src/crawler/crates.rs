//! crates.io crawler: newly created crates, from the crates.io API (#92).
//!
//! crates.io has no RSS feed, but `GET /api/v1/crates?sort=new` lists the
//! newest crates first, paged by `page` / `per_page` and stable enough
//! (newest-first) to stop at a checkpoint the same way the PyPI crawler
//! does: once we reach a crate we've already seen, everything after it on
//! later pages is older still.
//!
//! # Crawler policy compliance
//!
//! crates.io asks crawlers to (<https://crates.io/policies>):
//!
//! * send a descriptive User-Agent naming the tool and a contact — the
//!   crawler uses [`crate::net::DEFAULT_USER_AGENT`], which identifies
//!   DepRadar and links the repository;
//! * make at most one request per second.
//!
//! Both are enforced in [`PoliteClient`], not here: the rate limit is a
//! per-host token bucket, and this crawler never bypasses it (it has no
//! other HTTP path). Keep both in mind if you raise the crawl rate.
//!
//! The API also returns `meta.new_crates`, the total number of crates that
//! exist; a checkpoint can be seeded from it so the very first crawl doesn't
//! walk the whole history.

use serde::Deserialize;

use crate::net::{NetError, PoliteClient};

/// Base URL of the crates.io API.
pub const CRATES_IO_API_URL: &str = "https://crates.io";

/// Crates fetched per API page. crates.io caps `per_page` at 100.
const PER_PAGE: u32 = 100;

/// Errors from [`CratesCrawler`].
#[derive(Debug)]
pub enum CratesError {
    /// The HTTP request failed (or came back non-2xx).
    Net(NetError),
    /// The API response wasn't the JSON shape we expect.
    Malformed,
}

impl std::fmt::Display for CratesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CratesError::Net(err) => write!(f, "crates.io request failed: {err}"),
            CratesError::Malformed => write!(f, "crates.io response is not the expected shape"),
        }
    }
}

impl std::error::Error for CratesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CratesError::Net(err) => Some(err),
            CratesError::Malformed => None,
        }
    }
}

impl From<NetError> for CratesError {
    fn from(err: NetError) -> Self {
        CratesError::Net(err)
    }
}

/// One crate as listed by `GET /api/v1/crates`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CrateSummary {
    /// The crate name, e.g. `serde`.
    pub id: String,
    /// Most recently published version, e.g. `1.0.210`.
    #[serde(default)]
    pub newest_version: Option<String>,
    /// RFC 3339 creation timestamp, e.g. `2025-10-15T12:00:00.000000Z`.
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub downloads: Option<u64>,
    #[serde(default)]
    pub recent_downloads: Option<u64>,
}

/// Response body of `GET /api/v1/crates`.
#[derive(Debug, Deserialize)]
pub struct CratesPage {
    pub crates: Vec<CrateSummary>,
    /// Present on real responses; optional here so minimal fixtures work.
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
}

/// Dedupe state for the crates.io feed: the newest crate already seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Checkpoint {
    newest_crate: Option<String>,
}

impl Checkpoint {
    /// A checkpoint that has already seen `newest_crate` (or nothing, when
    /// `None`) — typically restored from a previous crawl's state.
    pub fn new(newest_crate: Option<&str>) -> Self {
        Checkpoint {
            newest_crate: newest_crate.map(str::to_string),
        }
    }

    /// Name of the newest already-seen crate, when any.
    pub fn newest_crate(&self) -> Option<&str> {
        self.newest_crate.as_deref()
    }

    /// True when `name` is the checkpoint crate — the signal to stop paging.
    pub fn reached(&self, name: &str) -> bool {
        self.newest_crate.as_deref() == Some(name)
    }

    /// Moves the checkpoint to the newest of `newly_seen` (crawl order:
    /// newest first).
    pub fn advance(&mut self, newly_seen: &[CrateSummary]) {
        if let Some(newest) = newly_seen.first() {
            self.newest_crate = Some(newest.id.clone());
        }
    }
}

/// Polls `GET /api/v1/crates?sort=new`, paging until it reaches the
/// checkpoint crate (or runs out of crates).
///
/// Requests go through [`PoliteClient`], which enforces crates.io's crawler
/// policy (descriptive User-Agent, ≤1 req/s) — see the module docs. The
/// per-page cost is therefore ~1 second of wall time per 100 crates; a
/// checkpoint from a previous run keeps steady-state crawls to a page or
/// two. Seed the checkpoint with [`Checkpoint::new`] from a saved
/// newest-crate name to skip history entirely on the first crawl.
pub struct CratesCrawler {
    client: PoliteClient,
    base_url: String,
    checkpoint: Checkpoint,
    /// Safety net so a lost checkpoint can't walk all ~180k crates.
    max_pages: u32,
}

impl Default for CratesCrawler {
    fn default() -> Self {
        Self::new(PoliteClient::default())
    }
}

impl CratesCrawler {
    /// A crawler for the live crates.io API.
    pub fn new(client: PoliteClient) -> Self {
        Self::with_base_url(client, CRATES_IO_API_URL)
    }

    /// A crawler against another base URL, e.g. a mock server in tests.
    pub fn with_base_url(client: PoliteClient, base_url: &str) -> Self {
        CratesCrawler {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            checkpoint: Checkpoint::default(),
            max_pages: 50,
        }
    }

    /// Starts from a restored checkpoint, e.g. the newest crate of the
    /// previous crawl, so the first crawl after a restart skips history.
    pub fn with_checkpoint(mut self, checkpoint: Checkpoint) -> Self {
        self.checkpoint = checkpoint;
        self
    }

    /// Caps how many pages one `crawl_new` call will fetch (each page is
    /// `PER_PAGE` crates). Zero disables the limit.
    pub fn with_max_pages(mut self, max_pages: u32) -> Self {
        self.max_pages = max_pages;
        self
    }

    /// Names of the crates created since the last crawl, newest first,
    /// then advances the checkpoint to the newest of them.
    ///
    /// An empty result is normal when either nothing new was published or
    /// the checkpoint crate is already on page 1.
    pub fn crawl_new(&mut self) -> Result<Vec<CrateSummary>, CratesError> {
        let mut newly_seen: Vec<CrateSummary> = Vec::new();
        let mut page = 1u32;
        loop {
            let url = format!(
                "{}/api/v1/crates?sort=new&per_page={PER_PAGE}&page={page}",
                self.base_url
            );
            let body: CratesPage = self.client.get_json(&url)?;
            let page_fetched = u32::try_from(body.crates.len()).unwrap_or(u32::MAX);
            if page_fetched == 0 {
                break;
            }
            let mut stop = false;
            for krate in body.crates {
                if self.checkpoint.reached(&krate.id) {
                    stop = true;
                    break;
                }
                newly_seen.push(krate);
            }
            let last_page = page_fetched < PER_PAGE;
            if stop || last_page || (self.max_pages != 0 && page >= self.max_pages) {
                break;
            }
            page += 1;
        }
        self.checkpoint.advance(&newly_seen);
        Ok(newly_seen)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn fast_client() -> PoliteClient {
        PoliteClient::new(
            Duration::from_millis(1),
            "DepRadar test agent",
            Duration::from_secs(5),
        )
        .unwrap()
    }

    fn page(names: &[&str]) -> String {
        let crates: Vec<String> = names
            .iter()
            .map(|name| {
                format!(
                    r#"{{"id":"{name}","newest_version":"1.0.0","created_at":"2025-10-15T12:00:00.000000Z","downloads":42}}"#
                )
            })
            .collect();
        format!(
            r#"{{"crates":[{}],"meta":{{"total_crates":3}}}}"#,
            crates.join(",")
        )
    }

    #[test]
    fn page_deserializes_the_api_shape() {
        let page: CratesPage = serde_json::from_str(&page(&["alpha", "beta"])).unwrap();
        assert_eq!(page.crates.len(), 2);
        assert_eq!(page.crates[0].id, "alpha");
        assert_eq!(page.crates[0].newest_version.as_deref(), Some("1.0.0"));
        assert_eq!(page.crates[0].downloads, Some(42));
        assert!(page.meta.is_some());
    }

    /// Two pages; the checkpoint crate is on page 2, so the crawl stops
    /// there and never asks for page 3.
    #[test]
    fn crawl_new_pages_until_the_checkpoint() {
        let mut server = mockito::Server::new();
        let page1: Vec<String> = (1..=100).map(|i| format!("c{i:03}")).collect();
        let page1_refs: Vec<&str> = page1.iter().map(String::as_str).collect();
        let m1 = server
            .mock("GET", "/api/v1/crates?sort=new&per_page=100&page=1")
            .with_body(page(&page1_refs))
            .expect(1)
            .create();
        let m2 = server
            .mock("GET", "/api/v1/crates?sort=new&per_page=100&page=2")
            .with_body(page(&["x-newer", "seen-before", "older"]))
            .expect(1)
            .create();

        let mut crawler = CratesCrawler::with_base_url(fast_client(), &server.url())
            .with_max_pages(50)
            .with_checkpoint(Checkpoint::new(Some("seen-before")));
        let new = crawler.crawl_new().unwrap();
        assert_eq!(new.len(), 101, "everything up to the checkpoint is new");
        assert_eq!(new[0].id, "c001");
        assert_eq!(new[100].id, "x-newer");
        assert_eq!(
            crawler.checkpoint.newest_crate(),
            Some("c001"),
            "checkpoint moves to the newest crate seen"
        );
        m1.assert();
        m2.assert();
    }

    /// A second crawl over the same data reports nothing new.
    #[test]
    fn crawl_new_dedupes_against_the_checkpoint() {
        let mut server = mockito::Server::new();
        let body = page(&["fresh", "fresh-2"]);
        let first = server
            .mock("GET", "/api/v1/crates?sort=new&per_page=100&page=1")
            .with_body(&body)
            .expect(1)
            .create();
        let second = server
            .mock("GET", "/api/v1/crates?sort=new&per_page=100&page=1")
            .with_body(&body)
            .expect(1)
            .create();

        let mut crawler = CratesCrawler::with_base_url(fast_client(), &server.url());
        let new = crawler.crawl_new().unwrap();
        assert_eq!(new.len(), 2);
        assert!(crawler.crawl_new().unwrap().is_empty());
        first.assert();
        second.assert();
    }

    /// The max-pages safety net stops the crawl even without a checkpoint.
    #[test]
    fn crawl_new_respects_max_pages() {
        let mut server = mockito::Server::new();
        let names: Vec<String> = (1..=100).map(|i| format!("c{i:03}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let body = page(&refs);
        // Page 2 would also be full, but max_pages = 1 forbids asking.
        let m1 = server
            .mock("GET", "/api/v1/crates?sort=new&per_page=100&page=1")
            .with_body(&body)
            .expect(1)
            .create();

        let mut crawler =
            CratesCrawler::with_base_url(fast_client(), &server.url()).with_max_pages(1);
        let new = crawler.crawl_new().unwrap();
        assert_eq!(new.len(), 100);
        m1.assert();
    }

    /// A short page means the end of the listing: no further requests.
    #[test]
    fn crawl_new_stops_on_a_short_page() {
        let mut server = mockito::Server::new();
        let m1 = server
            .mock("GET", "/api/v1/crates?sort=new&per_page=100&page=1")
            .with_body(page(&["only-one"]))
            .expect(1)
            .create();

        let mut crawler = CratesCrawler::with_base_url(fast_client(), &server.url());
        let new = crawler.crawl_new().unwrap();
        assert_eq!(new.len(), 1);
        m1.assert();
    }
}
