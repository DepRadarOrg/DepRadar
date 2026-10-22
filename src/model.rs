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
    /// The pinned version was yanked from crates.io or deprecated on npm.
    /// `message` carries the registry's deprecation note when one exists.
    Deprecated { message: Option<String> },
    /// The package's license is not in the configured allowlist.
    License {
        license: String,
        allowed: Vec<String>,
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
            FindingKind::Deprecated { message } => match message {
                Some(message) => write!(f, ": deprecated ({message})"),
                None => write!(f, ": yanked"),
            },
            FindingKind::License { license, allowed } => {
                write!(f, ": license \"{license}\" not in allowlist")?;
                if !allowed.is_empty() {
                    write!(f, " ({})", allowed.join(", "))?;
                }
                Ok(())
            }
        }
    }
}

/// What kind of problem an [`IssueRef`] points at. Mirrors [`FindingKind`]
/// without its payload; the payload that matters for linking travels in
/// [`IssueRef::detail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    Typosquat,
    Vulnerability,
    Deprecated,
    License,
}

impl IssueKind {
    fn as_str(self) -> &'static str {
        match self {
            IssueKind::Typosquat => "typosquat",
            IssueKind::Vulnerability => "vulnerability",
            IssueKind::Deprecated => "deprecated",
            IssueKind::License => "license",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "typosquat" => IssueKind::Typosquat,
            "vulnerability" => IssueKind::Vulnerability,
            "deprecated" => IssueKind::Deprecated,
            "license" => IssueKind::License,
            _ => return None,
        })
    }
}

impl Ecosystem {
    /// The lowercase token used for this ecosystem in an [`IssueRef`].
    pub fn as_ref_str(self) -> &'static str {
        match self {
            Ecosystem::Npm => "npm",
            Ecosystem::PyPI => "pypi",
            Ecosystem::CratesIo => "crates.io",
            Ecosystem::Go => "go",
            Ecosystem::Maven => "maven",
            Ecosystem::RubyGems => "rubygems",
            Ecosystem::Packagist => "packagist",
        }
    }

    /// Inverse of [`Ecosystem::as_ref_str`], case-insensitive.
    pub fn from_ref_str(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "npm" => Ecosystem::Npm,
            "pypi" => Ecosystem::PyPI,
            "crates.io" => Ecosystem::CratesIo,
            "go" => Ecosystem::Go,
            "maven" => Ecosystem::Maven,
            "rubygems" => Ecosystem::RubyGems,
            "packagist" => Ecosystem::Packagist,
            _ => return None,
        })
    }
}

/// The structured form of a bounty's `issue_ref`, shared by the scanner and
/// anything that creates or reads bounties on-chain. See
/// `docs/issue-ref.md` for the format:
///
/// ```text
/// <ecosystem>:<package>[@<version>]#<kind>[:<detail>]
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueRef {
    pub ecosystem: Ecosystem,
    pub package: String,
    pub version: Option<String>,
    pub kind: IssueKind,
    /// The advisory ID for a vulnerability, the impersonated package for a
    /// typosquat, or the offending license for a license finding.
    pub detail: Option<String>,
}

impl From<&Finding> for IssueRef {
    fn from(finding: &Finding) -> Self {
        let (kind, detail) = match &finding.kind {
            FindingKind::Typosquat {
                distance_match,
                homoglyph_match,
            } => (
                IssueKind::Typosquat,
                distance_match
                    .as_ref()
                    .map(|(name, _)| name.clone())
                    .or_else(|| homoglyph_match.clone()),
            ),
            FindingKind::Vulnerability { advisory_id } => {
                (IssueKind::Vulnerability, Some(advisory_id.clone()))
            }
            FindingKind::Deprecated { .. } => (IssueKind::Deprecated, None),
            FindingKind::License { license, .. } => (IssueKind::License, Some(license.clone())),
        };
        IssueRef {
            ecosystem: finding.ecosystem,
            package: finding.package.clone(),
            version: finding.version.clone(),
            kind,
            detail,
        }
    }
}

impl fmt::Display for IssueRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ecosystem.as_ref_str(), self.package)?;
        if let Some(version) = &self.version {
            write!(f, "@{version}")?;
        }
        write!(f, "#{}", self.kind.as_str())?;
        if let Some(detail) = &self.detail {
            write!(f, ":{detail}")?;
        }
        Ok(())
    }
}

impl Finding {
    /// This finding's standard `issue_ref`, ready to pass to the router's
    /// `create_bounty`.
    pub fn issue_ref(&self) -> String {
        format_issue_ref(&IssueRef::from(self))
    }
}

/// Renders `issue_ref` in the standard format.
pub fn format_issue_ref(issue_ref: &IssueRef) -> String {
    issue_ref.to_string()
}

/// Parses a standard `issue_ref`. Returns `None` for anything else, including
/// the free-form refs older bounties use.
pub fn parse_issue_ref(s: &str) -> Option<IssueRef> {
    let (ecosystem, rest) = s.split_once(':')?;
    let ecosystem = Ecosystem::from_ref_str(ecosystem)?;
    let (target, kind) = rest.split_once('#')?;
    let (kind, detail) = match kind.split_once(':') {
        Some((kind, detail)) if !detail.is_empty() => (kind, Some(detail.to_string())),
        Some(_) => return None,
        None => (kind, None),
    };
    let kind = IssueKind::parse(kind)?;
    // An `@` at index 0 starts an npm scope, not a version.
    let (package, version) = match target.rfind('@').filter(|&i| i > 0) {
        Some(i) if i + 1 < target.len() => (&target[..i], Some(target[i + 1..].to_string())),
        Some(_) => return None,
        None => (target, None),
    };
    if package.is_empty() || package.chars().any(char::is_whitespace) {
        return None;
    }
    Some(IssueRef {
        ecosystem,
        package: package.to_string(),
        version,
        kind,
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue_ref(
        ecosystem: Ecosystem,
        package: &str,
        version: Option<&str>,
        kind: IssueKind,
        detail: Option<&str>,
    ) -> IssueRef {
        IssueRef {
            ecosystem,
            package: package.to_string(),
            version: version.map(str::to_string),
            kind,
            detail: detail.map(str::to_string),
        }
    }

    #[test]
    fn formats_the_documented_examples() {
        let r = issue_ref(
            Ecosystem::Npm,
            "expresss",
            Some("1.0.0"),
            IssueKind::Typosquat,
            Some("express"),
        );
        assert_eq!(format_issue_ref(&r), "npm:expresss@1.0.0#typosquat:express");

        let r = issue_ref(
            Ecosystem::PyPI,
            "requests",
            Some("2.19.0"),
            IssueKind::Vulnerability,
            Some("GHSA-x84v-xcm2-53pg"),
        );
        assert_eq!(
            format_issue_ref(&r),
            "pypi:requests@2.19.0#vulnerability:GHSA-x84v-xcm2-53pg"
        );
    }

    #[test]
    fn round_trips() {
        let cases = [
            issue_ref(
                Ecosystem::Npm,
                "@babel/core",
                Some("7.0.0"),
                IssueKind::Deprecated,
                None,
            ),
            issue_ref(
                Ecosystem::Npm,
                "@babel/core",
                None,
                IssueKind::Typosquat,
                None,
            ),
            issue_ref(
                Ecosystem::CratesIo,
                "serde",
                Some("1.0.0"),
                IssueKind::License,
                Some("MIT OR Apache-2.0"),
            ),
            issue_ref(
                Ecosystem::Maven,
                "org.apache.logging.log4j:log4j-core",
                Some("2.14.1"),
                IssueKind::Vulnerability,
                Some("GHSA-jfh8-c2jp-5v3q"),
            ),
            issue_ref(
                Ecosystem::Go,
                "github.com/foo/bar",
                Some("v1.2.3"),
                IssueKind::Vulnerability,
                Some("GO-2023-0001"),
            ),
        ];
        for case in cases {
            let s = format_issue_ref(&case);
            assert_eq!(parse_issue_ref(&s), Some(case), "{s}");
        }
    }

    #[test]
    fn ecosystem_is_case_insensitive() {
        let r = parse_issue_ref("PyPI:flask#deprecated").unwrap();
        assert_eq!(r.ecosystem, Ecosystem::PyPI);
        assert_eq!(r.version, None);
    }

    #[test]
    fn rejects_free_form_and_malformed_refs() {
        for s in [
            "npm:expresss typosquat of express (demo)",
            "org/repo#123",
            "cargo:serde#typosquat",
            "npm:#typosquat",
            "npm:lodash@#vulnerability",
            "npm:lodash@4.17.0#vulnerability:",
            "npm:lodash@4.17.0#unknown",
        ] {
            assert_eq!(parse_issue_ref(s), None, "{s}");
        }
    }

    #[test]
    fn builds_from_a_finding() {
        let finding = Finding {
            package: "expresss".to_string(),
            version: Some("1.0.0".to_string()),
            ecosystem: Ecosystem::Npm,
            direct: true,
            path: Vec::new(),
            kind: FindingKind::Typosquat {
                distance_match: Some(("express".to_string(), 1)),
                homoglyph_match: None,
            },
        };
        assert_eq!(finding.issue_ref(), "npm:expresss@1.0.0#typosquat:express");
    }
}
