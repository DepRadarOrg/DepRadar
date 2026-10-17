//! `scan_dir` over a temp-dir monorepo: two lockfiles plus a `node_modules`
//! copy that must be skipped.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use deprader::pipeline::{scan_dir, ScanOptions};

/// A fresh directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("depradar-scan-dir-{}-{nanos}", process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const WEB_LOCK: &str = r#"{
  "name": "web",
  "lockfileVersion": 3,
  "packages": {
    "": {
      "name": "web",
      "version": "1.0.0",
      "dependencies": { "react": "^18.3.1", "expresss": "^4.18.2" }
    },
    "node_modules/react": { "version": "18.3.1" },
    "node_modules/expresss": { "version": "4.18.2" }
  }
}"#;

const NESTED_LOCK: &str = r#"{
  "name": "installed",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "installed", "version": "1.0.0" },
    "node_modules/lodahs": { "version": "1.0.0" }
  }
}"#;

/// Typosquat checks only: no network, no ignore file.
fn offline() -> ScanOptions<'static> {
    ScanOptions {
        advisories: None,
        ..ScanOptions::default()
    }
}

#[test]
fn scan_dir_finds_both_lockfiles_and_skips_node_modules() {
    let tree = TempDir::new();
    tree.write("apps/web/package-lock.json", WEB_LOCK);
    tree.write(
        "services/api/requirements.txt",
        "requests==2.31.0\nflask>=2.0\n",
    );
    tree.write("apps/web/node_modules/pkg/package-lock.json", NESTED_LOCK);
    tree.write("target/package-lock.json", NESTED_LOCK);
    tree.write("README.md", "not a lockfile");

    let report = scan_dir(&tree.0, &offline()).unwrap();

    let paths: Vec<&Path> = report.lockfiles.iter().map(|r| r.path.as_path()).collect();
    assert_eq!(
        paths,
        [
            Path::new("apps/web/package-lock.json"),
            Path::new("services/api/requirements.txt"),
        ]
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.has_findings());

    let web = &report.lockfiles[0];
    assert_eq!(web.summary.packages, 2);
    let flagged: Vec<&str> = web.findings.iter().map(|f| f.package.as_str()).collect();
    assert_eq!(flagged, ["expresss"]);

    let api = &report.lockfiles[1];
    assert_eq!(api.summary.packages, 2);
    assert!(api.findings.is_empty());

    // Nothing from the skipped node_modules / target copies leaks in.
    let rendered = report.to_string();
    assert!(!rendered.contains("lodahs"), "{rendered}");
    assert!(
        rendered.contains("== apps/web/package-lock.json =="),
        "{rendered}"
    );
}

#[test]
fn scan_dir_reports_a_malformed_lockfile_and_keeps_going() {
    let tree = TempDir::new();
    tree.write("good/requirements.txt", "requests==2.31.0\n");
    tree.write("bad/package-lock.json", "{ not json");

    let report = scan_dir(&tree.0, &offline()).unwrap();

    assert_eq!(report.lockfiles.len(), 1);
    assert_eq!(report.lockfiles[0].path, Path::new("good/requirements.txt"));
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].path, Path::new("bad/package-lock.json"));
}

#[test]
fn scan_dir_fails_when_the_root_is_missing() {
    let tree = TempDir::new();
    assert!(scan_dir(tree.0.join("missing"), &offline()).is_err());
}
