//! Diffing two unpacked versions of a package for risky changes.
//!
//! A patch release that quietly adds an install script or network code is
//! one of the strongest signals in supply-chain security: nothing about the
//! package name changed, only what it does. [`diff_versions`] compares two
//! unpacked package trees (`old` = the version you have, `new` = the
//! candidate) and reports:
//!
//! - added and removed files,
//! - install scripts present in `new` but not `old` (a `setup.py`, or npm
//!   `preinstall`/`install`/`postinstall` entries),
//! - suspicious `setup.py` calls (obfuscation, network, command execution
//!   — see [`crate::behavioral::setup_py`]) that are new in `new`.
//!
//! Findings are compared by kind per file, not by line, so a version bump
//! that merely shifts line numbers doesn't produce noise.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::cve::severity::Severity;

use super::setup_py::scan_setup_py_in;
use super::{collect_files, BehavioralFinding};

/// Severity given to an install script that appears between two versions.
pub const NEW_SCRIPT_SEVERITY: Severity = Severity::Medium;

/// What changed between two unpacked package versions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageDiff {
    /// Files present in `new` but not `old`, sorted, `/`-separated.
    pub added_files: Vec<String>,
    /// Files present in `old` but not `new`, sorted, `/`-separated.
    pub removed_files: Vec<String>,
    /// Human-readable descriptions of install scripts in `new` that `old`
    /// didn't have, sorted — e.g. `setup.py`, `package.json#postinstall`.
    pub new_install_scripts: Vec<String>,
    /// Suspicious `setup.py` calls in `new` that no file in `old`
    /// exhibited (matched by file and call kind, ignoring line shifts).
    pub new_findings: Vec<BehavioralFinding>,
}

impl PackageDiff {
    /// True when nothing risky changed — added files alone are fine, but
    /// new install scripts or new suspicious calls are not.
    pub fn is_clean(&self) -> bool {
        self.new_install_scripts.is_empty() && self.new_findings.is_empty()
    }
}

/// Compares the package trees rooted at `old` and `new`.
pub fn diff_versions(old: &Path, new: &Path) -> std::io::Result<PackageDiff> {
    let old_files: BTreeSet<String> = collect_files(old)?.into_iter().collect();
    let new_files: BTreeSet<String> = collect_files(new)?.into_iter().collect();

    let added_files: Vec<String> = new_files.difference(&old_files).cloned().collect();
    let removed_files: Vec<String> = old_files.difference(&new_files).cloned().collect();

    let old_scripts = install_scripts(old, &old_files);
    let new_scripts = install_scripts(new, &new_files);
    let new_install_scripts: Vec<String> = new_scripts.difference(&old_scripts).cloned().collect();

    let new_findings = new_suspicious_calls(old, &old_files, new, &new_files);

    Ok(PackageDiff {
        added_files,
        removed_files,
        new_install_scripts,
        new_findings,
    })
}

/// Describes every install-time hook a tree has: a `setup.py` at the root
/// and npm lifecycle scripts from `package.json`. Unparseable
/// `package.json` files are ignored (the lockfile scanner reports those).
fn install_scripts(root: &Path, files: &BTreeSet<String>) -> BTreeSet<String> {
    let mut scripts = BTreeSet::new();
    if files.contains("setup.py") {
        scripts.insert("setup.py".to_string());
    }
    if files.contains("package.json") {
        let Ok(bytes) = fs::read(root.join("package.json")) else {
            return scripts;
        };
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return scripts;
        };
        const LIFECYCLE: &[&str] = &["preinstall", "install", "postinstall"];
        if let Some(map) = json.get("scripts").and_then(|s| s.as_object()) {
            for hook in LIFECYCLE {
                if map.contains_key(*hook) {
                    scripts.insert(format!("package.json#{hook}"));
                }
            }
        }
    }
    scripts
}

/// Scans `setup.py` in both trees and keeps the findings whose call kind
/// is new for that file (or the file itself is new).
fn new_suspicious_calls(
    old: &Path,
    old_files: &BTreeSet<String>,
    new: &Path,
    new_files: &BTreeSet<String>,
) -> Vec<BehavioralFinding> {
    let mut baseline: BTreeSet<(String, String)> = BTreeSet::new();
    for file in old_files {
        if file != "setup.py" {
            continue;
        }
        if let Ok(source) = fs::read_to_string(old.join(file)) {
            for finding in scan_setup_py_in(file, &source) {
                baseline.insert((finding.file, finding.message));
            }
        }
    }

    let mut findings = Vec::new();
    for file in new_files {
        if file != "setup.py" {
            continue;
        }
        let Ok(source) = fs::read_to_string(new.join(file)) else {
            continue;
        };
        for finding in scan_setup_py_in(file, &source) {
            if !baseline.contains(&(finding.file.clone(), finding.message.clone())) {
                findings.push(finding);
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/behavioral")
            .join(name)
    }

    /// The headline case from the issue: v1 is a normal patch-level
    /// package, v2 adds an install script and obfuscated network code.
    #[test]
    fn v2_malicious_upgrade_is_reported() {
        let diff = diff_versions(&fixture("leftpad-lite-v1"), &fixture("leftpad-lite-v2")).unwrap();

        assert_eq!(diff.added_files, vec!["setup.py"]);
        assert_eq!(diff.removed_files, Vec::<String>::new());

        assert_eq!(diff.new_install_scripts, vec!["setup.py"]);

        let messages: Vec<&str> = diff
            .new_findings
            .iter()
            .map(|f| f.message.as_str())
            .collect();
        assert!(
            messages.contains(&"base64.b64decode call"),
            "obfuscation not reported: {messages:?}"
        );
        assert!(
            messages.contains(&"network call (urllib/requests)"),
            "network call not reported: {messages:?}"
        );
        assert!(
            messages.contains(&"subprocess call"),
            "subprocess call not reported: {messages:?}"
        );
        for finding in &diff.new_findings {
            assert_eq!(finding.file, "setup.py");
            assert!(finding.line.is_some(), "expected a line number: {finding}");
        }
        assert!(!diff.is_clean());
    }

    #[test]
    fn identical_versions_are_clean() {
        let diff = diff_versions(&fixture("leftpad-lite-v1"), &fixture("leftpad-lite-v1")).unwrap();
        assert_eq!(diff, PackageDiff::default());
        assert!(diff.is_clean());
    }

    #[test]
    fn added_files_alone_are_clean() {
        let diff =
            diff_versions(&fixture("leftpad-lite-v1"), &fixture("leftpad-lite-docs")).unwrap();
        assert_eq!(diff.added_files, vec!["CHANGELOG.md"]);
        assert!(diff.new_install_scripts.is_empty());
        assert!(diff.new_findings.is_empty());
        assert!(diff.is_clean());
    }

    #[test]
    fn preexisting_suspicious_calls_are_not_repeated() {
        // Both versions call requests.get; only the *new* b64decode is
        // reported, and the line shift doesn't create noise.
        let diff = diff_versions(&fixture("beacon-v1"), &fixture("beacon-v2")).unwrap();
        assert!(diff.added_files.is_empty());
        assert!(diff.removed_files.is_empty());
        assert!(diff.new_install_scripts.is_empty());
        let messages: Vec<&str> = diff
            .new_findings
            .iter()
            .map(|f| f.message.as_str())
            .collect();
        assert_eq!(messages, vec!["base64.b64decode call"]);
    }

    #[test]
    fn npm_install_script_added_in_package_json() {
        let diff = diff_versions(&fixture("npmish-v1"), &fixture("npmish-v2")).unwrap();
        assert_eq!(
            diff.new_install_scripts,
            vec!["package.json#postinstall".to_string()]
        );
        assert!(diff.added_files.is_empty(), "{:?}", diff.added_files);
        assert!(diff.new_findings.is_empty());
        assert!(!diff.is_clean());
    }
}
