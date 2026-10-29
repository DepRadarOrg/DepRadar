//! `depradar` — check package names and project lockfiles for typosquats and
//! known vulnerabilities.
//!
//! Run `depradar --help` for usage.

use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::generate;
use serde_json::{json, Value};

use deprader::config::{load_project_config, ConfigFormat, FailOn, ProjectConfig};
use deprader::ignore::{IgnoreFile, Suppressed, IGNORE_FILE_NAME};
use deprader::model::{Ecosystem, Finding, FindingKind};
use deprader::pipeline::{
    scan_dir, scan_lockfile, watch_packages, DirReport, FakeCrawler as WatchFakeCrawler, Report,
    ScanOptions,
};
use deprader::report::{severity_of, AdvisorySeverities, Severity};
use deprader::typosquat::scanner::{score_package, ScanResult};

const EXAMPLES: &str = "\
Examples:
  depradar name expresss                 Check one package name for typosquat signals
  cat names.txt | depradar name -        Read newline-separated names from stdin
  depradar scan package-lock.json        Scan a lockfile for typosquats and known CVEs
  depradar scan .                        Scan every lockfile in a directory or monorepo
  depradar scan . --offline              Typosquat checks only, no OSV lookup
  depradar explain GHSA-xxxx-xxxx-xxxx   Show details for one advisory
  depradar --format json scan . | jq     Machine-readable output for CI
  depradar watch --registry npm          Watch the npm registry and score new packages

Exit codes:
  0  No findings
  1  At least one finding (`scan` only; suppressed findings don't count)
  2  Usage error, or the scan couldn't complete";

#[derive(Debug, Parser)]
#[command(
    name = "depradar",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("DEPRADAR_GIT_SHA"), ")"),
    about = "Check package names and lockfiles for typosquats and known vulnerabilities",
    after_help = EXAMPLES
)]
struct Cli {
    /// Output format. `json` prints exactly one JSON document to stdout;
    /// errors and warnings still go to stderr.
    #[arg(long, short = 'f', value_enum, global = true)]
    format: Option<Format>,

    /// Severity level at which the run exits with code 1 (`scan` only).
    /// Accepted values: low, medium, high, critical, none. Overrides
    /// `fail_on` in `depradar.toml`.
    #[arg(long, value_enum, global = true)]
    fail_on: Option<FailOnArg>,

    /// Suppress progress indicators and non-essential output
    #[arg(long, short = 'q', global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Score a single package name against the popular-package dataset
    Name {
        /// The package name to check, e.g. `expresss`
        package: String,
    },
    /// Scan a lockfile, or every lockfile under a directory
    Scan {
        /// A lockfile (package-lock.json, npm-shrinkwrap.json,
        /// requirements*.txt) or a directory to search for them
        path: PathBuf,
        /// Skip the OSV vulnerability lookup (typosquat checks only)
        #[arg(long)]
        offline: bool,
        /// Glob patterns for paths to skip (may be repeated). Overrides
        /// `exclude` in `depradar.toml`.
        #[arg(long = "exclude", value_name = "GLOB")]
        exclude: Vec<String>,
        /// Restrict scanning to these ecosystems (may be repeated). Overrides
        /// `ecosystems` in `depradar.toml`.
        #[arg(long = "ecosystem", value_name = "ECOSYSTEM")]
        ecosystems: Vec<String>,
    },
    /// Show details for a single advisory (summary, severity, affected
    /// ranges, fixed version and references)
    Explain {
        /// The advisory ID to look up, e.g. `GHSA-xxxx-xxxx-xxxx`
        id: String,
    },
    /// Watch a package registry and emit typosquat findings as JSON lines
    Watch {
        /// The registry to watch
        #[arg(long, value_enum, default_value_t = Registry::Npm)]
        registry: Registry,
        /// Minimum confidence score (0-100) to emit a finding
        #[arg(long, default_value_t = 40)]
        min_confidence: u8,
    },
    /// Generate shell completion scripts to stdout
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

/// Package registry to watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Registry {
    Npm,
    Pypi,
    Crates,
}

/// CLI mirror of [`FailOn`] so clap can generate the `--help` values list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FailOnArg {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl From<FailOnArg> for FailOn {
    fn from(arg: FailOnArg) -> Self {
        match arg {
            FailOnArg::None => FailOn::None,
            FailOnArg::Low => FailOn::Low,
            FailOnArg::Medium => FailOn::Medium,
            FailOnArg::High => FailOn::High,
            FailOnArg::Critical => FailOn::Critical,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Shell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

impl From<Shell> for clap_complete::Shell {
    fn from(shell: Shell) -> Self {
        match shell {
            Shell::Bash => clap_complete::Shell::Bash,
            Shell::Zsh => clap_complete::Shell::Zsh,
            Shell::Fish => clap_complete::Shell::Fish,
            Shell::Powershell => clap_complete::Shell::PowerShell,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Text,
    Json,
}

/// Resolved configuration after merging defaults ← file ← CLI flags.
struct ResolvedConfig {
    format: Format,
    fail_on: FailOn,
    exclude: Vec<String>,
    /// Empty means "all ecosystems".
    ecosystems: Vec<String>,
}

impl ResolvedConfig {
    /// Merge: defaults ← `depradar.toml` values ← CLI flags.
    ///
    /// The `file` argument is the parsed `depradar.toml`, if one was found.
    /// `cli_*` arguments are `Some` only when the user explicitly passed the
    /// flag; otherwise the file (or built-in default) wins.
    fn resolve(
        file: Option<&ProjectConfig>,
        cli_format: Option<Format>,
        cli_fail_on: Option<FailOnArg>,
        cli_exclude: Vec<String>,
        cli_ecosystems: Vec<String>,
    ) -> Self {
        // Format: CLI flag > file > default (text)
        let format = cli_format.unwrap_or_else(|| {
            file.and_then(|f| f.format)
                .map(config_format_to_cli)
                .unwrap_or(Format::Text)
        });

        // fail_on: CLI flag > file > default (high)
        let fail_on = cli_fail_on
            .map(FailOn::from)
            .unwrap_or_else(|| file.and_then(|f| f.fail_on).unwrap_or_default());

        // exclude: CLI flag (if non-empty) > file > default (empty)
        let exclude = if !cli_exclude.is_empty() {
            cli_exclude
        } else {
            file.map(|f| f.exclude.clone()).unwrap_or_default()
        };

        // ecosystems: CLI flag (if non-empty) > file > default (empty = all)
        let ecosystems = if !cli_ecosystems.is_empty() {
            cli_ecosystems
        } else {
            file.map(|f| f.ecosystems.iter().map(|e| e.to_string()).collect())
                .unwrap_or_default()
        };

        ResolvedConfig {
            format,
            fail_on,
            exclude,
            ecosystems,
        }
    }
}

fn config_format_to_cli(f: ConfigFormat) -> Format {
    match f {
        ConfigFormat::Text => Format::Text,
        ConfigFormat::Json => Format::Json,
    }
}

fn severity_from_fail_on(fail_on: FailOn) -> Option<Severity> {
    match fail_on {
        FailOn::None => None,
        FailOn::Low => Some(Severity::Low),
        FailOn::Medium => Some(Severity::Medium),
        FailOn::High => Some(Severity::High),
        FailOn::Critical => Some(Severity::Critical),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Load the project config from the working directory (best-effort).
    let project_config = match load_project_config(&std::env::current_dir().unwrap_or_default()) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };

    match cli.command {
        Command::Name { package } => run_name(&package, cli.format),
        Command::Scan { path, offline } => match scan(&path, offline, cli.quiet) {
            Ok(scanned) => {
                match (cli.format, &scanned) {
                    (Format::Text, Scanned::File(report)) => println!("{report}"),
                    (Format::Text, Scanned::Dir(report)) => println!("{report}"),
                    (Format::Json, scanned) => print_json(&scanned_json(scanned)),
                }
                if scanned.has_findings() {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("error: {message}");
                    if resolved.format == Format::Json {
                        print_json(&json!({ "error": message }));
                    }
                    ExitCode::from(2)
                }
            }
        }
        Command::Explain { id } => run_explain(&id, cli.format),
        Command::Watch {
            registry,
            min_confidence,
        } => run_watch(registry, min_confidence),
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            generate(
                clap_complete::Shell::from(shell),
                &mut cmd,
                "depradar",
                &mut io::stdout(),
            );
            ExitCode::SUCCESS
        }
    }
}

/// Returns true when the scan has at least one finding at or above the
/// `fail_on` threshold.
fn scanned_exceeds_threshold(scanned: &Scanned, fail_on: FailOn) -> bool {
    let Some(threshold) = severity_from_fail_on(fail_on) else {
        return false;
    };

    let findings: &[Finding] = match scanned {
        Scanned::File(r) => &r.findings,
        Scanned::Dir(r) => {
            // Check each lockfile's findings.
            return r
                .lockfiles
                .iter()
                .any(|r| findings_exceed_threshold(&r.findings, threshold));
        }
    };
    findings_exceed_threshold(findings, threshold)
}

fn findings_exceed_threshold(findings: &[Finding], threshold: Severity) -> bool {
    if findings.is_empty() {
        return false;
    }
    // Fetch advisory severities for vulnerability findings in this set.
    let (advisories, _) = AdvisorySeverities::fetch(findings);
    findings
        .iter()
        .any(|f| severity_of(f, &advisories) >= threshold)
}

/// Scores one package name, or every package name read from stdin when
/// `package` is `-`.
fn run_name(package: &str, format: Format) -> ExitCode {
    if package != "-" {
        let result = score_package(package);
        match format {
            Format::Text => println!("{result}"),
            Format::Json => print_json(&score_json(&result)),
        }
        return ExitCode::SUCCESS;
    }

    let names = match read_names() {
        Ok(names) => names,
        Err(message) => {
            eprintln!("error: {message}");
            if format == Format::Json {
                print_json(&json!({ "error": message }));
            }
            return ExitCode::from(2);
        }
    };

    match format {
        Format::Text => {
            for name in &names {
                println!("{}", score_package(name));
            }
        }
        Format::Json => {
            let results = names
                .iter()
                .map(|name| score_json(&score_package(name)))
                .collect::<Vec<_>>();
            print_json(&Value::Array(results));
        }
    }
    ExitCode::SUCCESS
}

/// Reads one package name per line from stdin, skipping blank lines and
/// `#` comments.
fn read_names() -> Result<Vec<String>, String> {
    let stdin = io::stdin();
    let mut names = Vec::new();
    for line in stdin.lock().lines() {
        let line = line.map_err(|err| format!("failed to read stdin: {err}"))?;
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        names.push(name.to_string());
    }
    Ok(names)
}

/// What `depradar scan` scanned: one lockfile, or a directory tree.
enum Scanned {
    File(Report),
    Dir(DirReport),
}

/// Returns `true` when `lockfile_path` should be excluded based on
/// `exclude` glob patterns.
///
/// Patterns are matched against the lockfile path as a string (forward
/// slashes). A pattern without a directory separator is matched against the
/// file name only; one that contains `/` is matched against the full relative
/// path.
fn is_excluded(lockfile_path: &Path, exclude: &[String]) -> bool {
    if exclude.is_empty() {
        return false;
    }
    let path_str = lockfile_path.to_string_lossy();
    exclude.iter().any(|pattern| {
        // Simple glob: `*` matches any sequence of non-separator chars,
        // `**` matches any sequence including separators.
        glob_match(pattern, &path_str)
    })
}

/// Minimal glob matcher supporting `*` (non-separator) and `**` (any).
///
/// This is intentionally simple — for production-grade glob support the
/// `glob` or `globset` crate should be added. The current implementation is
/// sufficient for the common CI patterns (`vendor/**`, `third_party/**`).
fn glob_match(pattern: &str, text: &str) -> bool {
    // Normalise to forward slashes for cross-platform consistency.
    let text = text.replace('\\', "/");
    glob_match_inner(pattern.as_bytes(), text.as_bytes())
}

fn glob_match_inner(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(b'*'), _) => {
            if pattern.get(1) == Some(&b'*') {
                // `**` — match any sequence including `/`
                let rest = &pattern[2..];
                // skip the optional trailing separator after `**`
                let rest = rest.strip_prefix(b"/").unwrap_or(rest);
                for i in 0..=text.len() {
                    if glob_match_inner(rest, &text[i..]) {
                        return true;
                    }
                }
                false
            } else {
                // `*` — match any sequence of non-separator chars
                let rest = &pattern[1..];
                for i in 0..=text.len() {
                    if text[..i].contains(&b'/') {
                        break;
                    }
                    if glob_match_inner(rest, &text[i..]) {
                        return true;
                    }
                }
                false
            }
        }
        (Some(p), Some(t)) => *p == *t && glob_match_inner(&pattern[1..], &text[1..]),
        (Some(_), None) => false,
    }
}

/// Scans `path` as a single lockfile or, for a directory, every lockfile
/// under it, honouring a `.depradarignore` next to the lockfile (or at the
/// directory's root).
fn scan(path: &Path, offline: bool, quiet: bool) -> Result<Scanned, String> {
    let ignore_dir = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or_else(|| Path::new(""))
    };
    let ignore_path = ignore_dir.join(IGNORE_FILE_NAME);
    let ignore = if ignore_path.is_file() {
        Some(IgnoreFile::load(&ignore_path).map_err(|err| err.to_string())?)
    } else {
        None
    };

    // Check whether the target path is excluded before scanning.
    if is_excluded(path, exclude) {
        // Return an empty dir report so the caller gets a clean exit code.
        return Ok(Scanned::Dir(DirReport {
            root: path.to_path_buf(),
            lockfiles: Vec::new(),
            errors: Vec::new(),
        }));
    }

    let mut opts = ScanOptions {
        ignore: ignore.as_ref(),
        ..ScanOptions::default()
    };
    if offline {
        opts.advisories = None;
    }
    if quiet {
        opts.show_progress = false;
    }

    if path.is_dir() {
        let mut report =
            scan_dir(path, &opts).map_err(|err| format!("{}: {err}", path.display()))?;
        // Filter out lockfiles that match an exclude pattern.
        if !exclude.is_empty() {
            report.lockfiles.retain(|r| !is_excluded(&r.path, exclude));
        }
        Ok(Scanned::Dir(report))
    } else {
        scan_lockfile(path, &opts)
            .map(Scanned::File)
            .map_err(|err| err.to_string())
    }
}

/// Fetches one advisory from OSV and prints its details.
fn run_explain(id: &str, format: Format) -> ExitCode {
    let client = deprader::osv::OsvClient::new();
    let vuln = match client.fetch_vuln(id) {
        Ok(vuln) => vuln,
        Err(err) => {
            let message = err.to_string();
            eprintln!("error: {message}");
            if format == Format::Json {
                print_json(&json!({ "error": message }));
            }
            return ExitCode::from(2);
        }
    };

    match format {
        Format::Text => print!("{}", explain_text(&vuln)),
        Format::Json => print_json(&explain_json(&vuln)),
    }
    ExitCode::SUCCESS
}

/// Renders an advisory as human-readable text.
fn explain_text(vuln: &deprader::osv::Vuln) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = writeln!(out, "{}", vuln.id);
    if let Some(summary) = &vuln.summary {
        let _ = writeln!(out, "{summary}");
    }
    if let Some(severity) = vuln.severity.first() {
        let _ = writeln!(out, "Severity: {} ({})", severity.score, severity.kind);
    }
    for affected in &vuln.affected {
        let _ = writeln!(out, "Affected: {}", affected.package.name);
        for range in &affected.ranges {
            for event in &range.events {
                if let Some(introduced) = &event.introduced {
                    let _ = writeln!(out, "  introduced: {introduced}");
                }
                if let Some(fixed) = &event.fixed {
                    let _ = writeln!(out, "  fixed: {fixed}");
                }
            }
        }
    }
    if !vuln.references.is_empty() {
        let _ = writeln!(out, "References:");
        for reference in &vuln.references {
            let _ = writeln!(out, "  {}", reference.url);
        }
    }
    out
}

/// Renders an advisory as a single JSON document.
fn explain_json(vuln: &deprader::osv::Vuln) -> Value {
    json!({
        "id": vuln.id,
        "summary": vuln.summary,
        "severity": vuln
            .severity
            .iter()
            .map(|s| json!({ "type": s.kind, "score": s.score }))
            .collect::<Vec<_>>(),
        "affected": vuln
            .affected
            .iter()
            .map(|a| json!({
                "package": a.package.name,
                "ranges": a
                    .ranges
                    .iter()
                    .map(|r| json!({
                        "events": r
                            .events
                            .iter()
                            .map(|e| json!({
                                "introduced": e.introduced,
                                "fixed": e.fixed,
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "references": vuln
            .references
            .iter()
            .map(|r| json!({ "type": r.kind, "url": r.url }))
            .collect::<Vec<_>>(),
    })
}

/// Runs the watch loop for `registry`, printing each finding as a JSON line to
/// stdout. Findings below `min_confidence` are silently dropped.
fn run_watch(registry: Registry, min_confidence: u8) -> ExitCode {
    use deprader::pipeline::WATCH_CONFIDENCE_THRESHOLD;

    let registry_name = match registry {
        Registry::Npm => "npm",
        Registry::Pypi => "pypi",
        Registry::Crates => "crates",
    };
    eprintln!("Watching {registry_name} for new packages (min_confidence={min_confidence})…");

    // In production this would be replaced by a real registry poller.
    // For now we use a FakeCrawler with an empty list so the binary exits
    // cleanly; the real crawler will be wired in once the registry clients
    // land.
    let crawler = WatchFakeCrawler::new(std::iter::empty::<String>(), 64);
    let threshold = min_confidence.max(WATCH_CONFIDENCE_THRESHOLD);
    watch_packages(crawler, |line| {
        // Re-parse to apply the caller-supplied threshold; the inner loop
        // already applied WATCH_CONFIDENCE_THRESHOLD.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if v["confidence"].as_u64().unwrap_or(0) >= threshold as u64 {
                println!("{line}");
            }
        }
    });
    ExitCode::SUCCESS
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("a serde_json::Value always serializes")
    );
}

/// The registry name as OSV spells it.
fn ecosystem_name(ecosystem: Ecosystem) -> &'static str {
    match ecosystem {
        Ecosystem::Npm => "npm",
        Ecosystem::PyPI => "PyPI",
        Ecosystem::CratesIo => "crates.io",
        Ecosystem::Go => "Go",
        Ecosystem::Maven => "Maven",
        Ecosystem::RubyGems => "RubyGems",
        Ecosystem::Packagist => "Packagist",
    }
}

fn score_json(result: &ScanResult) -> Value {
    json!({
        "candidate": result.candidate,
        "likely_typosquat": result.is_likely_typosquat(),
        "confidence": result.confidence,
        "distance_match": result
            .distance_match
            .as_ref()
            .map(|(name, distance)| json!({ "name": name, "distance": distance })),
        "homoglyph_match": result.homoglyph_match,
        "digit_swap_match": result.digit_swap_match,
        "keyboard_typo": result.keyboard_typo,
        "scope_confusion": result.scope_confusion,
        "combosquat": result.combosquat,
    })
}

/// One shape for both scan targets, so consumers needn't care which was
/// scanned: a single file is a tree with one lockfile and no errors.
fn scanned_json(scanned: &Scanned) -> Value {
    match scanned {
        Scanned::File(report) => json!({
            "root": report.path,
            "lockfiles": [report_json(report)],
            "errors": [],
        }),
        Scanned::Dir(report) => json!({
            "root": report.root,
            "lockfiles": report.lockfiles.iter().map(report_json).collect::<Vec<_>>(),
            "errors": report
                .errors
                .iter()
                .map(|err| json!({ "path": err.path, "message": err.error.to_string() }))
                .collect::<Vec<_>>(),
        }),
    }
}

fn report_json(report: &Report) -> Value {
    let summary = &report.summary;
    json!({
        "path": report.path,
        "ecosystem": ecosystem_name(report.ecosystem),
        "summary": {
            "packages": summary.packages,
            "queried": summary.queried,
            "typosquats": summary.typosquats,
            "vulnerabilities": summary.vulnerabilities,
            "suppressed": summary.suppressed,
            "warnings": summary.warnings,
        },
        "findings": report.findings.iter().map(finding_json).collect::<Vec<_>>(),
        "suppressed": report.suppressed.iter().map(suppressed_json).collect::<Vec<_>>(),
        "warnings": report.warnings,
    })
}

fn finding_json(finding: &Finding) -> Value {
    let mut value = json!({
        "package": finding.package,
        "version": finding.version,
        "ecosystem": ecosystem_name(finding.ecosystem),
        "direct": finding.direct,
        "path": finding.path,
    });
    let kind = match &finding.kind {
        FindingKind::Typosquat {
            distance_match,
            homoglyph_match,
        } => json!({
            "kind": "typosquat",
            "distance_match": distance_match
                .as_ref()
                .map(|(name, distance)| json!({ "name": name, "distance": distance })),
            "homoglyph_match": homoglyph_match,
        }),
        FindingKind::Vulnerability { advisory_id } => json!({
            "kind": "vulnerability",
            "advisory_id": advisory_id,
        }),
        FindingKind::InstallScript {
            script_name,
            script_command,
        } => json!({
            "kind": "install_script",
            "script_name": script_name,
            "script_command": script_command,
        }),
    };
    if let (Value::Object(fields), Value::Object(extra)) = (&mut value, kind) {
        fields.extend(extra);
    }
    value
}

fn suppressed_json(suppressed: &Suppressed) -> Value {
    json!({
        "finding": finding_json(&suppressed.finding),
        "reason": suppressed.reason,
        "expires": suppressed.expires.map(|date| date.to_string()),
    })
}
