//! `depradar` — check package names and project lockfiles for typosquats and
//! known vulnerabilities.
//!
//! Run `depradar --help` for usage.

#[path = "depradar/repo_commands.rs"]
mod repo_commands;

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
    scan_dir, scan_dir_changed_since, scan_lockfile, watch_packages, DirReport,
    FakeCrawler as WatchFakeCrawler, Report, ScanOptions,
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
  depradar scan . --deep                 Also run behavioral checks on each dependency
  depradar db update                     Download and index the OSV dumps locally
  depradar scan . --offline-db           Scan using the local OSV database
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
    /// Scan repository files and write a private JSON report to stdout
    RepoScan(repo_commands::ScanArgs),
    /// Render the README badge from the latest publishable report
    RepoBadge {
        /// JSON scan report. Keep the SVG private unless cleared for release.
        report: PathBuf,
        /// Apply disclosure checks for public hosting.
        #[arg(long)]
        public: bool,
        #[arg(long, requires = "public")]
        receipt: Option<PathBuf>,
    },
    /// Open a testnet bounty for a confirmed repository finding
    RepoBounty(repo_commands::BountyArgs),
    /// Record that maintainers received the report privately
    RepoDisclose {
        report: PathBuf,
        receipt: PathBuf,
        /// Earliest public release time, expressed as Unix seconds
        #[arg(long)]
        embargo_until: u64,
    },
    /// Release a report after its recorded embargo expires
    RepoPublish { report: PathBuf, receipt: PathBuf },
    /// Score a single package name against the popular-package dataset
    Name {
        /// The package name to check, e.g. `expresss`
        package: String,
    },
    /// Manage the local OSV database used by `scan --offline-db`
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
    /// Scan a lockfile, or every lockfile under a directory
    Scan {
        /// A lockfile (package-lock.json, npm-shrinkwrap.json,
        /// requirements*.txt) or a directory to search for them
        path: PathBuf,
        /// Skip the OSV vulnerability lookup (typosquat checks only)
        #[arg(long)]
        offline: bool,
        /// Run behavioral checks on each dependency (downloads npm tarballs
        /// and inspects install scripts and obfuscation)
        #[arg(long)]
        deep: bool,
        /// Use the local OSV database (see `depradar db update`) instead of
        /// calling osv.dev. Implies no network access for vulnerability data.
        #[arg(long)]
        offline_db: bool,
        /// Glob patterns for paths to skip (may be repeated). Overrides
        /// `exclude` in `depradar.toml`.
        #[arg(long = "exclude", value_name = "GLOB")]
        exclude: Vec<String>,
        /// Restrict scanning to these ecosystems (may be repeated). Overrides
        /// `ecosystems` in `depradar.toml`.
        #[arg(long = "ecosystem", value_name = "ECOSYSTEM")]
        ecosystems: Vec<String>,
        /// Only scan lockfiles changed since this git ref, as listed by
        /// `git diff --name-only <GIT_REF>` (directory scans only)
        #[arg(long, value_name = "GIT_REF")]
        changed_since: Option<String>,
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
    /// Inspect or clear the OSV response cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

/// Subcommands for `depradar cache`.
#[derive(Debug, Subcommand)]
enum CacheAction {
    /// Print the cache path, entry count and total size
    Info,
    /// Delete the cache directory
    Clear,
}

/// Subcommands for managing the local OSV database.
#[derive(Debug, Subcommand)]
enum DbCommand {
    /// Download and index the OSV ecosystem dumps into the local database
    Update {
        /// Directory to store the local database in. Defaults to
        /// `$DEPRADAR_DB_DIR`, or the platform data directory.
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
        /// Restrict the download to these ecosystems (may be repeated).
        /// Defaults to every supported ecosystem.
        #[arg(long = "ecosystem", value_name = "ECOSYSTEM")]
        ecosystems: Vec<String>,
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
        Command::RepoScan(args) => repo_result(repo_commands::scan(
            args,
            project_config
                .as_ref()
                .map_or(&[], |config| config.exclude.as_slice()),
        )),
        Command::RepoBadge {
            report,
            public,
            receipt,
        } => repo_result(repo_commands::badge(&report, receipt.as_deref(), public)),
        Command::RepoDisclose {
            report,
            receipt,
            embargo_until,
        } => repo_result(repo_commands::record_disclosure(
            &report,
            &receipt,
            embargo_until,
        )),
        Command::RepoPublish { report, receipt } => {
            repo_result(repo_commands::publish(&report, &receipt))
        }
        Command::RepoBounty(args) => repo_result(repo_commands::bounty(args)),
        Command::Name { package } => run_name(&package, cli.format.unwrap_or(Format::Text)),
        Command::Db { command } => match command {
            DbCommand::Update { dir, ecosystems } => run_db_update(dir, &ecosystems),
        },
        Command::Scan {
            path,
            offline,
            deep,
            offline_db,
            exclude,
            ecosystems,
            changed_since,
        } => {
            let config = ResolvedConfig::resolve(
                project_config.as_ref(),
                cli.format,
                cli.fail_on,
                exclude,
                ecosystems,
            );
            match scan(
                &path,
                offline,
                deep,
                offline_db,
                cli.quiet,
                changed_since.as_deref(),
                &config.exclude,
                &config.ecosystems,
            ) {
                Ok(scanned) => {
                    let failed = scanned_exceeds_threshold(&scanned, config.fail_on);
                    match (config.format, &scanned) {
                        (Format::Text, Scanned::File(report)) => println!("{report}"),
                        (Format::Text, Scanned::Dir(report)) => println!("{report}"),
                        (Format::Json, scanned) => print_json(&scanned_json(scanned)),
                    }
                    if failed {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("error: {message}");
                    if config.format == Format::Json {
                        print_json(&json!({ "error": message }));
                    }
                    ExitCode::from(2)
                }
            }
        }
        Command::Explain { id } => run_explain(&id, cli.format.unwrap_or(Format::Text)),
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
        Command::Cache { action } => run_cache(action, cli.format.unwrap_or(Format::Text)),
    }
}

/// Runs `depradar cache info` or `depradar cache clear`.
fn run_cache(action: CacheAction, format: Format) -> ExitCode {
    use deprader::cve::cache;

    match action {
        CacheAction::Info => {
            let info = match cache::cache_info() {
                Ok(info) => info,
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
                Format::Text => {
                    println!("Cache path: {}", info.path.display());
                    println!("Entries: {}", info.entries);
                    println!("Size: {} bytes", info.size_bytes);
                }
                Format::Json => print_json(&json!({
                    "path": info.path,
                    "entries": info.entries,
                    "size_bytes": info.size_bytes,
                })),
            }
            ExitCode::SUCCESS
        }
        CacheAction::Clear => match cache::clear_cache() {
            Ok(path) => {
                match format {
                    Format::Text => println!("Cleared cache at {}", path.display()),
                    Format::Json => print_json(&json!({ "cleared": path })),
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                let message = err.to_string();
                eprintln!("error: {message}");
                if format == Format::Json {
                    print_json(&json!({ "error": message }));
                }
                ExitCode::from(2)
            }
        },
    }
}

fn repo_result(result: deprader::repo_scan::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
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

/// Downloads and indexes the OSV ecosystem dumps into the local database.
fn run_db_update(dir: Option<PathBuf>, ecosystems: &[String]) -> ExitCode {
    let db_dir = match dir {
        Some(dir) => dir,
        None => match deprader::cve::offline_db::default_db_dir() {
            Ok(dir) => dir,
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::from(2);
            }
        },
    };

    let selected = if ecosystems.is_empty() {
        deprader::cve::offline_db::supported_ecosystems()
    } else {
        ecosystems
            .iter()
            .filter_map(|name| Ecosystem::from_ref_str(name))
            .collect::<Vec<_>>()
    };
    if selected.is_empty() {
        eprintln!("error: no supported ecosystems selected");
        return ExitCode::from(2);
    }

    eprintln!(
        "Updating local OSV database in {} ({} ecosystem(s))…",
        db_dir.display(),
        selected.len()
    );
    match deprader::cve::offline_db::update(&db_dir, &selected) {
        Ok(summary) => {
            eprintln!(
                "Indexed {} advisories across {} ecosystem(s) into {}",
                summary.advisories,
                summary.ecosystems,
                db_dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}

/// Scans `path` as a single lockfile or, for a directory, every lockfile
/// under it, honouring a `.depradarignore` next to the lockfile (or at the
/// directory's root).
fn scan(
    path: &Path,
    offline: bool,
    deep: bool,
    offline_db: bool,
    quiet: bool,
    changed_since: Option<&str>,
    exclude: &[String],
    ecosystems: &[String],
) -> Result<Scanned, String> {
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
    if is_excluded(path, exclude) || (!path.is_dir() && !selected_ecosystem(path, ecosystems)) {
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
    } else if offline_db {
        let db_dir = deprader::cve::offline_db::default_db_dir().map_err(|err| err.to_string())?;
        let index = deprader::cve::offline_db::OfflineDb::open(&db_dir)
            .map_err(|err| format!("{}: {err}", db_dir.display()))?;
        opts.advisories = Some(Box::new(index));
    }
    if quiet {
        opts.show_progress = false;
    }
    if deep {
        opts.deep = true;
    }

    if path.is_dir() {
        let mut report = match changed_since {
            Some(git_ref) => scan_dir_changed_since(path, &opts, git_ref),
            None => scan_dir(path, &opts),
        }
        .map_err(|err| format!("{}: {err}", path.display()))?;
        // Filter out lockfiles that match an exclude pattern.
        report.lockfiles.retain(|r| {
            !is_excluded(r.path.strip_prefix(path).unwrap_or(&r.path), exclude)
                && selected_ecosystem(&r.path, ecosystems)
        });
        if !offline {
            for lockfile in &mut report.lockfiles {
                enrich_findings(&mut lockfile.findings);
            }
        }
        Ok(Scanned::Dir(report))
    } else {
        let mut report = scan_lockfile(path, &opts).map_err(|err| err.to_string())?;
        if !offline {
            enrich_findings(&mut report.findings);
        }
        Ok(Scanned::File(report))
    }
}

/// Enriches vulnerability findings with KEV and EPSS data.
///
/// Failures are non-fatal: a missing feed simply leaves the fields unset.
fn enrich_findings(findings: &mut [Finding]) {
    use deprader::cve::{epss, kev};

    let cve_ids: Vec<String> = findings
        .iter()
        .filter_map(|finding| match &finding.kind {
            FindingKind::Vulnerability { advisory_id } => Some(advisory_id.clone()),
            _ => None,
        })
        .collect();
    if cve_ids.is_empty() {
        return;
    }

    let kev_ids = kev::known_exploited_ids().unwrap_or_default();
    let epss_scores = epss::scores_for(&cve_ids).unwrap_or_default();

    for finding in findings.iter_mut() {
        if let FindingKind::Vulnerability { advisory_id } = &finding.kind {
            if kev_ids.contains(advisory_id) {
                finding.known_exploited = true;
            }
            if let Some(score) = epss_scores.get(advisory_id) {
                finding.epss_score = Some(*score);
            }
        }
    }
}

fn selected_ecosystem(path: &Path, ecosystems: &[String]) -> bool {
    use deprader::lockfile::{detect, LockfileKind};
    if ecosystems.is_empty() {
        return true;
    }
    let ecosystem = match detect(path) {
        Some(LockfileKind::CargoLock) => Ecosystem::CratesIo,
        Some(LockfileKind::Requirements | LockfileKind::PipfileLock | LockfileKind::PoetryLock) => {
            Ecosystem::PyPI
        }
        Some(_) => Ecosystem::Npm,
        None => return false,
    };
    ecosystems.iter().any(|name| {
        Ecosystem::from_ref_str(name) == Some(ecosystem)
            || ecosystem.to_string().eq_ignore_ascii_case(name)
    })
}

/// Fetches one advisory from OSV and prints its details.
fn run_explain(id: &str, format: Format) -> ExitCode {
    let client = deprader::cve::osv_client::OsvClient::new();
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
fn explain_text(vuln: &deprader::cve::OsvVulnerability) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = writeln!(out, "{}", vuln.id);
    if let Some(summary) = &vuln.summary {
        let _ = writeln!(out, "{summary}");
    }
    if let Some(severity) = vuln.severity.first() {
        let _ = writeln!(
            out,
            "Severity: {} ({})",
            severity.score, severity.severity_type
        );
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
fn explain_json(vuln: &deprader::cve::OsvVulnerability) -> Value {
    json!({
        "id": vuln.id,
        "summary": vuln.summary,
        "severity": vuln
            .severity
            .iter()
            .map(|s| json!({ "type": s.severity_type, "score": s.score }))
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
            .map(|r| json!({ "type": r.reference_type, "url": r.url }))
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
            "unpinned": summary.unpinned,
        },
        "findings": report.findings.iter().map(finding_json).collect::<Vec<_>>(),
        "suppressed": report.suppressed.iter().map(suppressed_json).collect::<Vec<_>>(),
        "warnings": report.warnings,
        "unpinned": report.unpinned(),
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
            "known_exploited": finding.known_exploited,
            "epss_score": finding.epss_score,
        }),
        FindingKind::Deprecated { message } => json!({"kind": "deprecated", "message": message}),
        FindingKind::License { license, allowed } => {
            json!({"kind": "license", "license": license, "allowed": allowed})
        }
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
