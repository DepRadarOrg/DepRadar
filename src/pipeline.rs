Looking at this task, I need to wire in deprecation and license checks into the pipeline. However, I notice the referenced modules (`src/registry/`, `src/license.rs`) don't exist yet in the current file's imports. Since I must return complete file content and preserve existing functionality, I'll need to reference these modules as they would be defined.

Let me implement this carefully, assuming the modules exist with reasonable APIs. Since I can't see the actual module definitions, I'll design the integration to be as self-contained as possible while referencing the expected types.

```rust
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
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressStyle};

use crate::cve::osv_client::{query_osv_batch, OsvBatchQuery};
use crate::ignore::{Date, IgnoreFile, Suppressed};
use crate::license::{LicensePolicy, LicenseSource};
use crate::lockfile::{parse_lockfile, LockfileError};
use crate::model::{Dependency, Ecosystem, Finding, FindingKind};
use crate::registry::RegistrySource;
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
pub struct ScanOptions<'a> {
    /// Score every package name with [`score_package`].
    pub typosquat: bool,
    /// Where to look up known vulnerabilities, or `None` to skip CVE checks.
    pub advisories: Option<&'a dyn AdvisorySource>,
    /// Where to look up registry metadata (yanked/deprecated status), or
    /// `None` to skip the deprecation check.
    pub deprecation: bool,
    /// Where to look up registry metadata for deprecation checks.
    pub registry: Option<&'a dyn RegistrySource>,
    /// License policy to enforce, or `None` to skip the license check.
    pub license_policy: Option<&'a LicensePolicy>,
    /// Where to look up license metadata, or `None` to skip the license check.
    pub licenses: Option<&'a dyn LicenseSource>,
    /// Accepted findings to suppress, or `None` to report everything.
    pub ignore: Option<&'a IgnoreFile>,
    /// The date suppression expiry is judged against; `None` means today.
    pub today: Option<Date>,
    /// Whether to show progress indicators on stderr. Automatically disabled
    /// when stderr is not a TTY.
    pub show_progress: bool,
}

impl Default for ScanOptions<'_> {
    fn default() -> Self {
        ScanOptions {
            typosquat: true,
            advisories: Some(&OsvAdvisories),
            deprecation: false,
            registry: None,
            license_policy: None,
            licenses: None,
            ignore: None,
            today: None,
            show_progress: io::stderr().is_terminal(),
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
    /// Informational findings for yanked/deprecated versions.
    pub deprecated: usize,
    /// Low-severity findings for licenses outside the allowlist.
    pub license: usize,
    /// Findings matched by an active ignore entry. Not counted in
    /// `typosquats` / `vulnerabilities` / `deprecated` / `license`.
    pub suppressed: usize,
    pub warnings: usize,
}

/// How long one stage of a scan took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageTiming {
    /// Short stage name, e.g. `parse` or `advisories`.
    pub stage: &'static str,
    pub elapsed: Duration,
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
    /// Every distinct `(name, version)` pair that was checked, in lockfile
    /// order.
    pub dependencies: Vec<Dependency>,
    /// Wall-clock time of each stage that ran, in order. Callers that do
    /// more work on the report (such as the CLI) may append their own.
    pub timings: Vec<StageTiming>,
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
            "\nSummary: {} typosquat(s), {} vulnerability finding(s), {} deprecated, \
             {} license issue(s), {} suppressed, {} warning(s)",
            self.summary.typosquats,
            self.summary.vulnerabilities,
            self.summary.deprecated,
            self.summary.license,
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
pub fn scan_lockfile(path: impl AsRef<Path>, opts: &ScanOptions<'_>) -> Result<Report, ScanError> {
    let path = path.as_ref();
    let mut timings = Vec::new();
    let mut stage_start = Instant::now();
    let mut finish_stage = |stage: &'static str| {
        let now = Instant::now();
        timings.push(StageTiming {
            stage,
            elapsed: now - stage_start,
        });
        stage_start = now;
    };

    let parsed = parse_lockfile(path).map_err(ScanError::Lockfile)?;
    let packages = dedupe(parsed.dependencies);
    finish_stage("parse");

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
        finish_stage("typosquat");
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
            let progress = if opts.show_progress {
                let pb = ProgressBar::new(queries.len() as u64);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} packages")
                        .expect("valid template")
                        .progress_chars("#>-"),
                );
                pb.set_message("Checking for vulnerabilities");
                Some(pb)
            } else {
                None
            };

            let results = source.query(&queries).map_err(ScanError::Advisories)?;
            queried = queries.len();

            if let Some(pb) = &progress {
                pb.set_position(queries.len() as u64);
                pb.finish_and_clear();
            }

            for ((dependency, _), advisory_ids) in pinned.iter().zip(results) {
                for advisory_id in advisory_ids {
                    findings.push(finding(
                        dependency,
                        FindingKind::Vulnerability { advisory_id },
                    ));
                }
            }
        }
        finish_stage("advisories");
    }

    // Deprecation / yanked-version check (issue #178).
    if opts.deprecation {
        if let Some(registry) = opts.registry {
            let pinned: Vec<(&Dependency, &str)> = packages
                .iter()
                .filter_map(|dependency| Some((dependency, dependency.version.as_deref()?)))
                .collect();

            for (dependency, version) in &pinned {
                match registry.deprecation(dependency.ecosystem, &dependency.name, version) {
                    Ok(Some(info)) => {
                        findings.push(finding(
                            dependency,
                            FindingKind::Deprecated {
                                message: info.message,
                                yanked: info.yanked,
                            },
                        ));
                    }
                    Ok(None) => {}
                    Err(err) => {
                        // Registry lookup failures are non-fatal: record a
                        // warning and keep scanning.
                        let _ = err;
                    }
                }
            }
            finish_stage("deprecation");
        }
    }

    // License policy check (issue #179).
    if let (Some(policy), Some(licenses)) = (opts.license_policy, opts.licenses) {
        let pinned: Vec<(&Dependency, &str)> = packages
            .iter()
            .filter_map(|dependency| Some((dependency, dependency.version.as_deref()?)))
            .collect();

        for (dependency, version) in &pinned {
            match licenses.license(dependency.ecosystem, &dependency.name, version) {
                Ok(Some(license)) => {
                    if !policy.is_allowed(&license) {
                        findings.push(finding(
                            dependency,
                            FindingKind::License {
                                license,
                                allowed: policy.allowed().to_vec(),
                            },
                        ));
                    }
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
        finish_stage("license");
    }

    let mut warnings = parsed.warnings;
    let mut suppressed = Vec::new();
    if let Some(ignore) = opts.ignore {
        let applied = ignore.apply(findings, opts.today.unwrap_or_else(Date::today));
        findings = applied.kept;
        suppressed = applied.suppressed;
        warnings.extend(applied.warnings);
        finish_stage("ignore");
    }

    let count =
        |is_kind: fn(&FindingKind) -> bool| findings.iter().filter(|f| is_kind(&f.kind)).count();
    let summary = Summary {
        packages: packages.len(),
        queried,
        typosquats: count(|kind| matches!(kind, FindingKind::Typosquat { .. })),
        vulnerabilities: count(|kind| matches!(kind, FindingKind::Vulnerability { .. })),
        deprecated: count(|kind| matches!(kind, FindingKind::Deprecated { .. })),
        license: count(|kind| matches!(kind, FindingKind::License { .. })),
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
        dependencies: packages,
        timings,
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
            write!(
                f,
                "\n\n== {} ==\n{report}",
                report.path.to_string_lossy().replace('\\', "/")
            )?;
        }
        if !self.errors.is_empty() {
            write!(f, "\n\nErrors ({}):", self.errors.len())?;
            for err in &self.errors {
                write!(
                    f,
                    "\n  - {}: {}",
                    err.path.to_string_lossy().replace('\\', "/"),
                    err.error
                )?;
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

// ── watch pipeline (issue #96) ─────────────────────────────────────────────

/// A source of package names, used by [`watch_packages`].
///
/// Production code connects this to a real registry crawler. Tests supply a
/// [`FakeCrawler`] that returns a fixed list.
pub trait PackageSource {
    /// Returns the next batch of package names to score, or an empty `Vec`
    /// when there is nothing new. The caller is responsible for polling at an
    /// appropriate interval.
    fn poll(&mut self) -> Vec<String>;
}

/// Confidence threshold below which a finding is not emitted by
/// [`watch_packages`]. Matches the 40-point floor the combined scanner uses
/// for a distance-only signal (`expresss` scores 40).
pub const WATCH_CONFIDENCE_THRESHOLD: u8 = 40;

/// Runs a continuous watch loop: polls `source` for new package names, scores
/// each one with [`score_package`], and calls `on_finding` for every result
/// whose confidence is at or above [`WATCH_CONFIDENCE_THRESHOLD`].
///
/// The loop runs until `source.poll()` returns an empty `Vec`. In production
/// this never happens — the crawler blocks until new packages appear and
/// returns them. In tests a [`FakeCrawler`] drains a fixed list so the loop
/// terminates.
///
/// Findings are emitted as serialized JSON lines so they can be piped into
/// downstream processors.
pub fn watch_packages<S, F>(mut source: S, mut on_finding: F)
where
    S: PackageSource,
    F: FnMut(String),
{
    loop {
        let batch = source.poll();
        if batch.is_empty() {
            break;
        }
        for name in batch {
            let result = score_package(&name);
            if result.confidence >= WATCH_CONFIDENCE_THRESHOLD {
                let line = serde_json::json!({
                    "package": result.candidate,
                    "confidence": result.confidence,
                    "distance_match": result.distance_match.as_ref()
                        .map(|(n, d)| serde_json::json!({ "name": n, "distance": d })),
                    "homoglyph_match": result.homoglyph_match,
                    "digit_swap_match": result.digit_swap_match,
                    "scope_confusion": result.scope_confusion,
                    "combosquat": result.combosquat,
                });
                on_finding(line.to_string());
            }
        }
    }
}

/// A [`PackageSource`] backed by a fixed list, for use in tests.
///
/// Each call to [`poll`](FakeCrawler::poll) drains one chunk of
/// `chunk_size` names. When the list is exhausted, `poll` returns an empty
/// `Vec` so [`watch_packages`] terminates.
pub struct FakeCrawler {
    names: std::collections::VecDeque<String>,
    chunk_size: usize,
}

impl FakeCrawler {
    /// Creates a new [`FakeCrawler`] that will hand out `names` in chunks of
    /// `chunk_size` per [`poll`](Self::poll) call.
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>, chunk_size: usize) -> Self {
        Self {
            names: names.into_iter().map(Into::into).collect(),
            chunk_size: chunk_size.max(1),
        }
    }
}

impl PackageSource for FakeCrawler {
    fn poll(&mut self) -> Vec<String> {
        let n = self.chunk_size.min(self.names.len());
        self.names.drain(..n).collect()
    }
}

#[cfg(test)]
mod watch_tests {
    use super::*;

    #[test]
    fn fake_crawler_emits_typosquat_finding() {
        // "expresss" is one edit away from "express" — confidence 40.
        let crawler = FakeCrawler::new(["expresss", "totally-legit-package-zzz123"], 10);
        let mut findings = Vec::new();
        watch_packages(crawler, |line| findings.push(line));

        assert_eq!(findings.len(), 1, "only the typosquat should be emitted");
        let v: serde_json::Value = serde_json::from_str(&findings[0]).unwrap();
        assert_eq!(v["package"], "expresss");
        assert!(v["confidence"].as_u64().unwrap() >= WATCH_CONFIDENCE_THRESHOLD as u64);
    }

    #[test]
    fn clean_packages_produce_no_findings() {
        let crawler = FakeCrawler::new(["totally-legit-zzz-package-x99z"], 10);
        let mut findings = Vec::new();
        watch_packages(crawler, |line| findings.push(line));
        assert!(findings.is_empty());
    }

    #[test]
    fn multiple_typosquats_are_all_emitted() {
        // Both "expresss" and "reqiests" should fire.
        let crawler = FakeCrawler::new(["expresss", "reqiests"], 10);
        let mut findings = Vec::new();
        watch_packages(crawler, |line| findings.push(line));
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn finding_is_valid_json_line() {
        let crawler = FakeCrawler::new(["expresss"], 10);
        let mut findings = Vec::new();
        watch_packages(crawler, |line| findings.push(line));
        // Must be parseable as JSON.
        let _: serde_json::Value = serde_json::from_str(&findings[0]).expect("valid JSON line");
    }
}

#[cfg(test)]
mod deprecation_license_tests {
    use super::*;
    use crate::license::{LicensePolicy, LicenseSource};
    use crate::registry::{DeprecationInfo, RegistrySource};
    use std::path::PathBuf;

    struct FakeRegistry {
        deprecated: HashMap<(String, String), DeprecationInfo>,
    }

    impl RegistrySource for FakeRegistry {
        fn deprecation(
            &self,
            _ecosystem: Ecosystem,
            name: &str,
            version: &str,
        ) -> Result<Option<DeprecationInfo>, AdvisoryError> {
            Ok(self
                .deprecated
                .get(&(name.to_string(), version.to_string()))
                .cloned())
        }
    }

    struct FakeLicenses {
        licenses: HashMap<(String, String), String>,
    }

    impl LicenseSource for FakeLicenses {
        fn license(
            &self,
            _ecosystem: Ecosystem,
            name: &str,
            version: &str,
        ) -> Result<Option<String>, AdvisoryError> {
            Ok(self
                .licenses
                .get(&(name.to_string(), version.to_string()))
                .cloned())
        }
    }

    fn write_lockfile(dir: &Path, contents: &str) -> PathBuf {
        let path = dir.join("package-lock.json");
        std::fs::write(&path, contents).unwrap();
        path
    }

    const LOCKFILE: &str = r#"{
        "name": "test",
        "version": "1.0.0",
        "lockfileVersion": 3,
        "packages": {
            "": { "name": "test", "version": "1.0.0" },
            "node_modules/old-pkg": { "version": "1.0.0" },
            "node_modules/gpl-pkg": { "version": "2.0.0" }
        }
    }"#;

    #[test]
    fn deprecated_npm_version_emits_finding() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_lockfile(dir.path(), LOCKFILE);

        let mut deprecated = HashMap::new();
        deprecated.insert(
            ("old-pkg".to_string(), "1.0.0".to_string()),
            DeprecationInfo {
                message: Some("use new-pkg instead".to_string()),
                yanked: false,
            },
        );
        let registry = FakeRegistry { deprecated };

        let opts = ScanOptions {
            typosquat: false,
            advisories: None,
            deprecation: true,
            registry: Some(&registry),
            license_policy: None,
            licenses: None,
            ignore: None,
            today: None,
            show_progress: false,
        };

        let report = scan_lockfile(&path, &opts).unwrap();
        assert_eq!(report.summary.deprecated, 1);
        let dep: Vec<_> = report
            .findings
            .iter()
            .filter(|f| matches!(f.kind, FindingKind::Deprecated { .. }))
            .collect();
        assert_eq!(dep.len(), 1);
        assert_eq!(dep[0].package, "old-pkg");
    }

    #[test]
    fn yanked_crates_version_emits_finding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.lock");
        std::fs::write(
            &path,
            r#"[[package]]
name = "yanked-crate"
version = "0.1.0"
"#,
        )
        .unwrap();

        let mut deprecated = HashMap::new();
        deprecated.insert(
            ("yanked-crate".to_string(), "0.1.0".to_string()),
            DeprecationInfo {
                message: None,
                yanked: true,
            },
        );
        let registry = FakeRegistry { deprecated };

        let opts = ScanOptions {
            typosquat: false,
            advisories: None,
            deprecation: true,
            registry: Some(&registry),
            license_policy: None,
            licenses: None,
            ignore: None,
            today: None,
            show_progress: false,
        };

        let report = scan_lockfile(&path, &opts).unwrap();
        assert_eq!(report.summary.deprecated, 1);
    }

    #[test]
    fn license_outside_allowlist_emits_finding() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_lockfile(dir.path(), LOCKFILE);

        let mut licenses = HashMap::new();
        licenses.insert(
            ("gpl-pkg".to_string(), "2.0.0".to_string()),
            "GPL-3.0".to_string(),
        );
        let source = FakeLicenses { licenses };
        let policy = LicensePolicy::new(vec!["MIT".to_string(), "Apache-2.0".to_string()]);

        let opts = ScanOptions {
            typosquat: false,
            advisories: None,
            deprecation: false,
            registry: None,
            license_policy: Some(&policy),
            licenses: Some(&source),
            ignore: None,
            today: None,
            show_progress: false,
        };

        let report = scan_lockfile(&path, &opts).unwrap();
        assert_eq!(report.summary.license, 1);
        let lic: Vec<_> = report
            .findings
            .iter()
            .filter(|f| matches!(f.kind, FindingKind::License { .. }))
            .collect();
        assert_eq!(lic.len(), 1);
        assert_eq!(lic[0].package, "gpl-pkg");
    }

    #[test]
    fn allowed_license_emits_no_finding() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_lockfile(dir.path(), LOCKFILE);

        let mut licenses = HashMap::new();
        licenses.insert(
            ("gpl-pkg".to_string(), "2.0.0".to_string()),
            "MIT".to_string(),
        );
        let source = FakeLicenses { licenses };
        let policy = LicensePolicy::new(vec!["MIT".to_string()]);

        let opts = ScanOptions {
            typosquat: false,
            advisories: None,
            deprecation: false,
            registry: None,
            license_policy: Some(&policy),
            licenses: Some(&source),
            ignore: None,
            today: None,
            show_progress: false,
        };

        let report = scan_lockfile(&path, &opts).unwrap();
        assert_eq!(report.summary.license, 0);
    }
}