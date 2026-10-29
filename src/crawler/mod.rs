//! Crawlers that watch package registries for newly published packages.
//!
//! Every crawler follows the same cursor and event contract. Network requests
//! go through [`crate::net::PoliteClient`] so registry rate limits are shared.
//!
//! * [`pypi`] — PyPI's RSS feeds of new projects and updates (#91).
//! * [`crates`] — crates.io's "newest crates" API (#92).

use std::collections::VecDeque;

use crate::model::Ecosystem;

pub mod crates;
pub mod npm;
pub mod pypi;

/// Cursor returned by one poll and supplied to the next poll.
///
/// A crawler must return a checkpoint that covers every event in that poll.
/// Callers should persist it only after processing those events successfully;
/// the next call passes that saved value as `since`. `None` means start at the
/// registry's initial cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Checkpoint {
    /// Registry sequence number, used by feeds such as npm's CouchDB changes.
    pub since: Option<u64>,
}

impl Checkpoint {
    /// Creates a checkpoint at a registry sequence number.
    pub fn new(since: u64) -> Self {
        Self { since: Some(since) }
    }
}

/// Whether a feed event represents a new package or a release of an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageEventKind {
    /// The package name appeared in the registry for the first time.
    NewPackage,
    /// An additional version was published for an existing package.
    NewVersion,
}

/// A package publication discovered by a registry crawler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEvent {
    /// Registry ecosystem.
    pub ecosystem: Ecosystem,
    /// Registry package name.
    pub name: String,
    /// Published version, when the event is a version release.
    pub version: Option<String>,
    /// Registry publication timestamp, in its original RFC 3339 form.
    pub published_at: Option<String>,
    /// Publication category.
    pub kind: PackageEventKind,
}

/// Registry crawler that resumes from a checkpoint and reports new events.
pub trait Crawler {
    /// Stable identifier used when storing this crawler's checkpoint.
    fn name(&self) -> &str;

    /// Fetches events after `since`, returning a cursor that covers them.
    ///
    /// If polling fails, callers must keep the old checkpoint and retry so no
    /// events are skipped. The returned checkpoint becomes the next `since`
    /// only after the caller has handled every returned event.
    fn poll(
        &mut self,
        since: &Checkpoint,
    ) -> Result<(Vec<PackageEvent>, Checkpoint), Box<dyn std::error::Error + Send + Sync>>;
}

/// In-memory crawler used to exercise checkpoint handling without a registry.
#[derive(Debug, Default)]
pub struct FakeCrawler {
    events: VecDeque<PackageEvent>,
    sequence: u64,
}

impl FakeCrawler {
    /// Creates a crawler that emits the supplied events once, in order.
    pub fn new(events: impl IntoIterator<Item = PackageEvent>) -> Self {
        Self {
            events: events.into_iter().collect(),
            sequence: 0,
        }
    }
}

impl Crawler for FakeCrawler {
    fn name(&self) -> &str {
        "fake"
    }

    fn poll(
        &mut self,
        since: &Checkpoint,
    ) -> Result<(Vec<PackageEvent>, Checkpoint), Box<dyn std::error::Error + Send + Sync>> {
        let mut sequence = since.since.unwrap_or(self.sequence);
        let events: Vec<_> = self.events.drain(..).collect();
        sequence = sequence.saturating_add(u64::try_from(events.len()).unwrap_or(u64::MAX));
        self.sequence = sequence;
        Ok((events, Checkpoint::new(sequence)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &str) -> PackageEvent {
        PackageEvent {
            ecosystem: Ecosystem::Npm,
            name: name.into(),
            version: None,
            published_at: None,
            kind: PackageEventKind::NewPackage,
        }
    }

    #[test]
    fn fake_crawler_emits_events_once_and_advances_checkpoint() {
        let mut crawler = FakeCrawler::new([event("alpha"), event("beta")]);
        assert_eq!(crawler.name(), "fake");
        let (events, checkpoint) = crawler.poll(&Checkpoint::default()).unwrap();
        assert_eq!(
            events.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["alpha", "beta"]
        );
        assert_eq!(checkpoint.since, Some(2));
        let (events, next) = crawler.poll(&checkpoint).unwrap();
        assert!(events.is_empty());
        assert_eq!(next, checkpoint);
    }
}
