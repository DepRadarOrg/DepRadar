//! Maintainer-change detector for popular packages.
//!
//! Account takeovers (the `ua-parser-js` and `event-stream` incidents) often
//! manifest as a new maintainer appearing on an established package. This
//! module tracks the set of maintainers associated with each version of a
//! popular package and emits a [`MaintainerChanged`] finding the first time a
//! version adds a maintainer that was not present in any previous version.
//!
//! State is persisted to a JSON file between runs via [`MaintainerStore`] so
//! the detector survives restarts.
//!
//! # Example
//!
//! ```no_run
//! use deprader::monitor::maintainers::{MaintainerStore, check_maintainers};
//! use std::collections::HashSet;
//!
//! let mut store = MaintainerStore::open("/var/lib/deprader/maintainers.json").unwrap();
//! let new_maintainers: HashSet<String> =
//!     ["alice", "bob", "eve"].iter().map(|s| s.to_string()).collect();
//!
//! let findings = check_maintainers(&mut store, "lodash", "4.18.0", &new_maintainers);
//! for f in findings {
//!     println!("{f:?}");
//! }
//! store.save().unwrap();
//! ```

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A finding emitted when a new version of a package introduces a maintainer
/// that was not present in any previously recorded version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintainerChanged {
    /// The affected package name.
    pub package: String,
    /// The version that introduced the new maintainer(s).
    pub version: String,
    /// Maintainer handles that appear for the first time in `version`.
    pub new_maintainers: Vec<String>,
}

/// Persistent store of `package → (version → maintainer_set)`.
#[derive(Debug, Default)]
pub struct MaintainerStore {
    path: PathBuf,
    /// `package_name → { version → set_of_maintainer_handles }`
    data: HashMap<String, HashMap<String, HashSet<String>>>,
}

/// On-disk JSON shape.
#[derive(Debug, Default, Serialize, Deserialize)]
struct OnDisk {
    #[serde(flatten)]
    packages: HashMap<String, HashMap<String, HashSet<String>>>,
}

impl MaintainerStore {
    /// Opens (or creates) the store at `path`.
    ///
    /// A missing file starts the store empty; a file that exists but is
    /// unreadable or malformed returns an error.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let data = match fs::read_to_string(&path) {
            Ok(contents) => {
                let on_disk: OnDisk = serde_json::from_str(&contents).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("malformed maintainer store: {e}"),
                    )
                })?;
                on_disk.packages
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => return Err(e),
        };
        Ok(Self { path, data })
    }

    /// Returns every maintainer handle ever seen across all recorded versions
    /// of `package`, or an empty set if the package is new.
    pub fn known_maintainers(&self, package: &str) -> HashSet<&str> {
        self.data
            .get(package)
            .map(|versions| versions.values().flat_map(|s| s.iter().map(String::as_str)).collect())
            .unwrap_or_default()
    }

    /// Records `maintainers` for `(package, version)`.
    pub fn record(&mut self, package: &str, version: &str, maintainers: HashSet<String>) {
        self.data
            .entry(package.to_string())
            .or_default()
            .insert(version.to_string(), maintainers);
    }

    /// Atomically flushes the in-memory state to disk.
    pub fn save(&self) -> io::Result<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let tmp = self.path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&OnDisk {
            packages: self.data.clone(),
        })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        fs::write(&tmp, json)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

/// Checks whether `version` of `package` introduces any maintainers that have
/// never been seen before, updates the store, and returns findings.
///
/// Callers should call [`MaintainerStore::save`] after processing a batch to
/// avoid write amplification.
pub fn check_maintainers(
    store: &mut MaintainerStore,
    package: &str,
    version: &str,
    maintainers: &HashSet<String>,
) -> Vec<MaintainerChanged> {
    let known = store.known_maintainers(package);
    let new: Vec<String> = maintainers
        .iter()
        .filter(|m| !known.contains(m.as_str()))
        .cloned()
        .collect();

    // Always record, even if nothing is new.
    store.record(package, version, maintainers.clone());

    if new.is_empty() || known.is_empty() {
        // If `known` is empty this is the first time we see this package —
        // no baseline to compare against, so no finding.
        return Vec::new();
    }

    let mut sorted_new = new;
    sorted_new.sort();
    vec![MaintainerChanged {
        package: package.to_string(),
        version: version.to_string(),
        new_maintainers: sorted_new,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Two snapshots: same maintainers → no finding.
    #[test]
    fn no_finding_when_maintainers_unchanged() {
        let dir = tempdir().unwrap();
        let mut store = MaintainerStore::open(dir.path().join("m.json")).unwrap();

        // First version establishes baseline.
        check_maintainers(&mut store, "lodash", "4.17.0", &set(&["john"]));
        // Second version — same maintainer.
        let findings = check_maintainers(&mut store, "lodash", "4.17.21", &set(&["john"]));
        assert!(findings.is_empty(), "no new maintainer → no finding");
    }

    /// Two snapshots: new maintainer added → one finding.
    #[test]
    fn finding_when_new_maintainer_appears() {
        let dir = tempdir().unwrap();
        let mut store = MaintainerStore::open(dir.path().join("m.json")).unwrap();

        check_maintainers(&mut store, "lodash", "4.17.0", &set(&["john"]));
        let findings =
            check_maintainers(&mut store, "lodash", "4.17.21", &set(&["john", "eve"]));

        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.package, "lodash");
        assert_eq!(f.version, "4.17.21");
        assert_eq!(f.new_maintainers, vec!["eve"]);
    }

    /// First time we see a package — no baseline, so no finding.
    #[test]
    fn no_finding_on_first_observation() {
        let dir = tempdir().unwrap();
        let mut store = MaintainerStore::open(dir.path().join("m.json")).unwrap();
        let findings = check_maintainers(&mut store, "newpkg", "1.0.0", &set(&["alice"]));
        assert!(findings.is_empty(), "no baseline means no finding");
    }

    /// Store round-trips through disk correctly.
    #[test]
    fn store_persists_across_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("m.json");

        {
            let mut store = MaintainerStore::open(&path).unwrap();
            check_maintainers(&mut store, "express", "4.18.0", &set(&["tj"]));
            store.save().unwrap();
        }

        // Re-open and check that the baseline is still there.
        let mut store2 = MaintainerStore::open(&path).unwrap();
        let findings = check_maintainers(&mut store2, "express", "4.18.1", &set(&["tj", "evil"]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].new_maintainers, vec!["evil"]);
    }
}
