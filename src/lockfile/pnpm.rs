//! pnpm `pnpm-lock.yaml` parser (lockfile v6 and v9).

use std::collections::BTreeMap;

use serde::Deserialize;

use super::ParseError;
use crate::model::{Dependency, Ecosystem};

/// The parts of a `pnpm-lock.yaml` we read: the `packages:` map. Everything
/// else (importers, snapshots, settings) is ignored.
#[derive(Debug, Deserialize)]
struct PnpmLock {
    #[serde(default)]
    packages: BTreeMap<String, PnpmPackage>,
}

#[derive(Debug, Default, Deserialize)]
struct PnpmPackage {
    /// Present in v6 (`dev: true`/`false`); v9 dropped it.
    #[serde(default)]
    dev: Option<bool>,
}

/// Parses a `pnpm-lock.yaml` (lockfile v6 or v9) into its locked
/// dependencies.
///
/// Reads the `packages:` section. v6 keys look like `/name@version` and v9
/// keys like `name@version`; both may be scoped (`@scope/name@version`) and
/// v6 may carry a peer-dependency suffix (`/name@1.0.0(react@18.2.0)`),
/// which is dropped. `dev` follows the v6 `dev:` field and is `false` for
/// v9, which doesn't record it. The lockfile doesn't mark which packages the
/// project declares itself, so `direct` is always `false`.
pub fn parse_pnpm_lock(contents: &str) -> Result<Vec<Dependency>, ParseError> {
    let lock: PnpmLock = serde_yaml::from_str(contents)?;

    Ok(lock
        .packages
        .into_iter()
        .filter_map(|(key, package)| {
            let (name, version) = split_key(&key)?;
            Some(Dependency {
                name: name.to_string(),
                version: Some(version.to_string()),
                ecosystem: Ecosystem::Npm,
                direct: false,
                dev: package.dev.unwrap_or(false),
            })
        })
        .collect())
}

/// Splits a `packages:` key into `(name, version)`, or `None` when it isn't
/// a registry package (for example a tarball or git dependency).
fn split_key(key: &str) -> Option<(&str, &str)> {
    let key = key.strip_prefix('/').unwrap_or(key);
    // Peer-dependency suffix: `1.0.0(react@18.2.0)` -> `1.0.0`.
    let key = key.split('(').next()?;
    // Skip the leading `@` of a scope when looking for the separator.
    let at = key.get(1..)?.rfind('@')? + 1;
    let (name, version) = (&key[..at], &key[at + 1..]);
    (!name.is_empty() && !version.is_empty()).then_some((name, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    const V6: &str = include_str!("../../tests/fixtures/pnpm-lock-v6.yaml");
    const V9: &str = include_str!("../../tests/fixtures/pnpm-lock-v9.yaml");

    fn find<'a>(deps: &'a [Dependency], name: &str) -> &'a Dependency {
        deps.iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} not found in {deps:?}"))
    }

    #[test]
    fn parses_v6_slash_prefixed_keys() {
        let deps = parse_pnpm_lock(V6).unwrap();
        assert_eq!(deps.len(), 3);
        assert!(deps.iter().all(|d| d.ecosystem == Ecosystem::Npm));

        let core = find(&deps, "@babel/core");
        assert_eq!(core.version.as_deref(), Some("7.23.0"));
        assert!(core.dev);

        let express = find(&deps, "express");
        assert_eq!(express.version.as_deref(), Some("4.18.2"));
        assert!(!express.dev);

        // Peer-dependency suffix is dropped.
        assert_eq!(find(&deps, "react-dom").version.as_deref(), Some("18.2.0"));
    }

    #[test]
    fn parses_v9_bare_keys() {
        let deps = parse_pnpm_lock(V9).unwrap();
        assert_eq!(deps.len(), 3);

        assert_eq!(
            find(&deps, "@babel/core").version.as_deref(),
            Some("7.24.0")
        );
        assert_eq!(find(&deps, "express").version.as_deref(), Some("4.18.2"));
        assert_eq!(find(&deps, "react-dom").version.as_deref(), Some("18.2.0"));
        assert!(deps.iter().all(|d| !d.dev && !d.direct));
    }

    #[test]
    fn missing_packages_section_is_empty() {
        assert!(parse_pnpm_lock("lockfileVersion: '9.0'\n")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn invalid_yaml_is_an_error() {
        assert!(parse_pnpm_lock("packages: [unclosed").is_err());
    }

    #[test]
    fn split_key_handles_scopes_suffixes_and_non_registry_keys() {
        assert_eq!(split_key("/a@1.0.0"), Some(("a", "1.0.0")));
        assert_eq!(split_key("@s/a@1.0.0"), Some(("@s/a", "1.0.0")));
        assert_eq!(split_key("a@1.0.0(b@2.0.0)"), Some(("a", "1.0.0")));
        assert_eq!(split_key("no-version"), None);
    }
}
