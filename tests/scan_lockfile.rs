//! End-to-end tests for `scan_lockfile` over fixture lockfiles, with OSV
//! replaced by an in-memory advisory source.

use std::cell::RefCell;
use std::path::PathBuf;

use deprader::cve::osv_client::OsvBatchQuery;
use deprader::ignore::{Date, IgnoreFile};
use deprader::model::{Ecosystem, FindingKind};
use deprader::pipeline::{scan_lockfile, AdvisoryError, AdvisorySource, ScanOptions};

/// Advisory source that knows a fixed set of vulnerable `(name, version)`
/// pins and records every batch it is asked about.
struct MockOsv {
    vulnerable: Vec<(&'static str, &'static str, &'static str)>,
    batches: RefCell<Vec<Vec<(String, String)>>>,
}

impl MockOsv {
    fn new(vulnerable: Vec<(&'static str, &'static str, &'static str)>) -> Self {
        MockOsv {
            vulnerable,
            batches: RefCell::new(Vec::new()),
        }
    }
}

impl AdvisorySource for MockOsv {
    fn query(&self, queries: &[OsvBatchQuery<'_>]) -> Result<Vec<Vec<String>>, AdvisoryError> {
        self.batches.borrow_mut().push(
            queries
                .iter()
                .map(|q| (q.name.to_string(), q.version.to_string()))
                .collect(),
        );
        Ok(queries
            .iter()
            .map(|q| {
                self.vulnerable
                    .iter()
                    .filter(|(name, version, _)| *name == q.name && *version == q.version)
                    .map(|(_, _, id)| id.to_string())
                    .collect()
            })
            .collect())
    }
}

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path)
}

#[test]
fn finds_typosquat_and_vulnerable_pin_in_npm_lockfile() {
    let osv = MockOsv::new(vec![("lodash", "4.17.4", "GHSA-29mw-wpgm-hmr9")]);
    let opts = ScanOptions {
        typosquat: true,
        advisories: Some(&osv),
        ..ScanOptions::default()
    };

    let report = scan_lockfile(fixture("npm-basic/package-lock.json"), &opts).unwrap();

    let typosquats: Vec<_> = report
        .findings
        .iter()
        .filter(|f| matches!(f.kind, FindingKind::Typosquat { .. }))
        .collect();
    assert_eq!(typosquats.len(), 1, "findings: {:#?}", report.findings);
    assert_eq!(typosquats[0].package, "expresss");
    assert!(typosquats[0].direct);
    assert!(typosquats[0].to_string().contains("[direct]"));
    assert_eq!(
        typosquats[0].kind,
        FindingKind::Typosquat {
            distance_match: Some(("express".to_string(), 1)),
            homoglyph_match: None,
        }
    );

    let vulnerabilities: Vec<_> = report
        .findings
        .iter()
        .filter(|f| matches!(f.kind, FindingKind::Vulnerability { .. }))
        .collect();
    assert_eq!(vulnerabilities.len(), 1, "findings: {:#?}", report.findings);
    assert_eq!(vulnerabilities[0].package, "lodash");
    assert_eq!(vulnerabilities[0].version.as_deref(), Some("4.17.4"));
    assert_eq!(vulnerabilities[0].ecosystem, Ecosystem::Npm);
    // lodash@4.17.4 is installed both at the top level and nested under
    // express; the deduped finding is direct because one install is.
    assert!(vulnerabilities[0].direct);
    assert_eq!(
        vulnerabilities[0].kind,
        FindingKind::Vulnerability {
            advisory_id: "GHSA-29mw-wpgm-hmr9".to_string()
        }
    );
}

#[test]
fn dedupes_pins_and_queries_osv_in_one_batch() {
    let osv = MockOsv::new(Vec::new());
    let opts = ScanOptions {
        typosquat: false,
        advisories: Some(&osv),
        ..ScanOptions::default()
    };

    let report = scan_lockfile(fixture("npm-basic/package-lock.json"), &opts).unwrap();

    // lodash@4.17.4 is installed twice but queried once; the unversioned
    // package isn't queried at all.
    let batches = osv.batches.borrow();
    assert_eq!(batches.len(), 1);
    let mut queried = batches[0].clone();
    queried.sort();
    assert_eq!(
        queried,
        vec![
            ("express".to_string(), "4.21.2".to_string()),
            ("expresss".to_string(), "1.0.0".to_string()),
            ("lodash".to_string(), "4.17.4".to_string()),
        ]
    );
    assert_eq!(report.summary.packages, 4);
    assert_eq!(report.summary.queried, 3);
    // Only the local `file:` install is flagged; nothing is vulnerable.
    assert!(
        report
            .findings
            .iter()
            .all(|f| matches!(f.kind, FindingKind::NonRegistrySource { .. })),
        "findings: {:#?}",
        report.findings
    );
}

#[test]
fn reports_parse_warnings_separately_from_findings() {
    let opts = ScanOptions {
        typosquat: false,
        advisories: None,
        ..ScanOptions::default()
    };

    let report = scan_lockfile(fixture("npm-basic/package-lock.json"), &opts).unwrap();

    // The unversioned package is a local `file:` install: one finding, and
    // its missing version is a warning rather than a second finding.
    assert_eq!(report.findings.len(), 1, "findings: {:#?}", report.findings);
    assert_eq!(report.findings[0].package, "zzz-unversioned-internal-fixture");
    assert_eq!(report.warnings.len(), 1, "warnings: {:#?}", report.warnings);
    assert!(report.warnings[0].contains("zzz-unversioned-internal-fixture"));
    assert_eq!(report.summary.warnings, 1);
}

#[test]
fn unsupported_file_name_is_an_error() {
    let opts = ScanOptions::default();
    assert!(scan_lockfile(fixture("does-not-exist.lock"), &opts).is_err());
}

#[test]
fn transitive_finding_carries_its_dependency_path() {
    let osv = MockOsv::new(vec![("old-lib", "1.0.0", "GHSA-test-old-lib")]);
    let opts = ScanOptions {
        typosquat: false,
        advisories: Some(&osv),
        ..ScanOptions::default()
    };

    let report = scan_lockfile(fixture("npm-chain/package-lock.json"), &opts).unwrap();

    assert_eq!(report.findings.len(), 1, "findings: {:#?}", report.findings);
    let finding = &report.findings[0];
    assert_eq!(finding.package, "old-lib");
    assert!(!finding.direct);
    assert_eq!(
        finding.path,
        vec!["my-app", "webpack", "loader-kit", "old-lib"]
    );
    assert!(finding
        .to_string()
        .contains("[transitive via my-app > webpack > loader-kit > old-lib]"));
}

const LODASH_IGNORE: &str = r#"
[[ignore]]
package = "lodash"
advisory = "GHSA-29mw-wpgm-hmr9"
reason = "Only reachable from build tooling"
expires = 2026-12-31
"#;

#[test]
fn ignore_file_suppresses_matching_findings_until_they_expire() {
    let osv = MockOsv::new(vec![("lodash", "4.17.4", "GHSA-29mw-wpgm-hmr9")]);
    let ignore = IgnoreFile::parse(LODASH_IGNORE).unwrap();
    let scan_on = |today: &str| {
        let opts = ScanOptions {
            typosquat: false,
            advisories: Some(&osv),
            ignore: Some(&ignore),
            today: Date::parse(today),
        };
        scan_lockfile(fixture("npm-basic/package-lock.json"), &opts).unwrap()
    };

    let active = scan_on("2026-10-16");
    assert!(!active.has_findings(), "findings: {:#?}", active.findings);
    assert_eq!(active.suppressed.len(), 1);
    assert_eq!(active.suppressed[0].finding.package, "lodash");
    assert_eq!(
        active.suppressed[0].reason,
        "Only reachable from build tooling"
    );
    assert_eq!(active.summary.suppressed, 1);
    assert_eq!(active.summary.vulnerabilities, 0);
    assert!(active.to_string().contains("Suppressed (1):"));

    let expired = scan_on("2027-01-01");
    assert!(expired.has_findings());
    assert!(expired.suppressed.is_empty());
    assert_eq!(expired.summary.vulnerabilities, 1);
    assert!(
        expired
            .warnings
            .iter()
            .any(|w| w.contains("expired on 2026-12-31")),
        "warnings: {:#?}",
        expired.warnings
    );
}

#[test]
fn git_and_file_dependencies_are_flagged_as_non_registry() {
    let opts = ScanOptions {
        typosquat: false,
        advisories: None,
        ..ScanOptions::default()
    };

    let report = scan_lockfile(fixture("non-registry/yarn.lock"), &opts).unwrap();

    let flagged: Vec<(&str, &str)> = report
        .findings
        .iter()
        .filter_map(|f| match &f.kind {
            FindingKind::NonRegistrySource { source } => {
                Some((f.package.as_str(), source.as_str()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        flagged,
        [
            (
                "left-pad",
                "git+https://github.com/stevemao/left-pad.git#v1.3.0"
            ),
            ("local-utils", "file:./packages/local-utils"),
        ]
    );
    assert_eq!(report.findings.len(), 2, "{:#?}", report.findings);
}

#[test]
fn counts_and_lists_unpinned_requirements() {
    let opts = ScanOptions {
        typosquat: false,
        advisories: None,
        ..ScanOptions::default()
    };

    let report = scan_lockfile(fixture("pypi-unpinned/requirements.txt"), &opts).unwrap();

    assert_eq!(report.summary.packages, 3);
    assert_eq!(report.summary.unpinned, 2);
    assert_eq!(report.unpinned(), vec!["requests", "flask"]);
    let text = report.to_string();
    assert!(text.contains("2 unpinned"), "{text}");
    assert!(text.contains("  - requests\n"), "{text}");
}

#[test]
fn ignore_file_without_reason_is_rejected() {
    assert!(IgnoreFile::parse("[[ignore]]\npackage = \"lodash\"\n").is_err());
}
