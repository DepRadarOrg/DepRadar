//! On-disk cache of OSV query responses, keyed by
//! `(ecosystem, name, version)` and expired after a TTL.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::osv_client::{Ecosystem, OsvQueryResponse};

/// Default time-to-live for a cached response.
pub const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Environment variable that overrides the cache directory.
pub const CACHE_DIR_ENV: &str = "DEPRADAR_CACHE_DIR";

#[derive(Serialize, Deserialize)]
struct Entry {
    stored_at_secs: u64,
    response: OsvQueryResponse,
}

#[derive(Serialize, Deserialize)]
struct EpssEntry {
    stored_at_secs: u64,
    scores: BTreeMap<String, f64>,
}

/// Summary of the on-disk cache, as reported by `depradar cache info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheInfo {
    pub path: PathBuf,
    pub entries: usize,
    pub size_bytes: u64,
}

/// The base cache directory: `$DEPRADAR_CACHE_DIR` if set, otherwise
/// `<OS cache dir>/depradar`, or `None` if neither is available.
pub fn cache_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(CACHE_DIR_ENV) {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    Some(dirs::cache_dir()?.join("depradar"))
}

/// The OSV cache directory: `<cache root>/osv`.
pub fn osv_cache_dir() -> Option<PathBuf> {
    Some(cache_root()?.join("osv"))
}

/// The EPSS cache directory: `<cache root>/epss`.
pub fn epss_cache_dir() -> Option<PathBuf> {
    Some(cache_root()?.join("epss"))
}

/// Counts the regular files directly inside `dir` and sums their sizes.
/// A missing directory reports zero entries and zero bytes.
pub fn dir_stats(dir: &Path) -> (usize, u64) {
    let mut entries = 0usize;
    let mut size = 0u64;
    if let Ok(read) = fs::read_dir(dir) {
        for entry in read.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    entries += 1;
                    size += meta.len();
                }
            }
        }
    }
    (entries, size)
}

/// Reports the path, entry count and total size of the cache rooted at
/// `root` (both the OSV and EPSS subdirectories).
pub fn cache_info(root: &Path) -> CacheInfo {
    let mut entries = 0usize;
    let mut size_bytes = 0u64;
    for sub in ["osv", "epss"] {
        let (e, s) = dir_stats(&root.join(sub));
        entries += e;
        size_bytes += s;
    }
    CacheInfo {
        path: root.to_path_buf(),
        entries,
        size_bytes,
    }
}

/// Deletes the cache directory at `root`, if it exists. Returns whether
/// anything was removed.
pub fn clear_cache(root: &Path) -> std::io::Result<bool> {
    match fs::remove_dir_all(root) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
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

    /// The default cache: `<cache root>/osv/` with a 24h TTL, or `None` if
    /// no cache directory is available.
    pub fn default_location() -> Option<Self> {
        Some(Self::new(osv_cache_dir()?, DEFAULT_TTL))
    }

    /// The directory backing this cache.
    pub fn dir(&self) -> &Path {
        &self.dir
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

/// A directory of JSON files caching EPSS scores, keyed by CVE ID.
pub struct EpssCache {
    dir: PathBuf,
    ttl: Duration,
}

impl EpssCache {
    pub fn new(dir: PathBuf, ttl: Duration) -> Self {
        Self { dir, ttl }
    }

    /// The default cache: `<cache root>/epss/` with a 24h TTL, or `None`
    /// if no cache directory is available.
    pub fn default_location() -> Option<Self> {
        Some(Self::new(epss_cache_dir()?, DEFAULT_TTL))
    }

    /// The directory backing this cache.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, cve: &str) -> PathBuf {
        let file: String = cve
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

    /// The cached scores for `cve`, if present, readable and younger than
    /// the TTL.
    pub fn get(&self, cve: &str) -> Option<BTreeMap<String, f64>> {
        let bytes = fs::read(self.path(cve)).ok()?;
        let entry: EpssEntry = serde_json::from_slice(&bytes).ok()?;
        let age = Self::now_secs().saturating_sub(entry.stored_at_secs);
        (age < self.ttl.as_secs()).then_some(entry.scores)
    }

    /// Stores `scores`. Failures are ignored: a cache that can't be
    /// written just means the next lookup goes to the network.
    pub fn put(&self, cve: &str, scores: &BTreeMap<String, f64>) {
        let entry = EpssEntry {
            stored_at_secs: Self::now_secs(),
            scores: scores.clone(),
        };
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        if let Ok(bytes) = serde_json::to_vec(&entry) {
            let _ = fs::write(self.path(cve), bytes);
        }
    }
}
