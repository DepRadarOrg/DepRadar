//! [SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)
//! output, the format GitHub code scanning ingests.
//!
//! The run declares one rule per [`FindingKind`] (`typosquat`,
//! `vulnerability`, `non-registry-source`, `deprecated` and `license`) and one result per finding.
//! Each result points at the scanned lockfile, on the first line that names the package when the
//! lockfile's contents are available. Severity maps to the SARIF `level`
//! (critical/high → `error`, medium → `warning`, low → `note`) and to the
//! `security-severity` score GitHub uses to rank security alerts.
//!
//! Upload the file with
//! [`github/codeql-action/upload-sarif`](https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/uploading-a-sarif-file-to-github).

use serde_json::{json, Map, Value};

use super::{is_malicious, label, severity_of, AdvisorySeverities, Severity};
use crate::model::{Finding, FindingKind};
use crate::pipeline::Report;

pub const SARIF_VERSION: &str = "2.1.0";
pub const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";

const INFORMATION_URI: &str = "https://github.com/DepRadarOrg/DepRadar";

/// A SARIF rule, one per [`FindingKind`].
struct Rule {
    id: &'static str,
    name: &'static str,
    short: &'static str,
    full: &'static str,
    default_severity: Severity,
}

const RULES: [Rule; 5] = [
    Rule {
        id: "typosquat",
        name: "Typosquat",
        short: "Dependency name imitates a popular package",
        full: "The package name is within a few edits of, or uses lookalike characters \
               to impersonate, a widely used package. Typosquats are a common way to \
               ship malware into a dependency graph.",
        default_severity: Severity::High,
    },
    Rule {
        id: "vulnerability",
        name: "KnownVulnerability",
        short: "Dependency version has a known vulnerability",
        full: "OSV reports a published security advisory affecting the exact version \
               pinned in the lockfile.",
        default_severity: Severity::High,
    },
    Rule {
        id: "non-registry-source",
        name: "NonRegistrySource",
        short: "Dependency is installed from outside the registry",
        full: "The package comes from a git URL, a tarball URL or a local path instead of \
               the package registry, so registry-side checks don't cover it.",
        default_severity: Severity::Low,
    },
    Rule {
        id: "deprecated",
        name: "DeprecatedDependency",
        short: "Dependency version is deprecated or yanked",
        full: "The package registry flagged this version.",
        default_severity: Severity::Low,
    },
    Rule {
        id: "license",
        name: "LicensePolicy",
        short: "Dependency license is outside the allowlist",
        full: "The package license is outside the configured policy.",
        default_severity: Severity::Low,
    },
];

fn rule_index(kind: &FindingKind) -> usize {
    match kind {
        FindingKind::Typosquat { .. } => 0,
        FindingKind::Vulnerability { .. } => 1,
        FindingKind::NonRegistrySource { .. } => 2,
        FindingKind::Deprecated { .. } => 3,
        FindingKind::License { .. } => 4,
    }
}

fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical | Severity::High => "error",
        Severity::Medium => "warning",
        Severity::Low => "note",
    }
}

/// The `security-severity` score GitHub buckets into its own
/// critical/high/medium/low ranking.
fn security_severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "9.5",
        Severity::High => "8.0",
        Severity::Medium => "5.5",
        Severity::Low => "2.0",
    }
}

/// Renders `report` as a SARIF log. `lockfile` is the scanned file's
/// contents, used to point each result at the line naming its package;
/// without it results point at line 1.
pub fn to_sarif(report: &Report, advisories: &AdvisorySeverities, lockfile: Option<&str>) -> Value {
    let uri = artifact_uri(report);
    let rules: Vec<Value> = RULES
        .iter()
        .map(|rule| {
            json!({
                "id": rule.id,
                "name": rule.name,
                "shortDescription": { "text": rule.short },
                "fullDescription": { "text": rule.full },
                "helpUri": INFORMATION_URI,
                "defaultConfiguration": { "level": level(rule.default_severity) },
                "properties": {
                    "tags": ["security", "supply-chain"],
                    "security-severity": security_severity(rule.default_severity),
                },
            })
        })
        .collect();
    let results: Vec<Value> = report
        .findings
        .iter()
        .map(|finding| result(finding, advisories, &uri, lockfile))
        .collect();

    json!({
        "$schema": SARIF_SCHEMA,
        "version": SARIF_VERSION,
        "runs": [{
            "tool": {
                "driver": {
                    "name": "DepRadar",
                    "informationUri": INFORMATION_URI,
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": rules,
                },
            },
            "artifacts": [{ "location": { "uri": uri } }],
            "results": results,
        }],
    })
}

fn result(
    finding: &Finding,
    advisories: &AdvisorySeverities,
    uri: &str,
    lockfile: Option<&str>,
) -> Value {
    let severity = severity_of(finding, advisories);
    let index = rule_index(&finding.kind);
    let line = lockfile
        .and_then(|contents| line_of(contents, &finding.package))
        .unwrap_or(1);
    let advisory = match &finding.kind {
        FindingKind::Vulnerability { advisory_id, .. } => advisory_id.as_str(),
        _ => "",
    };
    let malicious = is_malicious(finding);
    let fingerprint = format!(
        "{}:{}:{}@{}:{advisory}",
        RULES[index].id,
        finding.ecosystem.as_osv_str(),
        finding.package,
        finding.version.as_deref().unwrap_or("")
    );
    let mut tags = vec!["security", "supply-chain"];
    if malicious {
        tags.push("malware");
    }

    let mut properties = Map::new();
    properties.insert("package".into(), json!(finding.package));
    properties.insert("version".into(), json!(finding.version));
    properties.insert("ecosystem".into(), json!(finding.ecosystem.as_osv_str()));
    properties.insert("direct".into(), json!(finding.direct));
    properties.insert("severity".into(), json!(label(severity)));
    properties.insert(
        "security-severity".into(),
        json!(security_severity(severity)),
    );
    properties.insert("malicious".into(), json!(malicious));
    properties.insert("tags".into(), json!(tags));
    let mut message = format!("[{}] {finding}", label(severity));
    if let FindingKind::Vulnerability {
        known_exploited,
        epss_score,
        ..
    } = &finding.kind
    {
        if let Some(known_exploited) = known_exploited {
            properties.insert("known_exploited".into(), json!(known_exploited));
        }
        if let Some(epss_score) = epss_score {
            properties.insert("epss_score".into(), json!(epss_score));
        }
        if let Some(published) = advisories.published(advisory) {
            properties.insert("published".into(), json!(published));
        }
        if let Some(remediation) = advisories.remediation(advisory) {
            if let Some(fix) = &remediation.in_major {
                properties.insert("fixed_version".into(), json!(fix));
            }
            if let Some(alternative) = &remediation.major_upgrade {
                properties.insert("major_upgrade".into(), json!(alternative));
            }
            message.push_str(&format!(" ({remediation})"));
        }
    }

    json!({
        "ruleId": RULES[index].id,
        "ruleIndex": index,
        "level": level(severity),
        "message": { "text": message },
        "locations": [{
            "physicalLocation": {
                "artifactLocation": { "uri": uri, "index": 0 },
                "region": { "startLine": line },
            },
        }],
        "partialFingerprints": { "depradar/v1": fingerprint },
        "properties": Value::Object(properties),
    })
}

/// The lockfile path as a SARIF URI: forward slashes, no leading `./`.
fn artifact_uri(report: &Report) -> String {
    let path = report.path.to_string_lossy().replace('\\', "/");
    path.strip_prefix("./").unwrap_or(&path).to_string()
}

/// 1-based number of the first line in `contents` that names `package`: as
/// a quoted string (JSON lockfiles, `Cargo.lock`), an npm `node_modules/`
/// key, or at the start of the line (`requirements.txt`).
fn line_of(contents: &str, package: &str) -> Option<usize> {
    let quoted = format!("\"{package}\"");
    let nested = format!("node_modules/{package}\"");
    contents
        .lines()
        .position(|line| {
            line.contains(&quoted)
                || line.contains(&nested)
                || line.trim_start().strip_prefix(package).is_some_and(|rest| {
                    !rest.starts_with(|c: char| {
                        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
                    })
                })
        })
        .map(|index| index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_line_naming_a_package() {
        let lock = "{\n  \"packages\": {\n    \"node_modules/expresss\": {}\n  }\n}\n";
        assert_eq!(line_of(lock, "expresss"), Some(3));
        assert_eq!(line_of(lock, "express"), None);

        let requirements = "flask==2.0.0\nrequests==2.31.0\nrequests-mock==1.0\n";
        assert_eq!(line_of(requirements, "requests"), Some(2));
        assert_eq!(line_of(requirements, "requests-mock"), Some(3));
    }

    #[test]
    fn maps_severity_to_sarif_levels() {
        assert_eq!(level(Severity::Critical), "error");
        assert_eq!(level(Severity::High), "error");
        assert_eq!(level(Severity::Medium), "warning");
        assert_eq!(level(Severity::Low), "note");
    }

    #[test]
    fn renders_malicious_advisory_as_critical_error() {
        use crate::model::{Ecosystem, Finding};
        use std::path::PathBuf;

        let finding = Finding {
            kind: FindingKind::Vulnerability {
                advisory_id: "MAL-2024-1234".to_string(),
                known_exploited: None,
                epss_score: None,
            },
            ecosystem: Ecosystem::Npm,
            package: "evil-pkg".to_string(),
            version: Some("1.0.0".to_string()),
            direct: true,
        };
        let report = Report {
            path: PathBuf::from("package-lock.json"),
            findings: vec![finding],
        };
        let sarif = to_sarif(&report, &AdvisorySeverities::default(), None);
        let result = &sarif["runs"][0]["results"][0];
        assert_eq!(result["level"], "error");
        assert_eq!(result["properties"]["severity"], "critical");
        assert_eq!(result["properties"]["malicious"], true);
        assert!(result["properties"]["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "malware"));
    }

    #[test]
    fn emits_known_exploited_and_epss_properties() {
        use crate::model::{Ecosystem, Finding};

        let finding = Finding {
            kind: FindingKind::Vulnerability {
                advisory_id: "GHSA-xxxx".into(),
                known_exploited: Some(true),
                epss_score: Some(0.42),
            },
            ecosystem: Ecosystem::Npm,
            package: "left-pad".into(),
            version: Some("1.0.0".into()),
            direct: true,
        };
        let advisories = AdvisorySeverities::default();
        let value = result(&finding, &advisories, "package-lock.json", None);
        let properties = &value["properties"];
        assert_eq!(properties["known_exploited"], json!(true));
        assert_eq!(properties["epss_score"], json!(0.42));

        let plain = Finding {
            kind: FindingKind::Vulnerability {
                advisory_id: "GHSA-yyyy".into(),
                known_exploited: None,
                epss_score: None,
            },
            ecosystem: Ecosystem::Npm,
            package: "left-pad".into(),
            version: Some("1.0.0".into()),
            direct: true,
        };
        let value = result(&plain, &advisories, "package-lock.json", None);
        assert!(value["properties"].get("known_exploited").is_none());
        assert!(value["properties"].get("epss_score").is_none());
    }
}
