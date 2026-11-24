//! `scan_dir` over a temp-dir monorepo: two lockfiles plus a `node_modules`
//! copy that must be skipped.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use deprader::cve::osv_client::OsvBatchQuery;
use deprader::pipeline::{
    scan_dir, scan_dir_changed_since, AdvisoryError, AdvisorySource, ScanOptions,
};

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

/// Runs `git` in `dir`, panicking if it fails.
fn git(dir: &Path, args: &[&str]) {
    let status = process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=DepRadar"])
        .args(["-c", "user.email=depradar@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn scan_dir_changed_since_scans_only_changed_lockfiles() {
    let tree = TempDir::new();
    tree.write("apps/web/package-lock.json", WEB_LOCK);
    tree.write("services/api/requirements.txt", "requests==2.31.0\n");
    git(&tree.0, &["init", "-q"]);
    git(&tree.0, &["add", "."]);
    git(&tree.0, &["commit", "-q", "-m", "initial"]);

    tree.write(
        "services/api/requirements.txt",
        "requests==2.31.0\nflask==3.0.0\n",
    );

    let report = scan_dir_changed_since(&tree.0, &offline(), "HEAD").unwrap();

    let paths: Vec<&Path> = report.lockfiles.iter().map(|r| r.path.as_path()).collect();
    assert_eq!(paths, [Path::new("services/api/requirements.txt")]);
    assert_eq!(report.lockfiles[0].summary.packages, 2);

    // Against an unknown ref the scan fails instead of silently scanning nothing.
    assert!(scan_dir_changed_since(&tree.0, &offline(), "no-such-ref").is_err());
}

/// Reports one advisory for every pinned `requests` version.
struct VulnerableRequests;

impl AdvisorySource for VulnerableRequests {
    fn query(&self, queries: &[OsvBatchQuery<'_>]) -> Result<Vec<Vec<String>>, AdvisoryError> {
        Ok(queries
            .iter()
            .map(|query| match query.name {
                "requests" => vec!["GHSA-x84v-xcm2-53pg".to_string()],
                _ => Vec::new(),
            })
            .collect())
    }
}

#[test]
fn scan_dir_reports_a_shared_finding_once_with_every_lockfile() {
    let tree = TempDir::new();
    tree.write("services/api/requirements.txt", "requests==2.19.0\n");
    tree.write(
        "services/worker/requirements.txt",
        "requests==2.19.0\nflask==3.0.0\n",
    );
    let opts = ScanOptions {
        typosquat: false,
        advisories: Some(&VulnerableRequests),
        show_progress: false,
        ..ScanOptions::default()
    };

    let report = scan_dir(&tree.0, &opts).unwrap();

    // Each lockfile still records its own finding...
    assert_eq!(report.lockfiles[0].findings.len(), 1);
    assert_eq!(report.lockfiles[1].findings.len(), 1);

    // ...but across the tree it is one finding with two paths.
    let findings = report.findings();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].finding.package, "requests");
    assert_eq!(
        findings[0].lockfiles,
        [
            Path::new("services/api/requirements.txt"),
            Path::new("services/worker/requirements.txt"),
        ]
    );

    let rendered = report.to_string();
    let lockfiles = "(in services/api/requirements.txt, services/worker/requirements.txt)";
    assert_eq!(rendered.matches("requests@2.19.0").count(), 1, "{rendered}");
    assert!(rendered.contains(lockfiles), "{rendered}");
}
