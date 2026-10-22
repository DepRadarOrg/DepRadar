//! License policy checks for dependencies.
//!
//! This module provides a configurable allowlist of acceptable licenses and
//! helpers to fetch license metadata from npm, PyPI and crates.io registries.
//! When a dependency's license is not in the allowlist, a low-severity
//! [`FindingKind::License`] finding is produced.

use std::collections::HashSet;
use std::fmt;

use serde_json::Value;

use crate::finding::{Finding, FindingKind, Severity};

/// Default set of common permissive licenses that are considered acceptable.
pub const DEFAULT_ALLOWED_LICENSES: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "MPL-2.0",
];

/// A policy describing which licenses are acceptable.
#[derive(Debug, Clone)]
pub struct LicensePolicy {
    allowed: HashSet<String>,
}

impl Default for LicensePolicy {
    fn default() -> Self {
        Self::with_default_allowlist()
    }
}

impl LicensePolicy {
    /// Create a policy with the default permissive allowlist.
    pub fn with_default_allowlist() -> Self {
        let allowed = DEFAULT_ALLOWED_LICENSES
            .iter()
            .map(|s| normalize(s))
            .collect();
        Self { allowed }
    }

    /// Create a policy from an explicit list of allowed licenses.
    pub fn new<I, S>(licenses: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let allowed = licenses
            .into_iter()
            .map(|s| normalize(s.as_ref()))
            .collect();
        Self { allowed }
    }

    /// Add a license to the allowlist.
    pub fn allow(&mut self, license: &str) {
        self.allowed.insert(normalize(license));
    }

    /// Returns `true` if the given license is allowed by this policy.
    pub fn is_allowed(&self, license: &str) -> bool {
        self.allowed.contains(&normalize(license))
    }

    /// Check a fetched license against the policy.
    ///
    /// Returns `Some(Finding)` with a low-severity [`FindingKind::License`]
    /// when the license is not allowed, or `None` when it is allowed (or
    /// unknown/empty).
    pub fn check(&self, package: &str, license: Option<&str>) -> Option<Finding> {
        let license = license?;
        let license = license.trim();
        if license.is_empty() {
            return None;
        }
        if self.is_allowed(license) {
            return None;
        }
        Some(Finding {
            kind: FindingKind::License,
            severity: Severity::Low,
            message: format!(
                "Dependency `{package}` uses license `{license}` which is not in the allowlist"
            ),
        })
    }
}

/// Normalize a license string for comparison (trim + lowercase).
fn normalize(license: &str) -> String {
    license.trim().to_ascii_lowercase()
}

/// Error type for license fetching.
#[derive(Debug)]
pub enum LicenseError {
    /// The registry response could not be parsed.
    Parse(String),
    /// The registry returned an unexpected response.
    Registry(String),
}

impl fmt::Display for LicenseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LicenseError::Parse(msg) => write!(f, "failed to parse license metadata: {msg}"),
            LicenseError::Registry(msg) => write!(f, "registry error: {msg}"),
        }
    }
}

impl std::error::Error for LicenseError {}

/// A source capable of fetching a dependency's license metadata.
pub trait LicenseSource {
    /// Fetch the license string for the given package name.
    ///
    /// Returns `Ok(None)` when the registry does not report a license.
    fn fetch_license(&self, package: &str) -> Result<Option<String>, LicenseError>;
}

/// npm registry license source.
///
/// Parses the `license` field from an npm package document.
pub struct NpmLicenseSource {
    response: Value,
}

impl NpmLicenseSource {
    /// Create a source from a raw npm registry JSON response.
    pub fn from_response(response: Value) -> Self {
        Self { response }
    }
}

impl LicenseSource for NpmLicenseSource {
    fn fetch_license(&self, _package: &str) -> Result<Option<String>, LicenseError> {
        match self.response.get("license") {
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(Value::Object(_)) => {
                // Legacy `{ "type": "MIT" }` form.
                self.response
                    .get("license")
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|s| Ok(Some(s.to_string())))
                    .unwrap_or(Ok(None))
            }
            Some(Value::Null) | None => Ok(None),
            Some(other) => Err(LicenseError::Parse(format!(
                "unexpected npm license value: {other}"
            ))),
        }
    }
}

/// PyPI license source.
///
/// Prefers `info.license` and falls back to license classifiers such as
/// `License :: OSI Approved :: MIT License`.
pub struct PypiLicenseSource {
    response: Value,
}

impl PypiLicenseSource {
    /// Create a source from a raw PyPI JSON API response.
    pub fn from_response(response: Value) -> Self {
        Self { response }
    }
}

impl LicenseSource for PypiLicenseSource {
    fn fetch_license(&self, _package: &str) -> Result<Option<String>, LicenseError> {
        let info = match self.response.get("info") {
            Some(info) => info,
            None => return Ok(None),
        };

        if let Some(Value::String(license)) = info.get("license") {
            let trimmed = license.trim();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed.to_string()));
            }
        }

        if let Some(Value::Array(classifiers)) = info.get("classifiers") {
            for classifier in classifiers {
                if let Some(s) = classifier.as_str() {
                    if let Some(license) = parse_pypi_classifier(s) {
                        return Ok(Some(license));
                    }
                }
            }
        }

        Ok(None)
    }
}

/// Extract a license name from a PyPI classifier string.
fn parse_pypi_classifier(classifier: &str) -> Option<String> {
    let rest = classifier.strip_prefix("License :: ")?;
    // Drop the "OSI Approved :: " prefix if present.
    let rest = rest.strip_prefix("OSI Approved :: ").unwrap_or(rest);
    // Strip a trailing " License" suffix.
    let name = rest.strip_suffix(" License").unwrap_or(rest);
    Some(name.trim().to_string())
}

/// crates.io license source.
///
/// Parses the `license` field from a crates.io crate document.
pub struct CratesIoLicenseSource {
    response: Value,
}

impl CratesIoLicenseSource {
    /// Create a source from a raw crates.io JSON API response.
    pub fn from_response(response: Value) -> Self {
        Self { response }
    }
}

impl LicenseSource for CratesIoLicenseSource {
    fn fetch_license(&self, _package: &str) -> Result<Option<String>, LicenseError> {
        let crate_obj = self
            .response
            .get("crate")
            .unwrap_or(&self.response);
        match crate_obj.get("license") {
            Some(Value::String(s)) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Some(Value::Null) | None => Ok(None),
            Some(other) => Err(LicenseError::Parse(format!(
                "unexpected crates.io license value: {other}"
            ))),
        }
    }
}

/// Fetch a license from the given source and check it against the policy.
pub fn check_dependency<S: LicenseSource>(
    source: &S,
    policy: &LicensePolicy,
    package: &str,
) -> Result<Option<Finding>, LicenseError> {
    let license = source.fetch_license(package)?;
    Ok(policy.check(package, license.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_policy_allows_common_licenses() {
        let policy = LicensePolicy::default();
        for license in DEFAULT_ALLOWED_LICENSES {
            assert!(policy.is_allowed(license), "{license} should be allowed");
        }
    }

    #[test]
    fn default_policy_rejects_copyleft() {
        let policy = LicensePolicy::default();
        assert!(!policy.is_allowed("GPL-3.0"));
        assert!(!policy.is_allowed("AGPL-3.0"));
    }

    #[test]
    fn policy_is_case_insensitive() {
        let policy = LicensePolicy::default();
        assert!(policy.is_allowed("mit"));
        assert!(policy.is_allowed("Apache-2.0"));
    }

    #[test]
    fn check_returns_none_for_allowed_license() {
        let policy = LicensePolicy::default();
        assert!(policy.check("foo", Some("MIT")).is_none());
    }

    #[test]
    fn check_returns_low_severity_finding_for_disallowed_license() {
        let policy = LicensePolicy::default();
        let finding = policy.check("foo", Some("GPL-3.0")).expect("finding");
        assert_eq!(finding.kind, FindingKind::License);
        assert_eq!(finding.severity, Severity::Low);
        assert!(finding.message.contains("foo"));
        assert!(finding.message.contains("GPL-3.0"));
    }

    #[test]
    fn check_returns_none_for_missing_license() {
        let policy = LicensePolicy::default();
        assert!(policy.check("foo", None).is_none());
        assert!(policy.check("foo", Some("")).is_none());
    }

    #[test]
    fn npm_allowed_license() {
        let response = json!({ "name": "left-pad", "license": "MIT" });
        let source = NpmLicenseSource::from_response(response);
        let policy = LicensePolicy::default();
        let finding = check_dependency(&source, &policy, "left-pad").unwrap();
        assert!(finding.is_none());
    }

    #[test]
    fn npm_disallowed_license() {
        let response = json!({ "name": "copyleft-pkg", "license": "GPL-3.0" });
        let source = NpmLicenseSource::from_response(response);
        let policy = LicensePolicy::default();
        let finding = check_dependency(&source, &policy, "copyleft-pkg")
            .unwrap()
            .expect("finding");
        assert_eq!(finding.kind, FindingKind::License);
        assert_eq!(finding.severity, Severity::Low);
    }

    #[test]
    fn npm_legacy_object_license() {
        let response = json!({ "name": "old-pkg", "license": { "type": "MIT" } });
        let source = NpmLicenseSource::from_response(response);
        assert_eq!(source.fetch_license("old-pkg").unwrap().as_deref(), Some("MIT"));
    }

    #[test]
    fn pypi_allowed_license() {
        let response = json!({
            "info": {
                "license": "MIT",
                "classifiers": []
            }
        });
        let source = PypiLicenseSource::from_response(response);
        let policy = LicensePolicy::default();
        let finding = check_dependency(&source, &policy, "requests").unwrap();
        assert!(finding.is_none());
    }

    #[test]
    fn pypi_disallowed_license() {
        let response = json!({
            "info": {
                "license": "GPL-3.0",
                "classifiers": []
            }
        });
        let source = PypiLicenseSource::from_response(response);
        let policy = LicensePolicy::default();
        let finding = check_dependency(&source, &policy, "copyleft-pkg")
            .unwrap()
            .expect("finding");
        assert_eq!(finding.kind, FindingKind::License);
        assert_eq!(finding.severity, Severity::Low);
    }

    #[test]
    fn pypi_classifier_fallback() {
        let response = json!({
            "info": {
                "license": "",
                "classifiers": [
                    "Programming Language :: Python",
                    "License :: OSI Approved :: MIT License"
                ]
            }
        });
        let source = PypiLicenseSource::from_response(response);
        assert_eq!(source.fetch_license("pkg").unwrap().as_deref(), Some("MIT"));
    }

    #[test]
    fn pypi_classifier_disallowed() {
        let response = json!({
            "info": {
                "license": "",
                "classifiers": [
                    "License :: OSI Approved :: GNU General Public License v3 (GPLv3)"
                ]
            }
        });
        let source = PypiLicenseSource::from_response(response);
        let policy = LicensePolicy::default();
        let finding = check_dependency(&source, &policy, "copyleft-pkg")
            .unwrap()
            .expect("finding");
        assert_eq!(finding.kind, FindingKind::License);
    }

    #[test]
    fn crates_io_allowed_license() {
        let response = json!({
            "crate": {
                "name": "serde",
                "license": "MIT OR Apache-2.0"
            }
        });
        let source = CratesIoLicenseSource::from_response(response);
        // "MIT OR Apache-2.0" is a compound expression; a policy that allows
        // both components should accept it when listed explicitly.
        let policy = LicensePolicy::new(["MIT OR Apache-2.0"]);
        let finding = check_dependency(&source, &policy, "serde").unwrap();
        assert!(finding.is_none());
    }

    #[test]
    fn crates_io_disallowed_license() {
        let response = json!({
            "crate": {
                "name": "copyleft-crate",
                "license": "GPL-3.0"
            }
        });
        let source = CratesIoLicenseSource::from_response(response);
        let policy = LicensePolicy::default();
        let finding = check_dependency(&source, &policy, "copyleft-crate")
            .unwrap()
            .expect("finding");
        assert_eq!(finding.kind, FindingKind::License);
        assert_eq!(finding.severity, Severity::Low);
    }

    #[test]
    fn crates_io_missing_license() {
        let response = json!({ "crate": { "name": "no-license" } });
        let source = CratesIoLicenseSource::from_response(response);
        assert!(source.fetch_license("no-license").unwrap().is_none());
    }
}