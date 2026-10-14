//! Finding and issue reference models for DepRadar.
//!
//! Standardizes the `issue_ref` format used across crawler findings,
//! on-chain router bounties, and dashboard consumers:
//! `<ecosystem>:<package>[@<version>]#<kind>[:<advisory>]`

use std::fmt;

/// Structured representation of a standardized DepRadar finding reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FindingRef {
    /// Target package ecosystem (e.g. `npm`, `pypi`, `crates`, `rubygems`).
    pub ecosystem: String,
    /// Package name (supports scoped packages such as `@org/package`).
    pub package: String,
    /// Optional package version affected.
    pub version: Option<String>,
    /// Classification of the finding (e.g. `typosquat`, `cve`, `malicious`, `yanked`).
    pub kind: String,
    /// Optional advisory identifier (e.g. `CVE-2023-1234`, `GHSA-...`, or target package).
    pub advisory: Option<String>,
}

/// Errors when parsing a raw issue reference string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IssueRefParseError {
    Empty,
    MissingEcosystem,
    MissingPackage,
    MissingKind,
    InvalidFormat,
}

impl fmt::Display for IssueRefParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "issue_ref cannot be empty"),
            Self::MissingEcosystem => {
                write!(f, "missing ecosystem prefix (expected '<ecosystem>:...')")
            }
            Self::MissingPackage => write!(f, "missing package name"),
            Self::MissingKind => write!(f, "missing finding kind (expected '#<kind>')"),
            Self::InvalidFormat => write!(f, "invalid issue_ref format"),
        }
    }
}

impl std::error::Error for IssueRefParseError {}

/// Formats a `FindingRef` into the standard string format:
/// `<ecosystem>:<package>[@<version>]#<kind>[:<advisory>]`
pub fn format_issue_ref(finding: &FindingRef) -> String {
    let mut out = format!("{}:{}", finding.ecosystem.to_lowercase(), finding.package);
    if let Some(ref ver) = finding.version {
        out.push('@');
        out.push_str(ver);
    }
    out.push('#');
    out.push_str(&finding.kind.to_lowercase());
    if let Some(ref adv) = finding.advisory {
        out.push(':');
        out.push_str(adv);
    }
    out
}

/// Parses an `issue_ref` string adhering to:
/// `<ecosystem>:<package>[@<version>]#<kind>[:<advisory>]`
pub fn parse_issue_ref(input: &str) -> Result<FindingRef, IssueRefParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(IssueRefParseError::Empty);
    }

    let (eco_part, rest) = input
        .split_once(':')
        .ok_or(IssueRefParseError::MissingEcosystem)?;
    let ecosystem = eco_part.trim().to_lowercase();
    if ecosystem.is_empty() {
        return Err(IssueRefParseError::MissingEcosystem);
    }

    let (pkg_ver_part, kind_adv_part) = rest
        .split_once('#')
        .ok_or(IssueRefParseError::MissingKind)?;
    let pkg_ver = pkg_ver_part.trim();
    if pkg_ver.is_empty() {
        return Err(IssueRefParseError::MissingPackage);
    }

    // Split package and version, handling scoped packages like `@scope/pkg@1.0.0`
    let (package, version) = if let Some(idx) = pkg_ver.rfind('@') {
        if idx > 0 {
            let pkg = pkg_ver[..idx].trim().to_string();
            let ver = pkg_ver[idx + 1..].trim().to_string();
            (pkg, if ver.is_empty() { None } else { Some(ver) })
        } else {
            (pkg_ver.to_string(), None)
        }
    } else {
        (pkg_ver.to_string(), None)
    };

    if package.is_empty() {
        return Err(IssueRefParseError::MissingPackage);
    }

    let (kind_part, advisory) = if let Some((k, adv)) = kind_adv_part.split_once(':') {
        let adv_clean = adv.trim();
        (
            k.trim().to_lowercase(),
            if adv_clean.is_empty() {
                None
            } else {
                Some(adv_clean.to_string())
            },
        )
    } else {
        (kind_adv_part.trim().to_lowercase(), None)
    };

    if kind_part.is_empty() {
        return Err(IssueRefParseError::MissingKind);
    }

    Ok(FindingRef {
        ecosystem,
        package,
        version,
        kind: kind_part,
        advisory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip_full() {
        let original = FindingRef {
            ecosystem: "npm".to_string(),
            package: "expresss".to_string(),
            version: Some("1.0.0".to_string()),
            kind: "typosquat".to_string(),
            advisory: Some("express".to_string()),
        };
        let formatted = format_issue_ref(&original);
        assert_eq!(formatted, "npm:expresss@1.0.0#typosquat:express");

        let parsed = parse_issue_ref(&formatted).expect("should parse successfully");
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_round_trip_scoped_package_cve() {
        let original = FindingRef {
            ecosystem: "npm".to_string(),
            package: "@fastify/deepmerge".to_string(),
            version: Some("1.3.0".to_string()),
            kind: "cve".to_string(),
            advisory: Some("GHSA-9265-m339-ch94".to_string()),
        };
        let formatted = format_issue_ref(&original);
        assert_eq!(
            formatted,
            "npm:@fastify/deepmerge@1.3.0#cve:GHSA-9265-m339-ch94"
        );

        let parsed = parse_issue_ref(&formatted).expect("should parse successfully");
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_round_trip_no_version_no_advisory() {
        let original = FindingRef {
            ecosystem: "pypi".to_string(),
            package: "requests".to_string(),
            version: None,
            kind: "malicious".to_string(),
            advisory: None,
        };
        let formatted = format_issue_ref(&original);
        assert_eq!(formatted, "pypi:requests#malicious");

        let parsed = parse_issue_ref(&formatted).expect("should parse successfully");
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_parse_errors() {
        assert_eq!(parse_issue_ref(""), Err(IssueRefParseError::Empty));
        assert_eq!(parse_issue_ref("   "), Err(IssueRefParseError::Empty));
        assert_eq!(
            parse_issue_ref("npm-express"),
            Err(IssueRefParseError::MissingEcosystem)
        );
        assert_eq!(
            parse_issue_ref("npm:express"),
            Err(IssueRefParseError::MissingKind)
        );
        assert_eq!(
            parse_issue_ref(":express#cve"),
            Err(IssueRefParseError::MissingEcosystem)
        );
        assert_eq!(
            parse_issue_ref("npm:#cve"),
            Err(IssueRefParseError::MissingPackage)
        );
    }
}
