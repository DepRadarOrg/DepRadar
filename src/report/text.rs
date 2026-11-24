//! Human-readable terminal output.
//!
//! Like [`Report`]'s `Display`, but every finding is prefixed with its
//! severity, and the label can be colored: critical and high in red,
//! medium in yellow, low dimmed. [`use_color`] decides whether to color,
//! honouring [`NO_COLOR`](https://no-color.org) and whether stdout is a
//! terminal. [`Verbosity`] trims the output to the findings and summary
//! line (`--quiet`) or adds every dependency checked and the time each scan
//! stage took (`--verbose`).

use std::fmt::Write;

use owo_colors::OwoColorize;

use super::{is_malicious, label, severity_of, AdvisorySeverities, Severity};
use crate::pipeline::{Report, StageTiming};

/// The `--color` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorChoice {
    /// Color only when writing to a terminal.
    #[default]
    Auto,
    Always,
    Never,
}

/// Whether to emit ANSI colors. A non-empty `NO_COLOR` turns color off
/// whatever `choice` says; otherwise `Auto` colors only a terminal.
pub fn use_color(choice: ColorChoice, no_color: Option<&str>, is_terminal: bool) -> bool {
    if no_color.is_some_and(|value| !value.is_empty()) {
        return false;
    }
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => is_terminal,
    }
}

/// How much detail to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
    /// Only the findings and the summary line.
    Quiet,
    #[default]
    Normal,
    /// Also every dependency checked and the time per scan stage.
    Verbose,
}

/// How to render a report as text.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextOptions {
    pub color: bool,
    pub verbosity: Verbosity,
}

/// `[HIGH]`-style severity tag, colored if asked.
fn tag(severity: Severity, color: bool) -> String {
    let text = format!("[{}]", label(severity).to_ascii_uppercase());
    if !color {
        return text;
    }
    match severity {
        Severity::Critical => text.red().bold().to_string(),
        Severity::High => text.red().to_string(),
        Severity::Medium => text.yellow().to_string(),
        Severity::Low => text.dimmed().to_string(),
    }
}

/// The `malicious package` label for `MAL-` advisories, colored if asked.
fn malware_label(color: bool) -> String {
    let text = "malicious package".to_string();
    if color {
        text.red().bold().to_string()
    } else {
        text
    }
}

/// `[KNOWN EXPLOITED]`-style marker, colored if asked.
fn known_exploited_tag(color: bool) -> String {
    let text = "[KNOWN EXPLOITED]".to_string();
    if color {
        text.red().bold().to_string()
    } else {
        text
    }
}

/// Renders a single finding line, including the KEV marker and EPSS score
/// when present.
fn finding_line(
    finding: &crate::pipeline::Finding,
    severity: Severity,
    color: bool,
) -> String {
    let mut line = format!("{} {finding}", tag(severity, color));
    if is_malicious(finding) {
        line.push(' ');
        line.push_str(&format!("[{}]", malware_label(color)));
    }
    if finding.known_exploited {
        line.push(' ');
        line.push_str(&known_exploited_tag(color));
    }
    if let Some(epss) = finding.epss {
        let _ = write!(line, " (EPSS: {epss:.2})");
    }
    line
}

/// Renders `report` for the terminal.
pub fn render(report: &Report, advisories: &AdvisorySeverities, opts: &TextOptions) -> String {
    let mut out = String::new();
    // Writing to a String can't fail.
    let _ = write_report(&mut out, report, advisories, opts);
    out
}

fn write_report(
    out: &mut String,
    report: &Report,
    advisories: &AdvisorySeverities,
    opts: &TextOptions,
) -> std::fmt::Result {
    if opts.verbosity == Verbosity::Quiet {
        for finding in &report.findings {
            let severity = severity_of(finding, advisories);
            writeln!(out, "{}", finding_line(finding, severity, opts.color))?;
        }
        return writeln!(out, "{}", summary_line(report));
    }
    let verbose = opts.verbosity == Verbosity::Verbose;

    writeln!(
        out,
        "Scanned {} ({} packages, {} pinned versions checked for known vulnerabilities)",
        report.path.display(),
        report.summary.packages,
        report.summary.queried
    )?;

    if verbose {
        writeln!(
            out,
            "\nDependencies checked ({}):",
            report.dependencies.len()
        )?;
        for dependency in &report.dependencies {
            write!(out, "  - {}", dependency.name)?;
            if let Some(version) = &dependency.version {
                write!(out, "@{version}")?;
            }
            let scope = if dependency.direct {
                "direct"
            } else {
                "transitive"
            };
            writeln!(out, " [{scope}]")?;
        }
    }

    if report.findings.is_empty() {
        writeln!(out, "\nNo findings.")?;
    } else {
        writeln!(out, "\nFindings ({}):", report.findings.len())?;
        for finding in &report.findings {
            let severity = severity_of(finding, advisories);
            writeln!(out, "  - {}", finding_line(finding, severity, opts.color))?;
        }
    }

    if !report.suppressed.is_empty() {
        writeln!(out, "\nSuppressed ({}):", report.suppressed.len())?;
        for suppressed in &report.suppressed {
            write!(out, "  - {} — {}", suppressed.finding, suppressed.reason)?;
            match suppressed.expires {
                Some(expires) => writeln!(out, " (until {expires})")?,
                None => writeln!(out)?,
            }
        }
    }

    if !report.warnings.is_empty() {
        writeln!(out, "\nWarnings ({}):", report.warnings.len())?;
        for warning in &report.warnings {
            writeln!(out, "  - {warning}")?;
        }
    }

    let unpinned = report.unpinned();
    if !unpinned.is_empty() {
        writeln!(
            out,
            "\nUnpinned ({}, not checked for known vulnerabilities):",
            unpinned.len()
        )?;
        for name in unpinned {
            writeln!(out, "  - {name}")?;
        }
    }

    if verbose && !report.timings.is_empty() {
        writeln!(out, "\nTimings:")?;
        for timing in &report.timings {
            writeln!(out, "  - {}", format_timing(timing))?;
        }
        let total: std::time::Duration = report.timings.iter().map(|t| t.elapsed).sum();
        writeln!(out, "  - total: {}", format_duration(total))?;
    }

    writeln!(out, "\n{}", summary_line(report))
}

/// The closing `Summary: ...` line.
pub fn summary_line(report: &Report) -> String {
    format!(
        "Summary: {} typosquat(s), {} vulnerability finding(s), {} suppressed, {} warning(s), \
         {} unpinned",
        report.summary.typosquats,
        report.summary.vulnerabilities,
        report.summary.suppressed,
        report.summary.warnings,
        report.summary.unpinned
    )
}

fn format_timing(timing: &StageTiming) -> String {
    format!("{}: {}", timing.stage, format_duration(timing.elapsed))
}

fn format_duration(elapsed: std::time::Duration) -> String {
    format!("{:.1} ms", elapsed.as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{Finding, FindingKind, Report, ReportSummary};

    #[test]
    fn no_color_wins_over_everything() {
        for choice in [ColorChoice::Auto, ColorChoice::Always, ColorChoice::Never] {
            assert!(!use_color(choice, Some("1"), true), "{choice:?}");
        }
    }

    #[test]
    fn empty_no_color_is_ignored() {
        assert!(use_color(ColorChoice::Auto, Some(""), true));
    }

    #[test]
    fn auto_colors_only_a_terminal() {
        assert!(use_color(ColorChoice::Auto, None, true));
        assert!(!use_color(ColorChoice::Auto, None, false));
        assert!(use_color(ColorChoice::Always, None, false));
        assert!(!use_color(ColorChoice::Never, None, true));
    }

    #[test]
    fn tags_are_plain_without_color() {
        assert_eq!(tag(Severity::Critical, false), "[CRITICAL]");
        assert_eq!(tag(Severity::Low, false), "[LOW]");
        assert!(tag(Severity::High, true).contains('\u{1b}'));
        assert!(tag(Severity::Medium, true).contains("[MEDIUM]"));
    }

    #[test]
    fn formats_stage_timings_in_milliseconds() {
        let timing = StageTiming {
            stage: "parse",
            elapsed: std::time::Duration::from_micros(1_240),
        };
        assert_eq!(format_timing(&timing), "parse: 1.2 ms");
    }

    #[test]
    fn malware_label_is_plain_without_color() {
        assert_eq!(malware_label(false), "malicious package");
        assert!(malware_label(true).contains('\u{1b}'));
    }

    #[test]
    fn mal_finding_renders_critical_with_malware_label() {
        use crate::cve::Finding;
        use crate::pipeline::{Report, Summary};

        let finding = Finding {
            package: "evil-crate".to_string(),
            version: Some("1.0.0".to_string()),
            advisory: "MAL-2024-1234".to_string(),
            summary: "Malicious package".to_string(),
            severity: None,
        };
        let report = Report {
            path: std::path::PathBuf::from("Cargo.lock"),
            findings: vec![finding],
            suppressed: Vec::new(),
            warnings: Vec::new(),
            dependencies: Vec::new(),
            timings: Vec::new(),
            summary: Summary {
                packages: 1,
                queried: 1,
                vulnerabilities: 1,
                ..Summary::default()
            },
        };
        let advisories = AdvisorySeverities::default();
        let opts = TextOptions {
            color: false,
            verbosity: Verbosity::Normal,
        };
        let rendered = render(&report, &advisories, &opts);
        assert!(rendered.contains("[CRITICAL]"), "{rendered}");
        assert!(rendered.contains("malicious package"), "{rendered}");
    }

    fn sample_finding() -> Finding {
        Finding {
            kind: FindingKind::Vulnerability,
            package: "openssl".to_string(),
            version: Some("1.0.0".to_string()),
            aliases: vec!["CVE-2021-44228".to_string()],
            known_exploited: true,
            epss: Some(0.97),
            ..Finding::default()
        }
    }

    fn sample_report(finding: Finding) -> Report {
        Report {
            path: std::path::PathBuf::from("Cargo.toml"),
            findings: vec![finding],
            summary: ReportSummary {
                vulnerabilities: 1,
                ..ReportSummary::default()
            },
            ..Report::default()
        }
    }

    #[test]
    fn renders_known_exploited_marker_and_epss_score() {
        let report = sample_report(sample_finding());
        let advisories = AdvisorySeverities::default();
        for verbosity in [Verbosity::Quiet, Verbosity::Normal] {
            let opts = TextOptions {
                color: false,
                verbosity,
            };
            let text = render(&report, &advisories, &opts);
            assert!(
                text.contains("[KNOWN EXPLOITED]"),
                "missing KEV marker in {verbosity:?}: {text}"
            );
            assert!(
                text.contains("EPSS: 0.97"),
                "missing EPSS score in {verbosity:?}: {text}"
            );
        }
    }

    #[test]
    fn omits_marker_and_score_when_absent() {
        let mut finding = sample_finding();
        finding.known_exploited = false;
        finding.epss = None;
        let report = sample_report(finding);
        let advisories = AdvisorySeverities::default();
        let opts = TextOptions {
            color: false,
            verbosity: Verbosity::Normal,
        };
        let text = render(&report, &advisories, &opts);
        assert!(!text.contains("[KNOWN EXPLOITED]"));
        assert!(!text.contains("EPSS:"));
    }
}