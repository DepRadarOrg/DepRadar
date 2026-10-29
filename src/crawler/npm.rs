//! npm registry crawler backed by the CouchDB `_changes` feed.
//!
//! A poll follows numeric sequence cursors up to its page cap or the current
//! feed head, then returns `last_seq` as the checkpoint for the next call. npm's
//! current replication API omits package documents, so changed package
//! metadata is fetched from the public registry. Both hosts use the shared
//! [`PoliteClient`] rate limiter.
//!
//! npm's migration to `/registry/_changes` and the opt-in header is described
//! in its [replication API documentation](https://github.com/npm/registry/blob/main/docs/REPLICATE-API.md).

use serde::Deserialize;

use crate::crawler::{Checkpoint, Crawler, PackageEvent, PackageEventKind};
use crate::model::Ecosystem;
use crate::net::{NetError, PoliteClient};

/// npm's replicated CouchDB registry feed.
pub const NPM_CHANGES_URL: &str = "https://replicate.npmjs.com/registry/_changes";
const NPM_PACKAGE_URL: &str = "https://registry.npmjs.org/";
const PAGE_LIMIT: usize = 100;
const NPM_OPT_IN_HEADER: &str = "npm-replication-opt-in";

/// Errors returned while polling npm's change feed.
#[derive(Debug)]
pub enum NpmError {
    /// HTTP request failure.
    Net(NetError),
    /// Invalid JSON or sequence data from the feed.
    Malformed,
}

impl std::fmt::Display for NpmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Net(error) => write!(f, "npm changes request failed: {error}"),
            Self::Malformed => write!(f, "npm changes feed has an invalid response shape"),
        }
    }
}

impl std::error::Error for NpmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Net(error) => Some(error),
            Self::Malformed => None,
        }
    }
}

impl From<NetError> for NpmError {
    fn from(error: NetError) -> Self {
        Self::Net(error)
    }
}

#[derive(Debug, Deserialize)]
struct ChangesPage {
    results: Vec<Change>,
    last_seq: u64,
}

#[derive(Debug, Deserialize)]
struct Change {
    id: String,
    #[serde(default)]
    changes: Vec<Revision>,
}

#[derive(Debug, Deserialize)]
struct Revision {
    rev: String,
}

#[derive(Debug, Deserialize)]
struct PackageDocument {
    #[serde(default)]
    time: Option<std::collections::BTreeMap<String, serde_json::Value>>,
}

/// Polls npm's `_changes` feed using a caller supplied sequence checkpoint.
pub struct NpmCrawler {
    client: PoliteClient,
    endpoint: String,
    package_endpoint: String,
    page_limit: usize,
    max_pages: usize,
}

impl Default for NpmCrawler {
    fn default() -> Self {
        Self::new(PoliteClient::default())
    }
}

impl NpmCrawler {
    /// Creates a crawler for the public npm registry replication feed.
    pub fn new(client: PoliteClient) -> Self {
        Self::with_endpoint(client, NPM_CHANGES_URL)
    }

    /// Creates a crawler against another endpoint, primarily for tests.
    pub fn with_endpoint(client: PoliteClient, endpoint: &str) -> Self {
        let package_endpoint = if endpoint.starts_with("https://replicate.npmjs.com/") {
            NPM_PACKAGE_URL.to_string()
        } else {
            endpoint
                .split("/_changes")
                .next()
                .unwrap_or(endpoint)
                .trim_end_matches('/')
                .to_string()
                + "/"
        };
        Self::with_endpoints(client, endpoint, &package_endpoint)
    }

    /// Creates a crawler with explicit feed and package metadata endpoints.
    pub fn with_endpoints(client: PoliteClient, endpoint: &str, package_endpoint: &str) -> Self {
        Self {
            client,
            endpoint: endpoint.to_string(),
            package_endpoint: package_endpoint.trim_end_matches('/').to_string(),
            page_limit: PAGE_LIMIT,
            max_pages: 50,
        }
    }

    /// Sets the per-request page size. The production default is 100.
    pub fn with_page_limit(mut self, page_limit: usize) -> Self {
        self.page_limit = page_limit.max(1);
        self
    }

    /// Caps pages fetched in one poll; remaining changes resume next time.
    pub fn with_max_pages(mut self, max_pages: usize) -> Self {
        self.max_pages = max_pages.max(1);
        self
    }

    fn poll_changes(
        &self,
        since: &Checkpoint,
    ) -> Result<(Vec<PackageEvent>, Checkpoint), NpmError> {
        let mut cursor = since.since.unwrap_or(0);
        let mut events = Vec::new();
        let mut pages = 0;
        loop {
            let url = format!("{}?since={cursor}&limit={}", self.endpoint, self.page_limit);
            let page: ChangesPage =
                self.client
                    .get_json_with_header(&url, NPM_OPT_IN_HEADER, "true")?;
            let page_len = page.results.len();
            pages += 1;
            for change in page.results {
                let name = change.id.clone();
                let is_new_package = change
                    .changes
                    .iter()
                    .any(|revision| revision.rev.starts_with("1-"));
                if let Some(document) = self.fetch_package(&name)? {
                    events.extend(events_for_change(name, document, is_new_package));
                }
            }
            if page_len == self.page_limit && page.last_seq <= cursor {
                return Err(NpmError::Malformed);
            }
            cursor = page.last_seq;
            if page_len < self.page_limit || pages >= self.max_pages {
                break;
            }
        }
        Ok((events, Checkpoint::new(cursor)))
    }

    fn fetch_package(&self, name: &str) -> Result<Option<PackageDocument>, NpmError> {
        let mut url = reqwest::Url::parse(&format!("{}/", self.package_endpoint))
            .map_err(|_| NpmError::Malformed)?;
        url.path_segments_mut()
            .map_err(|_| NpmError::Malformed)?
            .push(name);
        match self.client.get_json(url.as_str()) {
            Ok(document) => Ok(Some(document)),
            Err(NetError::Status(status)) if status == reqwest::StatusCode::NOT_FOUND => Ok(None),
            Err(error) => Err(NpmError::from(error)),
        }
    }
}

impl Crawler for NpmCrawler {
    fn name(&self) -> &str {
        "npm"
    }

    fn poll(
        &mut self,
        since: &Checkpoint,
    ) -> Result<(Vec<PackageEvent>, Checkpoint), Box<dyn std::error::Error + Send + Sync>> {
        self.poll_changes(since)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
    }
}

fn events_for_change(
    name: String,
    doc: PackageDocument,
    is_new_package: bool,
) -> Vec<PackageEvent> {
    let Some(times) = doc.time else {
        return Vec::new();
    };
    let modified = times.get("modified").and_then(serde_json::Value::as_str);
    let created = times.get("created").and_then(serde_json::Value::as_str);
    let mut events = Vec::new();
    if is_new_package {
        events.push(PackageEvent {
            ecosystem: Ecosystem::Npm,
            name: name.clone(),
            version: None,
            published_at: created.map(str::to_string),
            kind: PackageEventKind::NewPackage,
        });
    }

    if let Some((version, published_at)) = times
        .iter()
        .filter(|(key, value)| {
            key.as_str() != "created" && key.as_str() != "modified" && value.as_str().is_some()
        })
        .max_by(|a, b| a.1.as_str().cmp(&b.1.as_str()))
    {
        let published_at = published_at
            .as_str()
            .expect("filtered to timestamp strings");
        if is_new_package || modified == Some(published_at) {
            events.push(PackageEvent {
                ecosystem: Ecosystem::Npm,
                name,
                version: Some(version.clone()),
                published_at: Some(published_at.to_string()),
                kind: PackageEventKind::NewVersion,
            });
        }
    }
    events
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

    #[test]
    fn polls_pages_and_resumes_from_last_sequence() {
        let mut server = mockito::Server::new();
        let first = server
            .mock("GET", "/_changes")
            .match_header(NPM_OPT_IN_HEADER, "true")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("since".into(), "0".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "1".into()),
            ]))
            .with_body(
                r#"{"results":[{"seq":1,"id":"alpha","changes":[{"rev":"1-a"}]}],"last_seq":1}"#,
            )
            .expect(1)
            .create();
        let second = server
            .mock("GET", "/_changes")
            .match_header(NPM_OPT_IN_HEADER, "true")
            .match_query(mockito::Matcher::UrlEncoded("since".into(), "1".into()))
            .with_body(
                r#"{"results":[{"seq":2,"id":"beta","changes":[{"rev":"1-b"}]}],"last_seq":2}"#,
            )
            .expect(1)
            .create();
        let third = server
            .mock("GET", "/_changes")
            .match_header(NPM_OPT_IN_HEADER, "true")
            .match_query(mockito::Matcher::UrlEncoded("since".into(), "2".into()))
            .with_body(
                r#"{"results":[{"seq":3,"id":"gone","changes":[{"rev":"2-c"}]}],"last_seq":3}"#,
            )
            .expect(1)
            .create();
        let fourth = server
            .mock("GET", "/_changes")
            .match_header(NPM_OPT_IN_HEADER, "true")
            .match_query(mockito::Matcher::UrlEncoded("since".into(), "3".into()))
            .with_body(r#"{"results":[],"last_seq":3}"#)
            .expect(1)
            .create();
        let alpha = server.mock("GET", "/alpha")
            .with_body(r#"{"name":"alpha","time":{"created":"2025-01-01T00:00:00Z","modified":"2025-01-01T00:00:00Z","1.0.0":"2025-01-01T00:00:00Z"}}"#)
            .expect(1).create();
        let beta = server.mock("GET", "/beta")
            .with_body(r#"{"name":"beta","time":{"created":"2025-01-02T00:00:00Z","modified":"2025-01-02T00:00:00Z","2.0.0":"2025-01-02T00:00:00Z"}}"#)
            .expect(1).create();
        let gone = server
            .mock("GET", "/gone")
            .with_status(404)
            .expect(1)
            .create();
        let crawler = NpmCrawler::with_endpoints(
            fast_client(),
            &format!("{}/_changes", server.url()),
            &server.url(),
        )
        .with_page_limit(1);
        let (events, checkpoint) = crawler.poll_changes(&Checkpoint::default()).unwrap();
        assert_eq!(checkpoint.since, Some(3));
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].kind, PackageEventKind::NewPackage);
        assert_eq!(events[1].version.as_deref(), Some("1.0.0"));
        assert_eq!(events[2].name, "beta");
        assert_eq!(events[3].kind, PackageEventKind::NewVersion);
        first.assert();
        second.assert();
        third.assert();
        fourth.assert();
        alpha.assert();
        beta.assert();
        gone.assert();
    }

    #[test]
    fn existing_package_change_emits_latest_version() {
        let body = r#"{"name":"alpha","time":{"created":"2025-01-01T00:00:00Z","modified":"2025-02-01T00:00:00Z","1.0.0":"2025-01-01T00:00:00Z","1.1.0":"2025-02-01T00:00:00Z","unpublished":{"time":"2025-03-01T00:00:00Z"}}}"#;
        let doc: PackageDocument = serde_json::from_str(body).unwrap();
        let events = events_for_change("alpha".into(), doc, false);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, PackageEventKind::NewVersion);
        assert_eq!(events[0].version.as_deref(), Some("1.1.0"));
    }
}
