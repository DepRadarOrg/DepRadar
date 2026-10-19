//! `depradar` — check package names and project lockfiles for typosquats and
//! known vulnerabilities.
//!
//! Run `depradar --help` for usage.

use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};

use deprader::ignore::{IgnoreFile, Suppressed, IGNORE_FILE_NAME};
use deprader::model::{Ecosystem, Finding, FindingKind};
use deprader::pipeline::{scan_dir, scan_lockfile, DirReport, Report, ScanOptions};
use deprader::typosquat::scanner::{score_package, ScanResult};

const EXAMPLES: &str = "\
Examples:
  depradar name expresss                 Check one package name for typosquat signals
  cat names.txt | depradar name -        Read newline-separated names from stdin
  depradar scan package-lock.json        Scan a lockfile for typosquats and known CVEs
  depradar scan .                        Scan every lockfile in a directory or monorepo
  depradar scan . --offline              Typosquat checks only, no OSV lookup
  depradar --format json scan . | jq     Machine-readable output for CI

Exit codes:
  0  No findings
  1  At least one finding (`scan` only; suppressed findings don't count)
  2  Usage error, or the scan couldn't complete";

#[derive(Debug, Parser)]
#[command(
    name = "depradar",
    version,
    about = "Check package names and lockfiles for typosquats and known vulnerabilities",
    after_help = EXAMPLES
)]
struct Cli {
    /// Output format. `json` prints exactly one JSON document to stdout;
    /// errors and warnings still go to stderr.
    #[arg(long, short = 'f', value_enum, default_value_t = Format::Text, global = true)]
    format: Format,

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
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Text,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Name { package } => run_name(&package, cli.format),
        Command::Scan { path, offline } => match scan(&path, offline) {
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
            }
            Err(message) => {
                eprintln!("error: {message}");
                if cli.format == Format::Json {
                    print_json(&json!({ "error": message }));
                }
                ExitCode::from(2)
            }
        },
    }
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

impl Scanned {
    fn has_findings(&self) -> bool {
        match self {
            Scanned::File(report) => report.has_findings(),
            Scanned::Dir(report) => report.has_findings(),
        }
    }
}

/// Scans `path` as a single lockfile or, for a directory, every lockfile
/// under it, honouring a `.depradarignore` next to the lockfile (or at the
/// directory's root).
fn scan(path: &Path, offline: bool) -> Result<Scanned, String> {
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

    let mut opts = ScanOptions {
        ignore: ignore.as_ref(),
        ..ScanOptions::default()
    };
    if offline {
        opts.advisories = None;
    }

    if path.is_dir() {
        scan_dir(path, &opts)
            .map(Scanned::Dir)
            .map_err(|err| format!("{}: {err}", path.display()))
    } else {
        scan_lockfile(path, &opts)
            .map(Scanned::File)
            .map_err(|err| err.to_string())
    }
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
