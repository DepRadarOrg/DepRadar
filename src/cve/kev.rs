//! CISA Known Exploited Vulnerabilities (KEV) feed support.
//!
//! This module downloads and caches the CISA KEV JSON feed and provides
//! matching against CVE aliases so that findings which are known to be
//! exploited in the wild can be flagged in reports.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::Deserialize;

/// URL of the CISA KEV JSON feed.
pub const KEV_FEED_URL: &str =
    "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";

/// Default cache file name used when persisting the KEV feed.
pub const KEV_CACHE_FILE: &str = "known_exploited_vulnerabilities.json";

/// How long a cached KEV feed is considered fresh before re-downloading.
pub const KEV_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Top-level structure of the CISA KEV JSON feed.
#[derive(Debug, Deserialize)]
struct KevFeed {
    #[serde(default)]
    vulnerabilities: Vec<KevEntry>,
}

/// A single entry in the CISA KEV feed.
#[derive(Debug, Deserialize)]
struct KevEntry {
    #[serde(default)]
    cve_id: String,
}

/// Set of CVE identifiers that are known to be exploited in the wild.
#[derive(Debug, Default, Clone)]
pub struct KevCatalog {
    cve_ids: HashSet<String>,
}

impl KevCatalog {
    /// Create an empty catalog.
    pub fn new() -> Self {
        Self {
            cve_ids: HashSet::new(),
        }
    }

    /// Build a catalog from the raw JSON contents of a KEV feed.
    pub fn from_json(json: &str) -> Result<Self> {
        let feed: KevFeed =
            serde_json::from_str(json).context("failed to parse CISA KEV feed JSON")?;

        let cve_ids = feed
            .vulnerabilities
            .into_iter()
            .map(|entry| entry.cve_id.trim().to_ascii_uppercase())
            .filter(|id| !id.is_empty())
            .collect();

        Ok(Self { cve_ids })
    }

    /// Number of known-exploited CVE identifiers in the catalog.
    pub fn len(&self) -> usize {
        self.cve_ids.len()
    }

    /// Returns `true` if the catalog contains no entries.
    pub fn is_empty(&self) -> bool {
        self.cve_ids.is_empty()
    }

    /// Returns `true` if the given CVE ID is in the catalog.
    pub fn contains(&self, cve_id: &str) -> bool {
        self.cve_ids.contains(&cve_id.trim().to_ascii_uppercase())
    }

    /// Returns `true` if any of the provided aliases matches a known-exploited CVE.
    pub fn matches_aliases(&self, aliases: &[String]) -> bool {
        aliases.iter().any(|alias| self.contains(alias))
    }

    /// Load the KEV catalog from the on-disk cache, downloading it if the
    /// cache is missing or stale.
    ///
    /// `cache_dir` is the directory in which the feed is cached.
    pub fn load(cache_dir: &Path) -> Result<Self> {
        let cache_path = cache_dir.join(KEV_CACHE_FILE);

        if is_cache_fresh(&cache_path) {
            if let Ok(contents) = fs::read_to_string(&cache_path) {
                if let Ok(catalog) = Self::from_json(&contents) {
                    return Ok(catalog);
                }
            }
        }

        match Self::fetch(cache_dir) {
            Ok(catalog) => Ok(catalog),
            Err(err) => {
                // Fall back to a stale cache if a download fails.
                if let Ok(contents) = fs::read_to_string(&cache_path) {
                    if let Ok(catalog) = Self::from_json(&contents) {
                        return Ok(catalog);
                    }
                }
                Err(err)
            }
        }
    }

    /// Download the KEV feed, persist it to the cache directory, and return
    /// the parsed catalog.
    pub fn fetch(cache_dir: &Path) -> Result<Self> {
        let body = download(KEV_FEED_URL)?;
        let catalog = Self::from_json(&body)?;

        // Best-effort cache write; a failure to cache should not fail the load.
        let _ = write_cache(cache_dir, &body);

        Ok(catalog)
    }
}

/// Returns `true` if the cache file exists and is younger than the TTL.
fn is_cache_fresh(path: &Path) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };

    let modified = match metadata.modified() {
        Ok(modified) => modified,
        Err(_) => return false,
    };

    match SystemTime::now().duration_since(modified) {
        Ok(age) => age < KEV_CACHE_TTL,
        Err(_) => false,
    }
}

/// Write the raw feed body to the cache directory.
fn write_cache(cache_dir: &Path, body: &str) -> Result<()> {
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create cache dir {}", cache_dir.display()))?;

    let cache_path = cache_dir.join(KEV_CACHE_FILE);
    let tmp_path = cache_path.with_extension("json.tmp");

    {
        let mut file = fs::File::create(&tmp_path)
            .with_context(|| format!("failed to create {}", tmp_path.display()))?;
        file.write_all(body.as_bytes())
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        file.sync_all().ok();
    }

    fs::rename(&tmp_path, &cache_path)
        .with_context(|| format!("failed to move cache into place at {}", cache_path.display()))?;

    Ok(())
}

/// Download the given URL and return the response body as a string.
fn download(url: &str) -> Result<String> {
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("failed to download CISA KEV feed from {url}"))?;

    response
        .into_string()
        .context("failed to read CISA KEV feed response body")
}

/// Returns the default cache path for the KEV feed within `cache_dir`.
pub fn default_cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(KEV_CACHE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
    {
        "title": "CISA Catalog of Known Exploited Vulnerabilities",
        "catalogVersion": "2024.01.01",
        "dateReleased": "2024-01-01T00:00:00.000Z",
        "count": 3,
        "vulnerabilities": [
            {
                "cveID": "CVE-2021-44228",
                "vendorProject": "Apache",
                "product": "Log4j2",
                "vulnerabilityName": "Apache Log4j2 Remote Code Execution Vulnerability",
                "dateAdded": "2021-12-10",
                "shortDescription": "Apache Log4j2 contains a vulnerability...",
                "requiredAction": "Apply updates per vendor instructions.",
                "dueDate": "2021-12-24",
                "knownRansomwareCampaignUse": "Known",
                "notes": ""
            },
            {
                "cveID": "CVE-2022-22965",
                "vendorProject": "VMware",
                "product": "Spring Framework",
                "vulnerabilityName": "Spring Framework Remote Code Execution Vulnerability",
                "dateAdded": "2022-04-04",
                "shortDescription": "Spring Framework contains a vulnerability...",
                "requiredAction": "Apply updates per vendor instructions.",
                "dueDate": "2022-04-25",
                "knownRansomwareCampaignUse": "Unknown",
                "notes": ""
            },
            {
                "cveID": "CVE-2023-4863",
                "vendorProject": "Google",
                "product": "Chromium",
                "vulnerabilityName": "Google Chromium Heap Buffer Overflow Vulnerability",
                "dateAdded": "2023-09-13",
                "shortDescription": "Google Chromium contains a heap buffer overflow...",
                "requiredAction": "Apply updates per vendor instructions.",
                "dueDate": "2023-10-04",
                "knownRansomwareCampaignUse": "Unknown",
                "notes": ""
            }
        ]
    }
    "#;

    #[test]
    fn parses_fixture_and_flags_matching_alias() {
        let catalog = KevCatalog::from_json(FIXTURE).expect("fixture should parse");

        assert_eq!(catalog.len(), 3);
        assert!(!catalog.is_empty());

        // The right finding is flagged.
        assert!(catalog.matches_aliases(&["CVE-2021-44228".to_string()]));
        assert!(catalog.matches_aliases(&[
            "GHSA-jfh8-c2jp-5v3q".to_string(),
            "CVE-2021-44228".to_string(),
        ]));

        // Non-exploited CVEs are not flagged.
        assert!(!catalog.matches_aliases(&["CVE-2020-0001".to_string()]));
        assert!(!catalog.matches_aliases(&[]));
    }

    #[test]
    fn matching_is_case_insensitive() {
        let catalog = KevCatalog::from_json(FIXTURE).expect("fixture should parse");
        assert!(catalog.matches_aliases(&["cve-2022-22965".to_string()]));
    }

    #[test]
    fn empty_feed_yields_empty_catalog() {
        let catalog = KevCatalog::from_json(r#"{"vulnerabilities": []}"#)
            .expect("empty feed should parse");
        assert!(catalog.is_empty());
        assert!(!catalog.matches_aliases(&["CVE-2021-44228".to_string()]));
    }
}