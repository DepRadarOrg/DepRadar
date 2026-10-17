//! End-to-end tests for the `depradar` binary (and the deprecated `scan`
//! alias). Every scan runs with `--offline` so no test touches the network.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

fn depradar() -> Command {
    Command::cargo_bin("depradar").unwrap()
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

/// Parses the whole of stdout as one JSON document, which fails if anything
/// else (logs, a second document) was printed alongside it.
fn stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout is not a single JSON document ({err}):\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[test]
fn name_output_matches_the_legacy_scan_binary() {
    let new = depradar().args(["name", "expresss"]).output().unwrap();
    let old = Command::cargo_bin("scan")
        .unwrap()
        .arg("expresss")
        .output()
        .unwrap();

    assert!(new.status.success());
    assert!(old.status.success());
    assert_eq!(
        String::from_utf8_lossy(&new.stdout),
        String::from_utf8_lossy(&old.stdout)
    );
    assert!(String::from_utf8_lossy(&new.stdout).contains("likely typosquat"));
    assert!(String::from_utf8_lossy(&old.stderr).contains("deprecated"));
}

#[test]
fn help_text_matches_snapshot() {
    let output = depradar().arg("--help").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        include_str!("snapshots/depradar-help.txt"),
        "help text changed; if that's intended, regenerate the snapshot with \
         `cargo run --bin depradar -- --help > tests/snapshots/depradar-help.txt`"
    );
}

#[test]
fn name_json_is_one_document() {
    let output = depradar()
        .args(["--format", "json", "name", "expresss"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let json = stdout_json(&output);
    assert_eq!(json["candidate"], "expresss");
    assert_eq!(json["likely_typosquat"], true);
    assert_eq!(json["distance_match"]["name"], "express");
    assert_eq!(json["distance_match"]["distance"], 1);
}

#[test]
fn scan_dir_json_groups_findings_by_lockfile() {
    let output = depradar()
        .args(["scan", "--offline", "--format", "json"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();
    // A typosquat was found.
    assert_eq!(output.status.code(), Some(1));

    let json = stdout_json(&output);
    let lockfiles = json["lockfiles"].as_array().unwrap();
    assert_eq!(lockfiles.len(), 1);
    assert_eq!(lockfiles[0]["path"], "package-lock.json");
    let findings = lockfiles[0]["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f["package"] == "expresss" && f["kind"] == "typosquat"),
        "{findings:?}"
    );
    assert_eq!(json["errors"], Value::Array(Vec::new()));
}

#[test]
fn scan_file_text_reports_findings() {
    let output = depradar()
        .args(["scan", "--offline"])
        .arg(fixture("pypi-basic/requirements.txt"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("reqeusts"), "{stdout}");
}

#[test]
fn scan_error_in_json_mode_still_prints_one_document() {
    let output = depradar()
        .args([
            "--format",
            "json",
            "scan",
            "--offline",
            "does/not/exist.lock",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout_json(&output)["error"].is_string());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("error: "));
}

#[test]
fn missing_subcommand_is_a_usage_error() {
    let output = depradar().output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
