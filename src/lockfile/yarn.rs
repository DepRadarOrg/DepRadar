//! Yarn classic (v1) `yarn.lock` parser.

use std::collections::BTreeSet;

use super::ParseError;
use crate::model::{Dependency, Ecosystem};

/// Parses a Yarn v1 `yarn.lock` into its locked dependencies.
///
/// An entry is a header line listing one or more comma-separated specifiers
/// (`"name@^1.0.0", name@~1.1:`), followed by indented fields of which only
/// `version "1.1.3"` is read. Scoped names (`@scope/name@range`) and entries
/// with several specifiers are supported; each `(name, version)` pair is
/// returned once. `yarn.lock` doesn't record which packages the project
/// declares itself, so `direct` and `dev` are always `false`.
pub fn parse_yarn_lock(contents: &str) -> Result<Vec<Dependency>, ParseError> {
    let mut seen = BTreeSet::new();
    let mut deps = Vec::new();
    let mut current: Option<String> = None;

    for line in contents.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if !line.starts_with(char::is_whitespace) {
            current = header_name(line);
        } else if let (Some(name), Some(version)) = (&current, field_version(line)) {
            if seen.insert((name.clone(), version.to_string())) {
                deps.push(Dependency {
                    name: name.clone(),
                    version: Some(version.to_string()),
                    ecosystem: Ecosystem::Npm,
                    direct: false,
                    dev: false,
                });
            }
            current = None;
        }
    }
    Ok(deps)
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
