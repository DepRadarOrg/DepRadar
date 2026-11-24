//! Yarn `yarn.lock` parser: classic (v1) and Berry (v2+).

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::{is_non_registry_spec, ParseError};
use crate::model::{Dependency, Ecosystem};

/// Parses a Yarn v1 `yarn.lock` into its locked dependencies.
///
/// An entry is a header line listing one or more comma-separated specifiers
/// (`"name@^1.0.0", name@~1.1:`), followed by indented fields of which only
/// `version "1.1.3"` is read. Scoped names (`@scope/name@range`) and entries
/// with several specifiers are supported; each `(name, version)` pair is
/// returned once. `yarn.lock` doesn't record which packages the project
/// declares itself, so `direct` and `dev` are always `false`.
///
/// A Berry lockfile, recognised by its top-level `__metadata` key, is
/// parsed by `parse_berry_lock` instead.
pub fn parse_yarn_lock(contents: &str) -> Result<Vec<Dependency>, ParseError> {
    if contents.lines().any(|line| line.starts_with("__metadata:")) {
        return parse_berry_lock(contents);
    }

    let mut seen = BTreeSet::new();
    let mut deps = Vec::new();
    let mut current: Option<(String, Option<String>)> = None;

    for line in contents.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if !line.starts_with(char::is_whitespace) {
            current = header_source(line)
                .map(|(name, source)| (name, Some(source)))
                .or_else(|| header_name(line).map(|name| (name, None)));
        } else if let (Some((name, source)), Some(version)) = (&current, field_version(line)) {
            if seen.insert((name.clone(), version.to_string())) {
                deps.push(Dependency {
                    name: name.clone(),
                    version: Some(version.to_string()),
                    ecosystem: Ecosystem::Npm,
                    direct: false,
                    dev: false,
                    path: Vec::new(),
                    source: source.clone(),
                });
            }
            current = None;
        }
    }
    Ok(deps)
}

/// The field of a Yarn Berry entry we read.
#[derive(Debug, Deserialize)]
struct BerryEntry {
    /// The locked package, e.g. `@babel/core@npm:7.12.3`.
    resolution: Option<String>,
}

/// Parses a Yarn Berry (v2+) `yarn.lock`, which is YAML.
///
/// Every top-level key other than `__metadata` is an entry whose
/// `resolution` (`name@npm:1.2.3`) gives the package name and version. Only
/// entries resolved from the npm registry are returned; the project's own
/// `workspace:` entries and other protocols (`patch:`, `link:`, ...) are
/// skipped. Each `(name, version)` pair is returned once, and `direct` and
/// `dev` are always `false`, as for v1.
fn parse_berry_lock(contents: &str) -> Result<Vec<Dependency>, ParseError> {
    let entries: BTreeMap<String, serde_yaml::Value> = serde_yaml::from_str(contents)?;
    let mut seen = BTreeSet::new();
    let mut deps = Vec::new();

    for (key, value) in entries {
        if key == "__metadata" {
            continue;
        }
        let entry: BerryEntry = serde_yaml::from_value(value)?;
        let Some(resolution) = entry.resolution else {
            continue;
        };
        let Some((name, version)) = berry_resolution(&resolution) else {
            continue;
        };
        if seen.insert((name.to_string(), version.to_string())) {
            deps.push(Dependency {
                name: name.to_string(),
                version: Some(version.to_string()),
                ecosystem: Ecosystem::Npm,
                direct: false,
                dev: false,
                path: Vec::new(),
            });
        }
    }
    Ok(deps)
}

/// Splits a Berry resolution such as `@babel/core@npm:7.12.3` into the
/// package name and its npm version. Returns `None` for other protocols.
fn berry_resolution(resolution: &str) -> Option<(&str, &str)> {
    // The name/protocol separator is the first `@` after position 0, which
    // keeps the leading `@` of a scope.
    let at = resolution.get(1..)?.find('@')? + 1;
    let name = &resolution[..at];
    let version = resolution[at + 1..].strip_prefix("npm:")?;
    (!version.is_empty()).then_some((name, version))
}

/// Extracts the package name from an entry header such as
/// `"@babel/code-frame@^7.0.0", "@babel/code-frame@^7.10.4":`. All
/// specifiers in one entry resolve to the same package, so the first one is
/// enough.
fn header_name(line: &str) -> Option<String> {
    let header = line.trim_end().strip_suffix(':')?;
    let specifier = header.split(',').next()?.trim().trim_matches('"');
    // The name/range separator is the last `@` after position 0, which
    // keeps the leading `@` of a scope.
    let at = specifier.get(1..)?.rfind('@')? + 1;
    let name = &specifier[..at];
    (!name.is_empty()).then(|| name.to_string())
}

/// For an entry header whose first specifier installs from outside the
/// registry (`"pkg@git+https://…"`, `pkg@file:./pkg`, a tarball URL),
/// returns the package name and that source. The name ends at the first `@`
/// after position 0, since a URL may contain `@` itself.
fn header_source(line: &str) -> Option<(String, String)> {
    let header = line.trim_end().strip_suffix(':')?;
    let specifier = header.split(',').next()?.trim().trim_matches('"');
    let at = specifier.get(1..)?.find('@')? + 1;
    let (name, range) = (&specifier[..at], &specifier[at + 1..]);
    (!name.is_empty() && is_non_registry_spec(range))
        .then(|| (name.to_string(), range.to_string()))
}

/// Returns the value of an indented `version "x.y.z"` field.
fn field_version(line: &str) -> Option<&str> {
    // Only top-level fields (two-space indent), not nested dependencies.
    let rest = line.strip_prefix("  ")?;
    let value = rest.strip_prefix("version")?.trim();
    Some(value.trim_matches('"'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const YARN_LOCK: &str = include_str!("../../tests/fixtures/yarn.lock");
    const YARN_BERRY_LOCK: &str = include_str!("../../tests/fixtures/yarn-berry/yarn.lock");

    fn find<'a>(deps: &'a [Dependency], name: &str) -> &'a Dependency {
        deps.iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} not found in {deps:?}"))
    }

    #[test]
    fn parses_scoped_and_multi_specifier_entries() {
        let deps = parse_yarn_lock(YARN_LOCK).unwrap();
        assert_eq!(deps.len(), 4);
        assert!(deps.iter().all(|d| d.ecosystem == Ecosystem::Npm));

        // Scoped, two quoted specifiers.
        assert_eq!(
            find(&deps, "@babel/code-frame").version.as_deref(),
            Some("7.12.13")
        );
        // Scoped, single specifier.
        assert_eq!(
            find(&deps, "@babel/highlight").version.as_deref(),
            Some("7.13.10")
        );
        // Unscoped, unquoted comma-separated specifiers.
        assert_eq!(find(&deps, "lodash").version.as_deref(), Some("4.17.21"));
        // Exact-version specifier.
        assert_eq!(find(&deps, "left-pad").version.as_deref(), Some("1.3.0"));
    }

    #[test]
    fn parses_berry_lockfile() {
        let deps = parse_yarn_lock(YARN_BERRY_LOCK).unwrap();
        assert_eq!(deps.len(), 4);
        assert!(deps.iter().all(|d| d.ecosystem == Ecosystem::Npm));
        assert!(deps.iter().all(|d| !d.direct && !d.dev));

        assert_eq!(
            find(&deps, "@babel/code-frame").version.as_deref(),
            Some("7.12.13")
        );
        assert_eq!(
            find(&deps, "@babel/highlight").version.as_deref(),
            Some("7.13.10")
        );
        assert_eq!(find(&deps, "lodash").version.as_deref(), Some("4.17.21"));
        assert_eq!(find(&deps, "left-pad").version.as_deref(), Some("1.3.0"));
        // The project's own workspace entry is skipped.
        assert!(!deps.iter().any(|d| d.name == "my-app"));
    }

    #[test]
    fn berry_resolution_reads_npm_entries_only() {
        assert_eq!(
            berry_resolution("@babel/core@npm:7.12.3"),
            Some(("@babel/core", "7.12.3"))
        );
        assert_eq!(berry_resolution("my-app@workspace:."), None);
        assert_eq!(
            berry_resolution("resolve@patch:resolve@npm%3A1.20.0#~builtin<compat/resolve>"),
            None
        );
    }

    #[test]
    fn invalid_berry_lockfile_is_an_error() {
        let lockfile = "__metadata:\n  version: 8\nfoo: [\n";
        assert!(parse_yarn_lock(lockfile).is_err());
    }

    #[test]
    fn empty_lockfile_has_no_dependencies() {
        let deps = parse_yarn_lock("# yarn lockfile v1\n\n").unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn header_name_splits_on_the_last_at() {
        assert_eq!(header_name("lodash@^4.0.0:").as_deref(), Some("lodash"));
        assert_eq!(
            header_name("\"@scope/pkg@^1.0.0\", \"@scope/pkg@~1.1\":").as_deref(),
            Some("@scope/pkg")
        );
        assert_eq!(header_name("  version \"1.0.0\""), None);
    }
}
