//! [SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)
//! output, the format GitHub code scanning ingests.
//!
//! The run declares one rule per [`FindingKind`] (`typosquat` and
//! `vulnerability`) and one result per finding. Each result points at the
//! scanned lockfile, on the first line that names the package when the
//! lockfile's contents are available. Severity maps to the SARIF `level`
//! (critical/high → `error`, medium → `warning`, low → `note`) and to the
//! `security-severity` score GitHub uses to rank security alerts.
//!
//! Upload the file with
//! [`github/codeql-action/upload-sarif`](https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/uploading-a-sarif-file-to-github).

use serde_json::{json, Value};

use super::{label, severity_of, AdvisorySeverities, Severity};
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

const RULES: [Rule; 3] = [
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
        id: "install-script",
        name: "InstallScript",
        short: "Package contains install-time script",
        full: "Package contains a preinstall, install, postinstall, or prepare script \
               that executes arbitrary code during installation. These scripts are a \
               common malware delivery vector.",
        default_severity: Severity::High,
    },
];

fn rule_index(kind: &FindingKind) -> usize {
    match kind {
        FindingKind::Typosquat { .. } => 0,
        FindingKind::Vulnerability { .. } => 1,
        FindingKind::InstallScript { .. } => 2,
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
        FindingKind::Vulnerability { advisory_id } => advisory_id.as_str(),
        FindingKind::Typosquat { .. } => "",
        FindingKind::InstallScript { script_name, .. } => script_name.as_str(),
    };
    let fingerprint = format!(
        "{}:{}:{}@{}:{advisory}",
        RULES[index].id,
        finding.ecosystem.as_osv_str(),
        finding.package,
        finding.version.as_deref().unwrap_or("")
    );

    json!({
        "ruleId": RULES[index].id,
        "ruleIndex": index,
        "level": level(severity),
        "message": { "text": format!("[{}] {finding}", label(severity)) },
        "locations": [{
            "physicalLocation": {
                "artifactLocation": { "uri": uri, "index": 0 },
                "region": { "startLine": line },
            },
        }],
        "partialFingerprints": { "depradar/v1": fingerprint },
        "properties": {
            "package": finding.package,
            "version": finding.version,
            "ecosystem": finding.ecosystem.as_osv_str(),
            "direct": finding.direct,
            "severity": label(severity),
            "security-severity": security_severity(severity),
        },
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
}
