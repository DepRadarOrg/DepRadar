//! End-to-end scan of a project lockfile: parse it, run the typosquat scorer
//! over every package name, check every pinned version against OSV, drop
//! anything accepted in a `.depradarignore` file ([`crate::ignore`]), and
//! collect the results into one [`Report`]. [`scan_dir`] does the same for
//! every lockfile in a directory tree, such as a monorepo.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cve::osv_client::{query_osv_batch, OsvBatchQuery};
use crate::ignore::{Date, IgnoreFile, Suppressed};
use crate::lockfile::{parse_lockfile, LockfileError};
use crate::model::{Dependency, Ecosystem, Finding, FindingKind};
use crate::typosquat::scanner::{score_package, ScanResult};

/// Error type an [`AdvisorySource`] can fail with.
pub type AdvisoryError = Box<dyn Error + Send + Sync>;

/// Where known-vulnerability data comes from. [`OsvAdvisories`] queries the
/// live OSV API; tests substitute their own.
pub trait AdvisorySource {
    /// Returns, for each query in order, the IDs of the advisories affecting
    /// that exact package version (empty if none).
    fn query(&self, queries: &[OsvBatchQuery<'_>]) -> Result<Vec<Vec<String>>, AdvisoryError>;
}

/// [`AdvisorySource`] backed by OSV's batch API ([`query_osv_batch`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct OsvAdvisories;

impl AdvisorySource for OsvAdvisories {
    fn query(&self, queries: &[OsvBatchQuery<'_>]) -> Result<Vec<Vec<String>>, AdvisoryError> {
        let results = query_osv_batch(queries)?;
        Ok(results
            .into_iter()
            .map(|result| result.vulns.into_iter().map(|vuln| vuln.id).collect())
            .collect())
    }
}

/// What [`scan_lockfile`] checks.
#[derive(Clone, Copy)]
pub struct ScanOptions<'a> {
    /// Score every package name with [`score_package`].
    pub typosquat: bool,
    /// Where to look up known vulnerabilities, or `None` to skip CVE checks.
    pub advisories: Option<&'a dyn AdvisorySource>,
    /// Accepted findings to suppress, or `None` to report everything.
    pub ignore: Option<&'a IgnoreFile>,
    /// The date suppression expiry is judged against; `None` means today.
    pub today: Option<Date>,
}

impl Default for ScanOptions<'_> {
    fn default() -> Self {
        ScanOptions {
            typosquat: true,
            advisories: Some(&OsvAdvisories),
            ignore: None,
            today: None,
        }
    }
}

/// Totals for a [`Report`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    /// Distinct `(name, version)` pairs in the lockfile.
    pub packages: usize,
    /// How many of those had a pinned version and were checked against the
    /// advisory source.
    pub queried: usize,
    pub typosquats: usize,
    pub vulnerabilities: usize,
    /// Findings matched by an active ignore entry. Not counted in
    /// `typosquats` / `vulnerabilities`.
    pub suppressed: usize,
    pub warnings: usize,
}

/// The outcome of scanning one lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub path: PathBuf,
    pub ecosystem: Ecosystem,
    /// Findings that still need attention.
    pub findings: Vec<Finding>,
    /// Findings an ignore entry accepted. Listed for visibility but excluded
    /// from [`Report::has_findings`] and so from the CLI exit code.
    pub suppressed: Vec<Suppressed>,
    /// Problems parsing the lockfile, and expired ignore entries. These are
    /// not findings: they mean part of the scan may not be what you expect.
    pub warnings: Vec<String>,
    pub summary: Summary,
}

impl Report {
    /// True if any unsuppressed finding was reported.
    pub fn has_findings(&self) -> bool {
        !self.findings.is_empty()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Scanned {} ({} packages, {} pinned versions checked for known vulnerabilities)",
            self.path.display(),
            self.summary.packages,
            self.summary.queried
        )?;

        if self.findings.is_empty() {
            writeln!(f, "\nNo findings.")?;
        } else {
            writeln!(f, "\nFindings ({}):", self.findings.len())?;
            for finding in &self.findings {
                writeln!(f, "  - {finding}")?;
            }
        }

        if !self.suppressed.is_empty() {
            writeln!(f, "\nSuppressed ({}):", self.suppressed.len())?;
            for suppressed in &self.suppressed {
                write!(f, "  - {} — {}", suppressed.finding, suppressed.reason)?;
                match suppressed.expires {
                    Some(expires) => writeln!(f, " (until {expires})")?,
                    None => writeln!(f)?,
                }
            }
        }

        if !self.warnings.is_empty() {
            writeln!(f, "\nWarnings ({}):", self.warnings.len())?;
            for warning in &self.warnings {
                writeln!(f, "  - {warning}")?;
            }
        }

        write!(
            f,
            "\nSummary: {} typosquat(s), {} vulnerability finding(s), {} suppressed, \
             {} warning(s)",
            self.summary.typosquats,
            self.summary.vulnerabilities,
            self.summary.suppressed,
            self.summary.warnings
        )
    }
}

/// Errors that stop a scan from producing a report.
#[derive(Debug)]
pub enum ScanError {
    Lockfile(LockfileError),
    Advisories(AdvisoryError),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::Lockfile(err) => write!(f, "{err}"),
            ScanError::Advisories(err) => write!(f, "vulnerability lookup failed: {err}"),
        }
    }
}

impl Error for ScanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ScanError::Lockfile(err) => Some(err),
            ScanError::Advisories(err) => Some(err.as_ref()),
        }
    }
}

/// Scans the lockfile at `path` for typosquats and known vulnerabilities.
///
/// Findings matched by an unexpired entry in `opts.ignore` move to
/// [`Report::suppressed`]; expired entries produce a warning instead.
/// Repeated `(name, version)` pairs (the same package installed in several
/// places) are checked once, and all pinned versions go to the advisory
/// source in a single batch.
pub fn scan_lockfile(
    path: impl AsRef<Path>,
    opts: &ScanOptions<'_>,
) -> Result<Report, ScanError> {
    let path = path.as_ref();
    let parsed = parse_lockfile(path).map_err(ScanError::Lockfile)?;
    let packages = dedupe(parsed.dependencies);

    let mut findings = Vec::new();

    if opts.typosquat {
        let mut scores: HashMap<&str, ScanResult> = HashMap::new();
        for dependency in &packages {
            let result = scores
                .entry(dependency.name.as_str())
                .or_insert_with(|| score_package(&dependency.name));
            if result.is_likely_typosquat() {
                findings.push(finding(
                    dependency,
                    FindingKind::Typosquat {
                        distance_match: result.distance_match.clone(),
                        homoglyph_match: result.homoglyph_match.clone(),
                    },
                ));
            }
        }
    }

    let mut queried = 0;
    if let Some(source) = opts.advisories {
        let pinned: Vec<(&Dependency, &str)> = packages
            .iter()
            .filter_map(|dependency| Some((dependency, dependency.version.as_deref()?)))
            .collect();
        let queries: Vec<OsvBatchQuery<'_>> = pinned
            .iter()
            .map(|&(dependency, version)| OsvBatchQuery {
                name: &dependency.name,
                ecosystem: dependency.ecosystem,
                version,
            })
            .collect();

        if !queries.is_empty() {
            let results = source.query(&queries).map_err(ScanError::Advisories)?;
            queried = queries.len();
            for ((dependency, _), advisory_ids) in pinned.iter().zip(results) {
                for advisory_id in advisory_ids {
                    findings.push(finding(
                        dependency,
                        FindingKind::Vulnerability { advisory_id },
                    ));
                }
            }
        }
    }

    let mut warnings = parsed.warnings;
    let mut suppressed = Vec::new();
    if let Some(ignore) = opts.ignore {
        let applied = ignore.apply(findings, opts.today.unwrap_or_else(Date::today));
        findings = applied.kept;
        suppressed = applied.suppressed;
        warnings.extend(applied.warnings);
    }

    let count = |is_kind: fn(&FindingKind) -> bool| {
        findings.iter().filter(|f| is_kind(&f.kind)).count()
    };
    let summary = Summary {
        packages: packages.len(),
        queried,
        typosquats: count(|kind| matches!(kind, FindingKind::Typosquat { .. })),
        vulnerabilities: count(|kind| matches!(kind, FindingKind::Vulnerability { .. })),
        suppressed: suppressed.len(),
        warnings: warnings.len(),
    };

    Ok(Report {
        path: path.to_path_buf(),
        ecosystem: parsed.ecosystem,
        findings,
        suppressed,
        warnings,
        summary,
    })
}

/// Directories [`scan_dir`] never descends into: installed dependencies,
/// VCS metadata and build output, none of which hold the project's own
/// lockfiles.
pub const SKIPPED_DIRS: &[&str] = &["node_modules", ".git", "target", "dist"];

/// A lockfile [`scan_dir`] found but couldn't scan.
#[derive(Debug)]
pub struct DirScanError {
    /// Relative to the scanned root.
    pub path: PathBuf,
    pub error: ScanError,
}

/// The outcome of scanning a directory tree: one [`Report`] per lockfile.
#[derive(Debug)]
pub struct DirReport {
    pub root: PathBuf,
    /// One report per scanned lockfile, sorted by path. Each report's `path`
    /// is relative to `root`.
    pub lockfiles: Vec<Report>,
    /// Lockfiles that were found but couldn't be parsed or checked. One bad
    /// lockfile doesn't stop the rest of the tree from being scanned.
    pub errors: Vec<DirScanError>,
}

impl DirReport {
    /// True if any lockfile has an unsuppressed finding.
    pub fn has_findings(&self) -> bool {
        self.lockfiles.iter().any(Report::has_findings)
    }
}

impl fmt::Display for DirReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Scanned {} ({} lockfile(s))",
            self.root.display(),
            self.lockfiles.len()
        )?;
        for report in &self.lockfiles {
            write!(f, "\n\n== {} ==\n{report}", report.path.display())?;
        }
        if !self.errors.is_empty() {
            write!(f, "\n\nErrors ({}):", self.errors.len())?;
            for err in &self.errors {
                write!(f, "\n  - {}: {}", err.path.display(), err.error)?;
            }
        }
        Ok(())
    }
}

/// Walks the tree under `root` and runs [`scan_lockfile`] on every lockfile
/// in it, grouping the results by lockfile path.
///
/// Directories named in [`SKIPPED_DIRS`] and symlinks are not followed. A
/// file counts as a lockfile when [`scan_lockfile`] recognises its name, so
/// this supports exactly the formats a single-file scan does. Fails only if
/// `root` itself can't be read.
pub fn scan_dir(root: impl AsRef<Path>, opts: &ScanOptions<'_>) -> io::Result<DirReport> {
    let root = root.as_ref();
    let mut lockfiles = Vec::new();
    let mut errors = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if dir == root => return Err(err),
            // A subdirectory we can't list is skipped, not fatal.
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() {
                if !SKIPPED_DIRS.iter().any(|skip| entry.file_name() == *skip) {
                    pending.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            match scan_lockfile(&path, opts) {
                Ok(mut report) => {
                    report.path = relative;
                    lockfiles.push(report);
                }
                Err(ScanError::Lockfile(LockfileError::UnknownFormat(_))) => {}
                Err(error) => errors.push(DirScanError {
                    path: relative,
                    error,
                }),
            }
        }
    }

    lockfiles.sort_by(|a, b| a.path.cmp(&b.path));
    errors.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(DirReport {
        root: root.to_path_buf(),
        lockfiles,
        errors,
    })
}

/// Collapses repeated `(name, version)` pairs, keeping first-seen order. A
/// pair counts as direct if any of its installs is direct, and keeps the
/// shortest known path among them.
fn dedupe(dependencies: Vec<Dependency>) -> Vec<Dependency> {
    let mut index: HashMap<(String, Option<String>), usize> = HashMap::new();
    let mut unique: Vec<Dependency> = Vec::new();
    for dependency in dependencies {
        let key = (dependency.name.clone(), dependency.version.clone());
        match index.get(&key) {
            Some(&existing) => {
                let kept = &mut unique[existing];
                kept.direct |= dependency.direct;
                let shorter = !dependency.path.is_empty()
                    && (kept.path.is_empty() || dependency.path.len() < kept.path.len());
                if shorter {
                    kept.path = dependency.path;
                }
            }
            None => {
                index.insert(key, unique.len());
                unique.push(dependency);
            }
        }
    }
    unique
}

fn finding(dependency: &Dependency, kind: FindingKind) -> Finding {
    Finding {
        package: dependency.name.clone(),
        version: dependency.version.clone(),
        ecosystem: dependency.ecosystem,
        direct: dependency.direct,
        path: dependency.path.clone(),
        kind,
    }
}
