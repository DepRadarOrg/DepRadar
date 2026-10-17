//! On-disk cache of OSV query responses, keyed by
//! `(ecosystem, name, version)` and expired after a TTL.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::osv_client::{Ecosystem, OsvQueryResponse};

/// Default time-to-live for a cached response.
pub const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Serialize, Deserialize)]
struct Entry {
    stored_at_secs: u64,
    response: OsvQueryResponse,
}

/// A directory of JSON files, one per cached query.
pub struct OsvCache {
    dir: PathBuf,
    ttl: Duration,
}

impl OsvCache {
    pub fn new(dir: PathBuf, ttl: Duration) -> Self {
        Self { dir, ttl }
    }

    /// The default cache: `<OS cache dir>/depradar/osv/` with a 24h TTL, or
    /// `None` if the OS has no cache directory.
    pub fn default_location() -> Option<Self> {
        let dir = dirs::cache_dir()?.join("depradar").join("osv");
        Some(Self::new(dir, DEFAULT_TTL))
    }

    fn path(&self, ecosystem: Ecosystem, name: &str, version: &str) -> PathBuf {
        let key = format!("{}__{name}__{version}", ecosystem.as_osv_str());
        let file: String = key
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect();
        self.dir.join(format!("{file}.json"))
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    /// The cached response, if present, readable and younger than the TTL.
    pub fn get(&self, ecosystem: Ecosystem, name: &str, version: &str) -> Option<OsvQueryResponse> {
        let bytes = fs::read(self.path(ecosystem, name, version)).ok()?;
        let entry: Entry = serde_json::from_slice(&bytes).ok()?;
        let age = Self::now_secs().saturating_sub(entry.stored_at_secs);
        (age < self.ttl.as_secs()).then_some(entry.response)
    }

    /// Stores `response`. Failures are ignored: a cache that can't be
    /// written just means the next query goes to the network.
    pub fn put(
        &self,
        ecosystem: Ecosystem,
        name: &str,
        version: &str,
        response: &OsvQueryResponse,
    ) {
        let entry = Entry {
            stored_at_secs: Self::now_secs(),
            response: response.clone(),
        };
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        if let Ok(bytes) = serde_json::to_vec(&entry) {
            let _ = fs::write(self.path(ecosystem, name, version), bytes);
        }
    }
}
