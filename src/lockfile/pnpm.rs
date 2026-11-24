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
    /// Recorded for packages whose key doesn't carry a registry
    /// `name@version` (git, tarball and directory installs).
    name: Option<String>,
    version: Option<String>,
    resolution: Option<PnpmResolution>,
}

/// Where pnpm fetched a package from. Registry packages only carry an
/// `integrity`; the other fields mark non-registry installs.
#[derive(Debug, Default, Deserialize)]
struct PnpmResolution {
    tarball: Option<String>,
    repo: Option<String>,
    commit: Option<String>,
    directory: Option<String>,
}

impl PnpmResolution {
    /// The non-registry location this resolution points at, if any.
    fn source(&self) -> Option<String> {
        if let Some(repo) = &self.repo {
            return Some(match &self.commit {
                Some(commit) => format!("{repo}#{commit}"),
                None => repo.clone(),
            });
        }
        if let Some(directory) = &self.directory {
            return Some(format!("file:{directory}"));
        }
        self.tarball.clone()
    }
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
///
/// Git, tarball and directory installs keep their location in
/// [`Dependency::source`], taking their name and version from the entry's
/// own `name` / `version` fields since the key doesn't hold a registry pin.
pub fn parse_pnpm_lock(contents: &str) -> Result<Vec<Dependency>, ParseError> {
    let lock: PnpmLock = serde_yaml::from_str(contents)?;

    Ok(lock
        .packages
        .into_iter()
        .filter_map(|(key, package)| {
            let source = package.resolution.as_ref().and_then(PnpmResolution::source);
            let (name, version) = match (split_key(&key), &source) {
                (Some((name, version)), None) => (name.to_string(), Some(version.to_string())),
                (split, Some(_)) => (
                    package
                        .name
                        .clone()
                        .or_else(|| split.map(|(name, _)| name.to_string()))?,
                    package.version.clone(),
                ),
                (None, None) => return None,
            };
            Some(Dependency {
                name,
                version,
                ecosystem: Ecosystem::Npm,
                direct: false,
                dev: package.dev.unwrap_or(false),
                path: Vec::new(),
                source,
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

    #[test]
    fn records_git_and_directory_sources() {
        let lock = r#"
lockfileVersion: '6.0'
packages:
  /express@4.18.2:
    resolution: {integrity: sha512-aaa}
  github.com/example/forked/0123abcd:
    resolution: {tarball: https://codeload.github.com/example/forked/tar.gz/0123abcd}
    name: forked
    version: 1.0.0
  file:packages/local-utils:
    resolution: {directory: packages/local-utils, type: directory}
    name: local-utils
    version: 0.1.0
"#;
        let deps = parse_pnpm_lock(lock).unwrap();
        assert_eq!(deps.len(), 3);
        assert_eq!(find(&deps, "express").source, None);

        let forked = find(&deps, "forked");
        assert_eq!(forked.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            forked.source.as_deref(),
            Some("https://codeload.github.com/example/forked/tar.gz/0123abcd")
        );
        assert_eq!(
            find(&deps, "local-utils").source.as_deref(),
            Some("file:packages/local-utils")
        );
    }
}
