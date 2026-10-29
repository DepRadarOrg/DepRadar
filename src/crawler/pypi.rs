//! PyPI crawler: newly published projects and newly uploaded versions,
//! from PyPI's RSS feeds (#91).
//!
//! PyPI publishes two RSS 2.0 feeds:
//!
//! * <https://pypi.org/rss/packages.xml> — new projects,
//! * <https://pypi.org/rss/updates.xml> — new releases of existing projects.
//!
//! Both are newest-first lists of `<item>`s carrying a stable `<guid>` (the
//! project/release URL), a `<title>`, a `<link>` and an RFC 822
//! `<pubDate>`. The feeds only hold the most recent few hundred entries, so
//! "what have I already seen" is a [`Checkpoint`]: the GUID and publish time
//! of the newest item we processed last poll. [`PypiCrawler::poll`] yields
//! only the items newer than the checkpoint and then advances it, so
//! re-polling an unchanged feed yields nothing.
//!
//! Parsing is a deliberately small hand-rolled extractor rather than an XML
//! library: the feed shape is fixed and tiny, and the crawler already has
//! plenty of dependencies through `reqwest`.

use crate::net::{NetError, PoliteClient};

/// Base URL of the live PyPI site.
pub const PYPI_BASE_URL: &str = "https://pypi.org";

/// Errors from [`PypiCrawler`].
#[derive(Debug)]
pub enum PypiError {
    /// The HTTP request failed (or came back non-2xx).
    Net(NetError),
    /// The response body wasn't an RSS feed we recognise.
    MalformedFeed,
}

impl std::fmt::Display for PypiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PypiError::Net(err) => write!(f, "PyPI request failed: {err}"),
            PypiError::MalformedFeed => write!(f, "PyPI response is not a recognisable RSS feed"),
        }
    }
}

impl std::error::Error for PypiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PypiError::Net(err) => Some(err),
            PypiError::MalformedFeed => None,
        }
    }
}

impl From<NetError> for PypiError {
    fn from(err: NetError) -> Self {
        PypiError::Net(err)
    }
}

/// One entry of either PyPI feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    /// Stable identity of the item, e.g. `https://pypi.org/project/requests/`.
    pub guid: String,
    /// Human-readable title, e.g. the project name or `name 1.2.3`.
    pub title: Option<String>,
    /// Canonical project URL.
    pub link: Option<String>,
    /// Raw RFC 822 `pubDate` as published.
    pub pub_date: Option<String>,
}

/// The publish time of a feed item, as epoch seconds.
///
/// PyPI emits `Wed, 15 Oct 2025 12:34:56 GMT`-style dates; we parse the
/// subset that format covers (always GMT) and convert to epoch seconds so
/// items can be ordered without a date library.
fn parse_pub_date(rfc822: &str) -> Option<i64> {
    let rest = rfc822.split_once(',').map_or(rfc822, |(_, rest)| rest);
    let mut words = rest.split_whitespace();
    let day: i64 = words.next()?.parse().ok()?;
    let month = month_number(words.next()?)?;
    let year: i64 = words.next()?.parse().ok()?;
    let mut hms = words.next()?.split(':');
    let hour: i64 = hms.next()?.parse().ok()?;
    let minute: i64 = hms.next()?.parse().ok()?;
    let second: i64 = hms.next()?.parse().ok()?;

    // Days since 1970-01-01, Howard Hinnant's days-from-civil algorithm:
    // shift the year so March starts it, count the era/day-of-era, and
    // subtract the days from 0000-03-01 to 1970-01-01 (719468).
    let month = month + 1; // back to 1-12, Jan = 1.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let march_month = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * march_month + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn month_number(name: &str) -> Option<i64> {
    Some(match name {
        "Jan" => 0,
        "Feb" => 1,
        "Mar" => 2,
        "Apr" => 3,
        "May" => 4,
        "Jun" => 5,
        "Jul" => 6,
        "Aug" => 7,
        "Sep" => 8,
        "Oct" => 9,
        "Nov" => 10,
        "Dec" => 11,
        _ => return None,
    })
}

/// Dedupe state for one feed: the newest item the crawler has already seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Checkpoint {
    guid: Option<String>,
    /// Epoch seconds; derived from the checkpoint item's `pubDate`.
    timestamp: Option<i64>,
}

impl Checkpoint {
    /// A checkpoint that has already seen the item with this GUID (or
    /// nothing, when `None`) — typically restored from a previous run.
    pub fn new(guid: Option<&str>, timestamp: Option<i64>) -> Self {
        Checkpoint {
            guid: guid.map(str::to_string),
            timestamp,
        }
    }

    /// GUID of the newest already-seen item, when any.
    pub fn last_guid(&self) -> Option<&str> {
        self.guid.as_deref()
    }

    /// Keeps only the items newer than this checkpoint.
    ///
    /// Feeds are newest-first, so this walks until the checkpoint GUID is
    /// reached (that item excluded) and returns everything before it. When
    /// the checkpoint GUID is missing from the feed (aged out, or a fresh
    /// checkpoint), the publish timestamps decide instead; items with no
    /// parseable date fall on the safe side and count as new.
    pub fn unseen<'a>(&self, items: &'a [FeedItem]) -> Vec<&'a FeedItem> {
        if let Some(guid) = &self.guid {
            if items.iter().any(|item| &item.guid == guid) {
                return items.iter().take_while(|item| &item.guid != guid).collect();
            }
        }
        match self.timestamp {
            None => items.iter().collect(),
            Some(seen) => items
                .iter()
                .filter(|item| {
                    item.pub_date
                        .as_deref()
                        .and_then(parse_pub_date)
                        .is_none_or(|at| at > seen)
                })
                .collect(),
        }
    }

    /// Moves the checkpoint to the newest of `newly_seen` (feed order: first).
    pub fn advance(&mut self, newly_seen: &[&FeedItem]) {
        let Some(newest) = newly_seen.first().copied() else {
            return;
        };
        self.guid = Some(newest.guid.clone());
        self.timestamp = newest.pub_date.as_deref().and_then(parse_pub_date);
    }
}

/// Which of PyPI's two feeds to poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PypiFeed {
    /// New projects: <https://pypi.org/rss/packages.xml>.
    NewPackages,
    /// New releases of existing projects: <https://pypi.org/rss/updates.xml>.
    Updates,
}

impl PypiFeed {
    fn path(self) -> &'static str {
        match self {
            PypiFeed::NewPackages => "/rss/packages.xml",
            PypiFeed::Updates => "/rss/updates.xml",
        }
    }
}

/// Text content of the first `<tag>` element inside `block`, attribute
/// strings on the start tag included, e.g. `<guid isPermaLink="false">…`.
fn tag_text(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = block.find(&open)?;
    let after_open = block[start + open.len()..].find('>')? + start + open.len() + 1;
    // Skip self-closing tags.
    if block.as_bytes().get(after_open.checked_sub(1)?) == Some(&b'/') {
        return None;
    }
    let close = format!("</{tag}>");
    let end = block[after_open..].find(&close)? + after_open;
    Some(unescape_xml(&block[after_open..end]))
}

/// Unescapes the five XML entities that can appear in package titles/URLs.
fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Parses one PyPI RSS 2.0 feed body into its items, in document order
/// (newest first).
///
/// A feed with no `<item>` at all parses to an empty list — PyPI serves an
/// empty feed rather than an error when nothing new arrived. Anything else
/// (an HTML error page, truncated XML) is still best-effort: items missing
/// a `<guid>` are skipped, since the guid is what the checkpoint dedupes on.
pub fn parse_feed(body: &str) -> Result<Vec<FeedItem>, PypiError> {
    let mut items = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("<item>") {
        let after = &rest[start + "<item>".len()..];
        let Some(len) = after.find("</item>") else {
            return Err(PypiError::MalformedFeed);
        };
        let block = &after[..len];
        rest = &after[len + "</item>".len()..];
        let Some(guid) = tag_text(block, "guid") else {
            continue;
        };
        items.push(FeedItem {
            guid,
            title: tag_text(block, "title"),
            link: tag_text(block, "link"),
            pub_date: tag_text(block, "pubDate"),
        });
    }
    if items.is_empty() && !body.contains("<item") && !body.contains("<?xml") {
        return Err(PypiError::MalformedFeed);
    }
    Ok(items)
}

/// Polls PyPI's RSS feeds, keeping a per-feed [`Checkpoint`] so that each
/// poll only reports entries the previous one didn't.
///
/// Requests go through [`PoliteClient`] (rate limit, User-Agent, timeout).
/// The crawler holds one checkpoint per feed, since the two feeds advance
/// independently.
pub struct PypiCrawler {
    client: PoliteClient,
    base_url: String,
    packages: Checkpoint,
    updates: Checkpoint,
}

impl Default for PypiCrawler {
    fn default() -> Self {
        Self::new(PoliteClient::default())
    }
}

impl PypiCrawler {
    /// A crawler for the live PyPI feeds.
    pub fn new(client: PoliteClient) -> Self {
        Self::with_base_url(client, PYPI_BASE_URL)
    }

    /// A crawler against another base URL, e.g. a mock server in tests.
    pub fn with_base_url(client: PoliteClient, base_url: &str) -> Self {
        PypiCrawler {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            packages: Checkpoint::default(),
            updates: Checkpoint::default(),
        }
    }

    /// Fetches `feed` and returns only the entries not covered by the
    /// checkpoint, then advances the checkpoint to the newest of them.
    ///
    /// An empty result is normal: nothing new since the last poll.
    pub fn poll(&mut self, feed: PypiFeed) -> Result<Vec<FeedItem>, PypiError> {
        let url = format!("{}{}", self.base_url, feed.path());
        let body = self.client.get_text(&url)?;
        let items = parse_feed(&body)?;

        let checkpoint = match feed {
            PypiFeed::NewPackages => &mut self.packages,
            PypiFeed::Updates => &mut self.updates,
        };
        let unseen = checkpoint.unseen(&items);
        let newly_seen: Vec<&FeedItem> = unseen.clone();
        checkpoint.advance(&newly_seen);
        Ok(newly_seen.into_iter().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Trimmed copy of the shape PyPI serves at /rss/packages.xml.
    const PACKAGES_FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>PyPI recent updates</title>
    <item>
      <title>brand-new-lib</title>
      <link>https://pypi.org/project/brand-new-lib/</link>
      <guid isPermaLink="false">https://pypi.org/project/brand-new-lib/</guid>
      <description>New project 'brand-new-lib' created</description>
      <pubDate>Wed, 15 Oct 2025 12:00:00 GMT</pubDate>
    </item>
    <item>
      <title>another-lib</title>
      <link>https://pypi.org/project/another-lib/</link>
      <guid isPermaLink="false">https://pypi.org/project/another-lib/</guid>
      <description>New project 'another-lib' created</description>
      <pubDate>Tue, 14 Oct 2025 09:30:00 GMT</pubDate>
    </item>
    <item>
      <title>old-lib</title>
      <link>https://pypi.org/project/old-lib/</link>
      <guid isPermaLink="false">https://pypi.org/project/old-lib/</guid>
      <description>New project 'old-lib' created</description>
      <pubDate>Mon, 13 Oct 2025 08:00:00 GMT</pubDate>
    </item>
  </channel>
</rss>"#;

    fn fast_client() -> PoliteClient {
        PoliteClient::new(
            Duration::from_millis(1),
            "DepRadar test agent",
            Duration::from_secs(5),
        )
        .unwrap()
    }

    #[test]
    fn parse_feed_reads_items_in_document_order() {
        let items = parse_feed(PACKAGES_FEED).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].guid, "https://pypi.org/project/brand-new-lib/");
        assert_eq!(items[0].title.as_deref(), Some("brand-new-lib"));
        assert_eq!(
            items[0].link.as_deref(),
            Some("https://pypi.org/project/brand-new-lib/")
        );
        assert_eq!(
            items[0].pub_date.as_deref(),
            Some("Wed, 15 Oct 2025 12:00:00 GMT")
        );
        assert_eq!(items[2].guid, "https://pypi.org/project/old-lib/");
    }

    #[test]
    fn parse_feed_unescapes_xml_entities() {
        let feed = r#"<rss><channel><item>
            <guid>a&amp;b</guid><title>&lt;sneaky&gt; &quot;name&quot;</title>
        </item></channel></rss>"#;
        let items = parse_feed(feed).unwrap();
        assert_eq!(items[0].guid, "a&b");
        assert_eq!(items[0].title.as_deref(), Some("<sneaky> \"name\""));
    }

    #[test]
    fn parse_feed_rejects_non_rss_bodies() {
        assert!(matches!(
            parse_feed("<html><body>500 Internal Server Error</body></html>"),
            Err(PypiError::MalformedFeed)
        ));
    }

    #[test]
    fn parse_feed_accepts_an_empty_feed() {
        let items =
            parse_feed(r#"<?xml version="1.0"?><rss><channel><title>x</title></channel></rss>"#)
                .unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn pub_dates_convert_to_comparable_epoch_seconds() {
        // Known value: 2025-10-15T12:00:00Z == 1760529600.
        assert_eq!(
            parse_pub_date("Wed, 15 Oct 2025 12:00:00 GMT"),
            Some(1_760_529_600)
        );
        // The Unix epoch itself.
        assert_eq!(parse_pub_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(parse_pub_date("not a date"), None);
    }
    #[test]
    fn checkpoint_filters_already_seen_items() {
        let items = parse_feed(PACKAGES_FEED).unwrap();
        let mut checkpoint = Checkpoint::default();
        let first_poll = checkpoint.unseen(&items);
        assert_eq!(first_poll.len(), 3);
        checkpoint.advance(&first_poll);
        assert_eq!(
            checkpoint.last_guid(),
            Some("https://pypi.org/project/brand-new-lib/")
        );

        // Nothing new: the same feed again yields nothing.
        assert!(checkpoint.unseen(&items).is_empty());

        // A genuinely new GUID on top of the feed is reported.
        let mut newer = items.clone();
        newer.insert(
            0,
            FeedItem {
                guid: "https://pypi.org/project/even-newer-lib/".to_string(),
                title: Some("even-newer-lib".to_string()),
                link: None,
                pub_date: Some("Fri, 17 Oct 2025 09:00:00 GMT".to_string()),
            },
        );
        let unseen = checkpoint.unseen(&newer);
        assert_eq!(unseen.len(), 1);
        assert_eq!(unseen[0].guid, "https://pypi.org/project/even-newer-lib/");
    }

    #[test]
    fn checkpoint_falls_back_to_timestamps_when_guid_aged_out() {
        let items = parse_feed(PACKAGES_FEED).unwrap();
        let mut checkpoint = Checkpoint::default();
        let first = checkpoint.unseen(&items);
        checkpoint.advance(&first);

        // The checkpoint GUID has aged out of the feed; timestamps decide.
        let mut aged: Vec<FeedItem> = items
            .iter()
            .filter(|i| i.guid != "https://pypi.org/project/brand-new-lib/")
            .cloned()
            .collect();
        aged.push(FeedItem {
            guid: "https://pypi.org/project/brand-newest/".to_string(),
            title: None,
            link: None,
            pub_date: Some("Thu, 16 Oct 2025 10:00:00 GMT".to_string()),
        });
        let unseen = checkpoint.unseen(&aged);
        assert_eq!(unseen.len(), 1);
        assert_eq!(unseen[0].guid, "https://pypi.org/project/brand-newest/");
    }

    /// The crawler polls a mock server twice; the second poll sees only the
    /// item that appeared in between.
    #[test]
    fn poll_dedupes_against_the_checkpoint() {
        let mut server = mockito::Server::new();
        let body_v2 = PACKAGES_FEED
            .replace(
                "Wed, 15 Oct 2025 12:00:00 GMT",
                "Fri, 17 Oct 2025 08:00:00 GMT",
            )
            .replace("brand-new-lib", "very-new-lib");
        let first = server
            .mock("GET", "/rss/packages.xml")
            .with_body(PACKAGES_FEED)
            .expect(1)
            .create();
        let second = server
            .mock("GET", "/rss/packages.xml")
            .with_body(body_v2)
            .expect(1)
            .create();

        let mut crawler = PypiCrawler::with_base_url(fast_client(), &server.url());

        let poll = crawler.poll(PypiFeed::NewPackages).unwrap();
        assert_eq!(poll.len(), 3, "first poll reports the whole feed");

        let poll = crawler.poll(PypiFeed::NewPackages).unwrap();
        assert_eq!(poll.len(), 1, "second poll reports only the new item");
        assert_eq!(poll[0].guid, "https://pypi.org/project/very-new-lib/");

        first.assert();
        second.assert();
    }

    /// Each feed keeps its own checkpoint.
    #[test]
    fn packages_and_updates_polls_do_not_share_checkpoints() {
        let mut server = mockito::Server::new();
        let packages = server
            .mock("GET", "/rss/packages.xml")
            .with_body(PACKAGES_FEED)
            .expect(1)
            .create();
        let updates = server
            .mock("GET", "/rss/updates.xml")
            .with_body(PACKAGES_FEED)
            .expect(1)
            .create();

        let mut crawler = PypiCrawler::with_base_url(fast_client(), &server.url());
        assert_eq!(crawler.poll(PypiFeed::NewPackages).unwrap().len(), 3);
        assert_eq!(crawler.poll(PypiFeed::Updates).unwrap().len(), 3);
        packages.assert();
        updates.assert();
    }
}
