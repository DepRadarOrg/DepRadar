//! SARIF output is valid against the official SARIF 2.1.0 JSON schema
//! (`tests/fixtures/sarif-schema-2.1.0.json`, from
//! <https://github.com/oasis-tcs/sarif-spec>).

use std::path::PathBuf;

use deprader::cve::osv_client::OsvBatchQuery;
use deprader::pipeline::{scan_lockfile, AdvisoryError, AdvisorySource, ScanOptions};
use deprader::report::sarif::to_sarif;
use deprader::report::{AdvisorySeverities, Severity};
use serde_json::Value;

/// Reports GHSA-29mw-wpgm-hmr9 against lodash@4.17.4 and nothing else.
struct VulnerableLodash;

impl AdvisorySource for VulnerableLodash {
    fn query(&self, queries: &[OsvBatchQuery<'_>]) -> Result<Vec<Vec<String>>, AdvisoryError> {
        Ok(queries
            .iter()
            .map(|q| match (q.name, q.version) {
                ("lodash", "4.17.4") => vec!["GHSA-29mw-wpgm-hmr9".to_string()],
                _ => Vec::new(),
            })
            .collect())
    }
}

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path)
}

fn sarif_for_npm_basic() -> Value {
    let path = fixture("npm-basic/package-lock.json");
    let opts = ScanOptions {
        typosquat: true,
        advisories: Some(&VulnerableLodash),
        ..ScanOptions::default()
    };
    let report = scan_lockfile(&path, &opts).unwrap();
    let mut advisories = AdvisorySeverities::default();
    advisories.insert("GHSA-29mw-wpgm-hmr9", Severity::Medium);
    let contents = std::fs::read_to_string(&path).unwrap();
    to_sarif(&report, &advisories, Some(&contents))
}

#[test]
fn sarif_output_matches_the_2_1_0_schema() {
    let schema: Value =
        serde_json::from_str(include_str!("fixtures/sarif-schema-2.1.0.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let sarif = sarif_for_npm_basic();

    let errors: Vec<String> = validator
        .iter_errors(&sarif)
        .map(|err| format!("{} at {}", err, err.instance_path))
        .collect();
    assert!(errors.is_empty(), "schema violations: {errors:#?}");
}

#[test]
fn one_rule_per_finding_kind_and_one_result_per_finding() {
    let sarif = sarif_for_npm_basic();
    let run = &sarif["runs"][0];

    assert_eq!(sarif["version"], "2.1.0");
    let rules: Vec<&str> = run["tool"]["driver"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| rule["id"].as_str().unwrap())
        .collect();
    assert_eq!(rules, ["typosquat", "vulnerability", "install-script"]);

    let results = run["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "results: {results:#?}");

    let typosquat = &results[0];
    assert_eq!(typosquat["ruleId"], "typosquat");
    assert_eq!(typosquat["level"], "error");
    assert_eq!(typosquat["properties"]["package"], "expresss");

    let vulnerability = &results[1];
    assert_eq!(vulnerability["ruleId"], "vulnerability");
    assert_eq!(vulnerability["level"], "warning");
    assert_eq!(vulnerability["properties"]["severity"], "medium");

    for result in results {
        let location = &result["locations"][0]["physicalLocation"];
        let uri = location["artifactLocation"]["uri"].as_str().unwrap();
        assert!(uri.ends_with("npm-basic/package-lock.json"), "{uri}");
        assert!(location["region"]["startLine"].as_u64().unwrap() > 1);
    }
}
