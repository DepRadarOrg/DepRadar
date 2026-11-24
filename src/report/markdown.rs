//! Markdown report output for GitHub issues and PR comments.
//!
//! Renders a scan [`Report`](crate::pipeline::Report) as Markdown with a
//! summary table followed by detailed sections per finding. Each finding
//! includes a severity badge, package details, and references.

use std::fmt::Write;

use super::{is_malicious, label, severity_of, AdvisorySeverities, Severity};
use crate::pipeline::Report;

/// Renders a severity badge in Markdown.
fn severity_badge(severity: Severity) -> String {
    let color = match severity {
        Severity::Critical => "critical",
        Severity::High => "important",
        Severity::Medium => "yellow",
        Severity::Low => "informational",
    };
    let level = label(severity).to_uppercase();
    format!("![{level}](https://img.shields.io/badge/{level}-{color})")
}

/// Renders `report` as Markdown suitable for GitHub issues and PR comments.
pub fn render(report: &Report, advisories: &AdvisorySeverities) -> String {
    let mut out = String::new();
    // Writing to a String can't fail.
    let _ = write_report(&mut out, report, advisories);
    out
}

fn write_report(
    out: &mut String,
    report: &Report,
    advisories: &AdvisorySeverities,
) -> std::fmt::Result {
    // Header
    writeln!(out, "# DepRadar Security Report\n")?;

    // Summary table
    writeln!(out, "## Summary\n")?;
    writeln!(out, "| Metric | Count |")?;
    writeln!(out, "|--------|-------|")?;
    writeln!(out, "| Total Packages | {} |", report.summary.packages)?;
    writeln!(out, "| Typosquats | {} |", report.summary.typosquats)?;
    writeln!(
        out,
        "| Vulnerabilities | {} |",
        report.summary.vulnerabilities
    )?;
    writeln!(out, "| Suppressed | {} |", report.summary.suppressed)?;
    writeln!(out, "| Warnings | {} |", report.summary.warnings)?;
    writeln!(out)?;

    // Findings sections
    if report.findings.is_empty() {
        writeln!(out, "## Findings\n")?;
        writeln!(out, "✅ No security issues found.\n")?;
    } else {
        writeln!(out, "## Findings\n")?;
        for (idx, finding) in report.findings.iter().enumerate() {
            let severity = severity_of(finding, advisories);
            writeln!(out, "### Finding #{}\n", idx + 1)?;
            writeln!(out, "{}\n", severity_badge(severity))?;
            writeln!(out, "**Package:** `{}`", finding.package)?;
            if let Some(version) = &finding.version {
                writeln!(out, " @ `{}`", version)?;
            } else {
                writeln!(out)?;
            }
            writeln!(
                out,
                "**Ecosystem:** {}",
                format_ecosystem(&finding.ecosystem)
            )?;
            writeln!(out, "**Severity:** {}", label(severity))?;
            if is_malicious(finding) {
                writeln!(out, "**Label:** malicious package")?;
            }
            writeln!(
                out,
                "**Scope:** {}",
                if finding.direct {
                    "Direct"
                } else {
                    "Transitive"
                }
            )?;

            if !finding.path.is_empty() {
                writeln!(out, "**Path:** {}", finding.path.join(" → "))?;
            }

            // Add details about the finding kind
            writeln!(out)?;
            write_finding_details(out, finding)?;

            writeln!(out)?;
        }
    }

    // Suppressed findings section
    if !report.suppressed.is_empty() {
        writeln!(out, "## Suppressed Findings\n")?;
        for suppressed in &report.suppressed {
            writeln!(out, "- {} — {}", suppressed.finding, suppressed.reason)?;
            if let Some(expires) = suppressed.expires {
                writeln!(out, "  _(expires: {})_", expires)?;
            }
        }
        writeln!(out)?;
    }

    // Warnings section
    if !report.warnings.is_empty() {
        writeln!(out, "## Warnings\n")?;
        for warning in &report.warnings {
            writeln!(out, "- {}", warning)?;
        }
        writeln!(out)?;
    }

    Ok(())
}

/// Writes the remediation recommendation, preferring the smallest
/// non-breaking (in-major) upgrade and showing the next-major fix as an
/// alternative when both exist.
fn write_remediation(
    out: &mut String,
    remediation: &crate::cve::remediation::Remediation,
) -> std::fmt::Result {
    match (&remediation.in_major, &remediation.next_major) {
        (Some(in_major), Some(next_major)) => {
            writeln!(
                out,
                "\n**Recommendation**: Update to `{}` (fixes it within your current major version).",
                in_major
            )?;
            writeln!(
                out,
                "\n_Alternative: upgrade to `{}` (next major version)._",
                next_major
            )?;
        }
        (Some(in_major), None) => {
            writeln!(
                out,
                "\n**Recommendation**: Update to `{}`.",
                in_major
            )?;
        }
        (None, Some(next_major)) => {
            writeln!(
                out,
                "\n**Recommendation**: Update to `{}` (requires a major version upgrade).",
                next_major
            )?;
        }
        (None, None) => {
            writeln!(
                out,
                "\n**Recommendation**: Update to a patched version or find an alternative package."
            )?;
        }
    }
    Ok(())
}

fn format_ecosystem(ecosystem: &crate::model::Ecosystem) -> &'static str {
    match ecosystem {
        crate::model::Ecosystem::Npm => "npm",
        crate::model::Ecosystem::PyPI => "PyPI",
        crate::model::Ecosystem::CratesIo => "crates.io",
        crate::model::Ecosystem::Go => "Go",
        crate::model::Ecosystem::Maven => "Maven",
        crate::model::Ecosystem::RubyGems => "RubyGems",
        crate::model::Ecosystem::Packagist => "Packagist",
    }
}

/// Writes detailed information about the finding kind.
fn write_finding_details(out: &mut String, finding: &crate::model::Finding) -> std::fmt::Result {
    use crate::model::FindingKind;

    match &finding.kind {
        FindingKind::Typosquat {
            distance_match,
            homoglyph_match,
        } => {
            writeln!(out, "#### Details\n")?;
            writeln!(
                out,
                "This package appears to be a typosquat of a popular package.\n"
            )?;

            if let Some((name, distance)) = distance_match {
                writeln!(
                    out,
                    "- **Edit distance**: {} edit(s) from `{}`",
                    distance, name
                )?;
            }

            if let Some(name) = homoglyph_match {
                writeln!(
                    out,
                    "- **Homoglyph impersonation**: Lookalike characters impersonating `{}`",
                    name
                )?;
            }

            writeln!(out, "\n**Recommendation**: Remove this dependency immediately and verify it was installed intentionally.")?;
        }
        FindingKind::Vulnerability {
            advisory_id,
            known_exploited,
            epss_score,
            published,
            remediation,
        } => {
            writeln!(out, "#### Details\n")?;
            if is_malicious(finding) {
                writeln!(
                    out,
                    "⚠️ **Malicious package detected.** This package is known to be malware.\n"
                )?;
            } else {
                writeln!(out, "Known vulnerability detected.\n")?;
            }
            writeln!(out, "- **Advisory ID**: `{}`", advisory_id)?;
            writeln!(
                out,
                "- **Reference**: [View on OSV](https://osv.dev/vulnerability/{})",
                advisory_id
            )?;
            if let Some(published) = published {
                writeln!(out, "- **Published**: {}", published)?;
            }
            if *known_exploited {
                writeln!(
                    out,
                    "- **Known exploited**: ⚠️ Listed in CISA's Known Exploited Vulnerabilities catalog"
                )?;
            }
            if let Some(score) = epss_score {
                writeln!(out, "- **EPSS score**: {:.2}%", score * 100.0)?;
            }
            if is_malicious(finding) {
                writeln!(
                    out,
                    "\n**Recommendation**: Remove this dependency immediately and audit for compromise."
                )?;
            } else {
                write_remediation(out, remediation)?;
            }
        }
        FindingKind::Deprecated { .. } | FindingKind::License { .. } => {
            writeln!(out, "#### Details\n")?;
            writeln!(out, "{finding}")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Ecosystem, Finding, FindingKind};

    #[test]
    fn severity_badges_use_correct_colors() {
        assert!(severity_badge(Severity::Critical).contains("critical"));
        assert!(severity_badge(Severity::High).contains("important"));
        assert!(severity_badge(Severity::Medium).contains("yellow"));
        assert!(severity_badge(Severity::Low).contains("informational"));
    }

    #[test]
    fn format_ecosystem_returns_correct_names() {
        assert_eq!(format_ecosystem(&Ecosystem::Npm), "npm");
        assert_eq!(format_ecosystem(&Ecosystem::PyPI), "PyPI");
        assert_eq!(format_ecosystem(&Ecosystem::CratesIo), "crates.io");
        assert_eq!(format_ecosystem(&Ecosystem::Go), "Go");
        assert_eq!(format_ecosystem(&Ecosystem::Maven), "Maven");
        assert_eq!(format_ecosystem(&Ecosystem::RubyGems), "RubyGems");
        assert_eq!(format_ecosystem(&Ecosystem::Packagist), "Packagist");
    }

    fn make_test_report() -> crate::pipeline::Report {
        use crate::pipeline::Summary;
        use std::path::PathBuf;

        crate::pipeline::Report {
            path: PathBuf::from("package-lock.json"),
            ecosystem: Ecosystem::Npm,
            findings: vec![
                Finding {
                    package: "expresss".to_string(),
                    version: Some("1.0.0".to_string()),
                    ecosystem: Ecosystem::Npm,
                    direct: true,
                    path: vec![],
                    kind: FindingKind::Typosquat {
                        distance_match: Some(("express".to_string(), 1)),
                        homoglyph_match: None,
                    },
                },
                Finding {
                    package: "lodash".to_string(),
                    version: Some("4.17.4".to_string()),
                    ecosystem: Ecosystem::Npm,
                    direct: false,
                    path: vec![
                        "my-app".to_string(),
                        "webpack".to_string(),
                        "lodash".to_string(),
                    ],
                    kind: FindingKind::Vulnerability {
                        advisory_id: "GHSA-29mw-wpgm-hmr9".to_string(),
                        known_exploited: false,
                        epss_score: None,
                        published: Some("2021-05-06T00:00:00Z".to_string()),
                        remediation: crate::cve::remediation::Remediation {
                            in_major: Some("4.17.21".to_string()),
                            next_major: Some("5.0.0".to_string()),
                        },
                    },
                },
            ],
            suppressed: vec![],
            warnings: vec![],
            summary: Summary {
                packages: 150,
                queried: 150,
                typosquats: 1,
                vulnerabilities: 1,
                suppressed: 0,
                warnings: 0,
            },
            dependencies: vec![],
            timings: vec![],
        }
    }

    #[test]
    fn malicious_advisory_renders_critical_with_label() {
        use crate::pipeline::Summary;
        use std::path::PathBuf;

        let report = crate::pipeline::Report {
            path: PathBuf::from("package-lock.json"),
            ecosystem: Ecosystem::Npm,
            findings: vec![Finding {
                package: "evil-pkg".to_string(),
                version: Some("1.0.0".to_string()),
                ecosystem: Ecosystem::Npm,
                direct: true,
                path: vec![],
                kind: FindingKind::Vulnerability {
                    advisory_id: "MAL-2024-1234".to_string(),
                    known_exploited: false,
                    epss_score: None,
                    published: None,
                    remediation: crate::cve::remediation::Remediation::default(),
                },
            }],
            suppressed: vec![],
            warnings: vec![],
            summary: Summary {
                packages: 1,
                queried: 1,
                typosquats: 0,
                vulnerabilities: 1,
                suppressed: 0,
                warnings: 0,
            },
            dependencies: vec![],
            timings: vec![],
        };

        let output = render(&report, &AdvisorySeverities::default());
        assert!(output.contains("malicious package"));
        assert!(output.contains("CRITICAL"));
        assert!(output.contains("Malicious package detected"));
    }

    #[test]
    fn snapshot_markdown_report() {
        let report = make_test_report();
        let mut advisories = AdvisorySeverities::default();
        advisories.insert("GHSA-29mw-wpgm-hmr9", Severity::Medium);

        let output = render(&report, &advisories);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn remediation_shows_in_major_fix_with_next_major_alternative() {
        let report = make_test_report();
        let output = render(&report, &AdvisorySeverities::default());

        // The in-major fix is the primary recommendation.
        assert!(output.contains("Update to `4.17.21`"));
        // The next-major fix is shown as an alternative.
        assert!(output.contains("Alternative: upgrade to `5.0.0`"));
    }

    #[test]
    fn remediation_shows_only_next_major_when_no_in_major_fix() {
        use crate::pipeline::Summary;
        use std::path::PathBuf;

        let report = crate::pipeline::Report {
            path: PathBuf::from("package-lock.json"),
            ecosystem: Ecosystem::Npm,
            findings: vec![Finding {
                package: "some-pkg".to_string(),
                version: Some("1.0.0".to_string()),
                ecosystem: Ecosystem::Npm,
                direct: true,
                path: vec![],
                kind: FindingKind::Vulnerability {
                    advisory_id: "GHSA-xxxx-yyyy-zzzz".to_string(),
                    known_exploited: false,
                    epss_score: None,
                    published: None,
                    remediation: crate::cve::remediation::Remediation {
                        in_major: None,
                        next_major: Some("2.0.0".to_string()),
                    },
                },
            }],
            suppressed: vec![],
            warnings: vec![],
            summary: Summary {
                packages: 1,
                queried: 1,
                typosquats: 0,
                vulnerabilities: 1,
                suppressed: 0,
                warnings: 0,
            },
            dependencies: vec![],
            timings: vec![],
        };

        let output = render(&report, &AdvisorySeverities::default());
        assert!(output.contains("Update to `2.0.0`"));
        assert!(output.contains("requires a major version upgrade"));
        assert!(!output.contains("Alternative:"));
    }
}
