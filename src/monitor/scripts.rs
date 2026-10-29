//! Install-script watcher for popular npm packages.
//!
//! A popular package suddenly gaining a `preinstall`, `install`, or
//! `postinstall` lifecycle script is one of the strongest compromise signals
//! in the npm ecosystem (e.g. the `ua-parser-js` account takeover). This
//! module compares the `scripts` object of two consecutive `package.json`
//! snapshots and emits an [`InstallScriptAdded`] finding when a dangerous
//! lifecycle hook appears or changes.
//!
//! State between runs is persisted by [`ScriptStore`], which keeps the last
//! known `scripts` map for each tracked package version.
//!
//! # Example
//!
//! ```no_run
//! use deprader::monitor::scripts::{ScriptStore, check_install_scripts};
//! use std::collections::HashMap;
//!
//! let mut store = ScriptStore::open("/var/lib/deprader/scripts.json").unwrap();
//! let mut scripts = HashMap::new();
//! scripts.insert("postinstall".to_string(), "node malware.js".to_string());
//! let findings = check_install_scripts(&mut store, "popular-pkg", "1.2.0", &scripts);
//! store.save().unwrap();
//! ```

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The lifecycle hooks that indicate install-time code execution on npm.
pub const DANGEROUS_HOOKS: &[&str] = &["preinstall", "install", "postinstall"];

/// A finding emitted when a new version of a popular npm package gains or
/// changes a lifecycle install script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallScriptAdded {
    /// The affected package name.
    pub package: String,
    /// The version that introduced or changed the script.
    pub version: String,
    /// The lifecycle hook key (`preinstall`, `install`, or `postinstall`).
    pub hook: String,
    /// The new script value.
    pub script: String,
    /// The previous script value, or `None` if the hook is brand-new.
    pub previous: Option<String>,
}

/// Persistent store of `package → (version → scripts_map)`.
#[derive(Debug, Default)]
pub struct ScriptStore {
    path: PathBuf,
    /// `package_name → { version → { hook → script } }`
    data: HashMap<String, HashMap<String, HashMap<String, String>>>,
}

/// On-disk JSON shape.
#[derive(Debug, Default, Serialize, Deserialize)]
struct OnDisk {
    #[serde(flatten)]
    packages: HashMap<String, HashMap<String, HashMap<String, String>>>,
}

impl ScriptStore {
    /// Opens (or creates) the store at `path`.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let data = match fs::read_to_string(&path) {
            Ok(contents) => {
                let on_disk: OnDisk = serde_json::from_str(&contents).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("malformed script store: {e}"),
                    )
                })?;
                on_disk.packages
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => return Err(e),
        };
        Ok(Self { path, data })
    }

    /// Returns the last known scripts map for any version of `package`, or
    /// `None` if the package has never been recorded.
    ///
    /// When multiple versions exist the most recently inserted one is
    /// returned. For the purposes of change detection we only need *a*
    /// previous baseline, not a specific version.
    pub fn latest_scripts(&self, package: &str) -> Option<&HashMap<String, String>> {
        self.data.get(package)?.values().last()
    }

    /// Records `scripts` for `(package, version)`.
    pub fn record(
        &mut self,
        package: &str,
        version: &str,
        scripts: HashMap<String, String>,
    ) {
        self.data
            .entry(package.to_string())
            .or_default()
            .insert(version.to_string(), scripts);
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

/// Compares `scripts` for the new `version` of `package` against whatever
/// was recorded previously, emitting a finding for every dangerous lifecycle
/// hook that is new or changed.
///
/// If this is the first time the package is seen the scripts are recorded but
/// no finding is emitted (no baseline to compare against).
pub fn check_install_scripts(
    store: &mut ScriptStore,
    package: &str,
    version: &str,
    scripts: &HashMap<String, String>,
) -> Vec<InstallScriptAdded> {
    let previous: HashMap<String, String> = store
        .latest_scripts(package)
        .cloned()
        .unwrap_or_default();

    let is_first_observation = !store.data.contains_key(package);

    // Record the new version unconditionally.
    store.record(package, version, scripts.clone());

    if is_first_observation {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for hook in DANGEROUS_HOOKS {
        if let Some(new_script) = scripts.get(*hook) {
            let prev = previous.get(*hook);
            if prev.map(String::as_str) != Some(new_script.as_str()) {
                findings.push(InstallScriptAdded {
                    package: package.to_string(),
                    version: version.to_string(),
                    hook: hook.to_string(),
                    script: new_script.clone(),
                    previous: prev.cloned(),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn scripts(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// v1 has no install script, v2 adds `postinstall` → one finding.
    #[test]
    fn finding_when_postinstall_added() {
        let dir = tempdir().unwrap();
        let mut store = ScriptStore::open(dir.path().join("s.json")).unwrap();

        // v1: no dangerous scripts.
        check_install_scripts(&mut store, "ua-parser-js", "1.0.0", &scripts(&[("test", "jest")]));

        // v2: postinstall appears.
        let findings = check_install_scripts(
            &mut store,
            "ua-parser-js",
            "1.0.1",
            &scripts(&[("test", "jest"), ("postinstall", "node malware.js")]),
        );

        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.package, "ua-parser-js");
        assert_eq!(f.version, "1.0.1");
        assert_eq!(f.hook, "postinstall");
        assert_eq!(f.script, "node malware.js");
        assert_eq!(f.previous, None);
    }

    /// No change in scripts → no finding.
    #[test]
    fn no_finding_when_scripts_unchanged() {
        let dir = tempdir().unwrap();
        let mut store = ScriptStore::open(dir.path().join("s.json")).unwrap();

        let s = scripts(&[("test", "jest"), ("postinstall", "echo hi")]);
        check_install_scripts(&mut store, "mypkg", "1.0.0", &s);
        let findings = check_install_scripts(&mut store, "mypkg", "1.0.1", &s);
        assert!(findings.is_empty());
    }

    /// First observation → no finding (no baseline).
    #[test]
    fn no_finding_on_first_observation() {
        let dir = tempdir().unwrap();
        let mut store = ScriptStore::open(dir.path().join("s.json")).unwrap();
        let findings = check_install_scripts(
            &mut store,
            "newpkg",
            "1.0.0",
            &scripts(&[("postinstall", "node setup.js")]),
        );
        assert!(findings.is_empty(), "no baseline → no finding");
    }

    /// A changed (not just added) script also fires.
    #[test]
    fn finding_when_postinstall_changes() {
        let dir = tempdir().unwrap();
        let mut store = ScriptStore::open(dir.path().join("s.json")).unwrap();

        check_install_scripts(
            &mut store,
            "mypkg",
            "1.0.0",
            &scripts(&[("postinstall", "node setup.js")]),
        );
        let findings = check_install_scripts(
            &mut store,
            "mypkg",
            "1.0.1",
            &scripts(&[("postinstall", "node malware.js")]),
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].previous.as_deref(), Some("node setup.js"));
        assert_eq!(findings[0].script, "node malware.js");
    }

    /// Multiple hooks added at once → multiple findings.
    #[test]
    fn multiple_hooks_produce_multiple_findings() {
        let dir = tempdir().unwrap();
        let mut store = ScriptStore::open(dir.path().join("s.json")).unwrap();

        check_install_scripts(&mut store, "mypkg", "1.0.0", &scripts(&[]));
        let findings = check_install_scripts(
            &mut store,
            "mypkg",
            "1.0.1",
            &scripts(&[
                ("preinstall", "echo pre"),
                ("postinstall", "echo post"),
            ]),
        );
        assert_eq!(findings.len(), 2);
    }

    /// Store round-trips to disk.
    #[test]
    fn store_persists_across_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("s.json");

        {
            let mut store = ScriptStore::open(&path).unwrap();
            check_install_scripts(&mut store, "mypkg", "1.0.0", &scripts(&[("test", "jest")]));
            store.save().unwrap();
        }

        let mut store2 = ScriptStore::open(&path).unwrap();
        let findings = check_install_scripts(
            &mut store2,
            "mypkg",
            "1.0.1",
            &scripts(&[("test", "jest"), ("postinstall", "node evil.js")]),
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].hook, "postinstall");
    }
}
