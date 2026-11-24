//! Rust (crates.io) lockfile parser.

use serde::Deserialize;

use super::ParseError;
use crate::model::{Dependency, Ecosystem};

/// The parts of a `Cargo.lock` we read: its `[[package]]` entries.
#[derive(Debug, Deserialize)]
struct CargoLock {
    #[serde(default)]
    package: Vec<CargoLockPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoLockPackage {
    name: String,
    version: String,
    /// Where the package was fetched from (`registry+...` or `git+...`).
    /// Absent for workspace members and path dependencies.
    source: Option<String>,
}

/// Parses a `Cargo.lock` into its locked dependencies.
///
/// Every `[[package]]` entry with a `source` is returned with its pinned
/// version. Entries without one are the workspace's own crates (or local
/// path dependencies) and are skipped. Crates fetched from git keep their
/// `git+...` source in [`Dependency::source`]. `Cargo.lock` doesn't record
/// which crates are declared directly or only used for development, so
/// `direct` and `dev` are always `false`.
pub fn parse_cargo_lock(contents: &str) -> Result<Vec<Dependency>, ParseError> {
    let lock: CargoLock = toml::from_str(contents)?;

    Ok(lock
        .package
        .into_iter()
        .filter(|package| package.source.is_some())
        .map(|package| Dependency {
            name: package.name,
            version: Some(package.version),
            ecosystem: Ecosystem::CratesIo,
            direct: false,
            dev: false,
            path: Vec::new(),
            source: package.source.filter(|source| source.starts_with("git+")),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_CARGO_LOCK: &str = include_str!("../../Cargo.lock");

    #[test]
    fn parses_this_repos_cargo_lock() {
        let deps = parse_cargo_lock(OWN_CARGO_LOCK).unwrap();
        assert!(deps.iter().all(|d| d.ecosystem == Ecosystem::CratesIo));

        let soroban = deps
            .iter()
            .find(|d| d.name == "soroban-sdk")
            .expect("soroban-sdk should be in this repo's Cargo.lock");
        assert!(soroban.version.is_some());
    }

    #[test]
    fn skips_workspace_members() {
        let deps = parse_cargo_lock(OWN_CARGO_LOCK).unwrap();
        assert!(!deps.iter().any(|d| d.name == "deprader"));
        assert!(!deps.iter().any(|d| d.name == "deprader-router"));
    }

    #[test]
    fn reads_registry_and_git_sources() {
        let lock = r#"
            version = 4

            [[package]]
            name = "my-app"
            version = "0.1.0"
            dependencies = ["serde", "local-helper"]

            [[package]]
            name = "local-helper"
            version = "0.2.0"

            [[package]]
            name = "serde"
            version = "1.0.200"
            source = "registry+https://github.com/rust-lang/crates.io-index"
            checksum = "ddc6f9cc94d67c0e21aaf7eda3a010fd3af78ebf6e096aa6e2e13c79749cce4f"

            [[package]]
            name = "forked"
            version = "0.3.0"
            source = "git+https://github.com/example/forked?branch=main#0123abcd"
        "#;
        let deps = parse_cargo_lock(lock).unwrap();
        let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["serde", "forked"]);
        assert_eq!(deps[0].version.as_deref(), Some("1.0.200"));
        assert_eq!(deps[0].source, None);
        assert_eq!(
            deps[1].source.as_deref(),
            Some("git+https://github.com/example/forked?branch=main#0123abcd")
        );
    }

    #[test]
    fn invalid_toml_is_an_error() {
        assert!(parse_cargo_lock("[[package]\nname = ").is_err());
    }
}
