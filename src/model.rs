//! Shared types for scanning a project's dependencies: what a lockfile
//! parser extracts ([`Dependency`]) and what the scanner reports about it
//! ([`Finding`]).

use std::fmt;

pub use crate::cve::osv_client::Ecosystem;

/// One installed package as recorded in a lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// Package name as the registry knows it (e.g. `lodash`, `@babel/core`,
    /// `requests`).
    pub name: String,
    /// The exact pinned version, if the lockfile records one. Unpinned
    /// entries (e.g. `flask>=2.0` in a `requirements.txt`) are still checked
    /// for typosquatting but can't be matched against OSV.
    pub version: Option<String>,
    pub ecosystem: Ecosystem,
    /// True if the project depends on this package itself (it's listed in
    /// the project manifest), false if it was only pulled in by another
    /// dependency.
    pub direct: bool,
    /// True for development-only dependencies.
    pub dev: bool,
    /// The shortest chain of packages that pulls this one in, starting from
    /// the project and ending with this package, e.g.
    /// `["my-app", "webpack", "old-lib"]`. Formats with no project root
    /// (`requirements.txt`) start at the dependency itself. Empty if the
    /// lockfile doesn't connect the package to the project.
    pub path: Vec<String>,
}

/// What the scanner found wrong with a dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// The name looks like an imitation of a popular package. The two fields
    /// mirror [`ScanResult`](crate::typosquat::scanner::ScanResult): at least
    /// one is set.
    Typosquat {
        distance_match: Option<(String, usize)>,
        homoglyph_match: Option<String>,
    },
    /// OSV reports a known vulnerability affecting this exact version.
    Vulnerability { advisory_id: String },
    /// Package contains an install-time script that executes during installation.
    /// These scripts are a common malware delivery path.
    InstallScript {
        script_name: String,
        script_command: String,
    },
}

/// A single problem with a single dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub package: String,
    pub version: Option<String>,
    pub ecosystem: Ecosystem,
    /// Whether the affected package is a direct or transitive dependency;
    /// see [`Dependency::direct`].
    pub direct: bool,
    /// How the affected package is reached; see [`Dependency::path`].
    pub path: Vec<String>,
    pub kind: FindingKind,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.version {
            Some(version) => write!(f, "{}@{}", self.package, version)?,
            None => write!(f, "{}", self.package)?,
        }
        if self.direct {
            write!(f, " [direct]")?;
        } else if self.path.is_empty() {
            write!(f, " [transitive]")?;
        } else {
            write!(f, " [transitive via {}]", self.path.join(" > "))?;
        }

        match &self.kind {
            FindingKind::Typosquat {
                distance_match,
                homoglyph_match,
            } => {
                write!(f, ": likely typosquat (")?;
                let mut reasons = Vec::new();
                if let Some((name, distance)) = distance_match {
                    reasons.push(format!("edit distance {distance} from \"{name}\""));
                }
                if let Some(name) = homoglyph_match {
                    reasons.push(format!("homoglyph impersonation of \"{name}\""));
                }
                write!(f, "{})", reasons.join("; "))
            }
            FindingKind::Vulnerability { advisory_id } => {
                write!(f, ": known vulnerability {advisory_id}")
            }
            FindingKind::InstallScript {
                script_name,
                script_command,
            } => {
                write!(f, ": install-time script '{script_name}': {script_command}")
            }
        }
    }
}
