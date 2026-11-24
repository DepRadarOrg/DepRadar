//! npm lockfile and manifest parsers.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use serde::Deserialize;

use super::{is_non_registry_spec, LockfileError, ParseError, ParsedLockfile};
use crate::model::{Dependency, Ecosystem};

const NODE_MODULES: &str = "node_modules/";

#[derive(Debug, Deserialize)]
struct PackageLock {
    name: Option<String>,
    #[serde(rename = "lockfileVersion")]
    lockfile_version: Option<u32>,
    #[serde(default)]
    packages: BTreeMap<String, PackageEntry>,
}

#[derive(Debug, Deserialize)]
struct PackageEntry {
    /// Set when the install location's name differs from the real package
    /// name (npm aliases, `"foo": "npm:bar@1.0.0"`).
    name: Option<String>,
    version: Option<String>,
    /// A symlink to a workspace folder; the target is listed separately.
    #[serde(default)]
    link: bool,
    /// For a `link` entry, the key of the entry it points to.
    resolved: Option<String>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "optionalDependencies")]
    optional_dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "peerDependencies")]
    peer_dependencies: BTreeMap<String, String>,
}

impl PackageEntry {
    /// Names of every package this entry depends on, of any kind.
    fn dependency_names(&self) -> impl Iterator<Item = &str> {
        self.dependencies
            .keys()
            .chain(self.dev_dependencies.keys())
            .chain(self.optional_dependencies.keys())
            .chain(self.peer_dependencies.keys())
            .map(String::as_str)
    }
}

/// Parses the contents of an npm `package-lock.json` (lockfile v2/v3).
pub fn parse_lock(contents: &str) -> Result<ParsedLockfile, LockfileError> {
    let lock: PackageLock = serde_json::from_str(contents)
        .map_err(|err| LockfileError::Invalid(format!("package-lock.json: {err}")))?;

    let mut dependencies = Vec::new();
    let mut warnings = Vec::new();

    let direct_keys: BTreeSet<String> = lock
        .packages
        .get("")
        .map(|root| {
            root.dependency_names()
                .map(|name| format!("{NODE_MODULES}{name}"))
                .collect()
        })
        .unwrap_or_default();

    let paths = shortest_paths(&lock);

    if lock.packages.is_empty() {
        let version = lock
            .lockfile_version
            .map_or_else(|| "unknown".to_string(), |v| v.to_string());
        warnings.push(format!(
            "package-lock.json lockfileVersion {version} has no `packages` map; \
             only lockfile v2/v3 are supported, so no dependencies were scanned"
        ));
    }

    for (key, entry) in &lock.packages {
        if key.is_empty() || entry.link {
            continue;
        }
        let Some(name) = package_name(key, entry) else {
            continue;
        };

        let version = match &entry.version {
            Some(version) if is_registry_version(version) => Some(version.clone()),
            Some(version) => {
                warnings.push(format!(
                    "{name} (`{key}`) resolves to `{version}`, not a registry version; \
                     skipping vulnerability check"
                ));
                None
            }
            None => {
                warnings.push(format!(
                    "{name} (`{key}`) has no version; skipping vulnerability check"
                ));
                None
            }
        };

        // npm records git and remote-tarball installs as a URL `version`,
        // and local tarballs as a `file:` `resolved`.
        let source = match (&entry.version, &entry.resolved) {
            (Some(version), _) if !is_registry_version(version) => Some(version.clone()),
            (_, Some(resolved))
                if is_non_registry_spec(resolved) && !resolved.starts_with("http") =>
            {
                Some(resolved.clone())
            }
            _ => None,
        };

        dependencies.push(Dependency {
            path: paths.get(key.as_str()).cloned().unwrap_or_default(),
            name,
            version,
            ecosystem: Ecosystem::Npm,
            direct: direct_keys.contains(key),
            dev: false,
            source,
        });
    }

    Ok(ParsedLockfile {
        ecosystem: Ecosystem::Npm,
        dependencies,
        warnings,
    })
}

/// Breadth-first search from the root entry over the resolved dependency
/// graph, returning the shortest name path to every reachable install
/// location. Unreachable (extraneous) entries are absent.
fn shortest_paths(lock: &PackageLock) -> HashMap<&str, Vec<String>> {
    let mut paths: HashMap<&str, Vec<String>> = HashMap::new();
    let Some((root_key, root)) = lock.packages.get_key_value("") else {
        return paths;
    };
    let root_name = root
        .name
        .clone()
        .or_else(|| lock.name.clone())
        .unwrap_or_else(|| "(root)".to_string());

    let mut visited: HashSet<&str> = HashSet::from([root_key.as_str()]);
    let mut queue: VecDeque<(&str, Vec<String>)> = VecDeque::from([("", vec![root_name])]);

    while let Some((key, path)) = queue.pop_front() {
        let Some(entry) = lock.packages.get(key) else {
            continue;
        };
        for name in entry.dependency_names() {
            let Some(child) = resolve(&lock.packages, key, name) else {
                continue;
            };
            if !visited.insert(child) {
                continue;
            }
            let mut child_path = path.clone();
            child_path.push(display_name(child, &lock.packages[child]));
            paths.insert(child, child_path.clone());
            queue.push_back((child, child_path));
        }
    }

    paths
}

/// The install location Node would load when the package at `from` requires
/// `name`: the nearest `node_modules/<name>` walking up from `from` to the
/// project root, following workspace links to their target.
fn resolve<'a>(
    packages: &'a BTreeMap<String, PackageEntry>,
    from: &str,
    name: &str,
) -> Option<&'a str> {
    let mut base = from;
    loop {
        let candidate = if base.is_empty() {
            format!("{NODE_MODULES}{name}")
        } else {
            format!("{base}/{NODE_MODULES}{name}")
        };
        if let Some((key, entry)) = packages.get_key_value(&candidate) {
            return match (entry.link, entry.resolved.as_deref()) {
                (true, Some(target)) => packages.get_key_value(target).map(|(k, _)| k.as_str()),
                _ => Some(key.as_str()),
            };
        }
        if base.is_empty() {
            return None;
        }
        base = match base.rfind("/node_modules/") {
            Some(index) => &base[..index],
            None => "",
        };
    }
}

/// Name to show for an entry in a dependency path. Workspace packages live
/// outside `node_modules/`, so fall back to their folder name.
fn display_name(key: &str, entry: &PackageEntry) -> String {
    package_name(key, entry).unwrap_or_else(|| {
        entry
            .name
            .clone()
            .unwrap_or_else(|| key.rsplit('/').next().unwrap_or(key).to_string())
    })
}

/// The package name for a `packages` entry: the explicit `name` if the lock
/// records one, otherwise the path segment(s) after the last `node_modules/`
/// (which keeps scoped names like `@babel/core` whole). `None` for entries
/// that aren't under `node_modules/`.
fn package_name(key: &str, entry: &PackageEntry) -> Option<String> {
    let start = key.rfind(NODE_MODULES)? + NODE_MODULES.len();
    let name = entry.name.as_deref().unwrap_or(&key[start..]);
    (!name.is_empty()).then(|| name.to_string())
}

/// False for versions npm records for non-registry installs: git URLs,
/// tarball URLs and `file:` paths. OSV can't match those.
fn is_registry_version(version: &str) -> bool {
    !version.contains(':') && !version.contains('/')
}

/// The dependency sections of a `package.json`. Only the package names are
/// used; the version ranges (or git/file/tag specs) they map to are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageJson {
    #[serde(default)]
    dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    dev_dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    optional_dependencies: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    peer_dependencies: BTreeMap<String, serde_json::Value>,
}

/// Parses the direct dependencies declared in a `package.json`.
///
/// Without a lockfile there are no resolved versions, but the names a
/// project depends on can still be typosquat-checked. Reads
/// `dependencies`, `devDependencies`, `optionalDependencies` and
/// `peerDependencies`; every entry is `direct: true` with `version: None`.
///
/// A name listed in more than one section is returned once. It's only
/// marked `dev` when `devDependencies` is the only place it appears — a
/// library commonly lists a peer dependency under `devDependencies` too, so
/// it can run its own tests against it.
pub fn parse_package_json(contents: &str) -> Result<Vec<Dependency>, ParseError> {
    let manifest: PackageJson = serde_json::from_str(contents)?;

    let runtime = manifest
        .dependencies
        .into_keys()
        .chain(manifest.optional_dependencies.into_keys())
        .chain(manifest.peer_dependencies.into_keys())
        .map(|name| (name, false));
    let dev = manifest
        .dev_dependencies
        .into_keys()
        .map(|name| (name, true));

    let mut seen = BTreeSet::new();
    Ok(runtime
        .chain(dev)
        .filter(|(name, _)| seen.insert(name.clone()))
        .map(|(name, dev)| Dependency {
            path: vec![name.clone()],
            name,
            version: None,
            ecosystem: Ecosystem::Npm,
            direct: true,
            dev,
            source: None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE_JSON: &str = include_str!("../../tests/fixtures/package.json");

    fn find<'a>(deps: &'a [Dependency], name: &str) -> &'a Dependency {
        deps.iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name} not found in {deps:?}"))
    }

    #[test]
    fn parses_all_four_sections() {
        let deps = parse_package_json(PACKAGE_JSON).unwrap();
        assert!(deps.iter().all(|d| d.ecosystem == Ecosystem::Npm));
        assert!(deps.iter().all(|d| d.direct && d.version.is_none()));

        // dependencies
        assert!(!find(&deps, "express").dev);
        assert!(!find(&deps, "@angular/core").dev);
        // devDependencies
        assert!(find(&deps, "typescript").dev);
        assert!(find(&deps, "@types/node").dev);
        // optionalDependencies
        assert!(!find(&deps, "fsevents").dev);
        // peerDependencies
        assert!(!find(&deps, "rxjs").dev);
    }

    #[test]
    fn names_in_several_sections_are_listed_once() {
        let deps = parse_package_json(PACKAGE_JSON).unwrap();
        assert_eq!(deps.len(), 8);

        // Both a peer and a dev dependency: kept once, not dev-only.
        let react: Vec<_> = deps.iter().filter(|d| d.name == "react").collect();
        assert_eq!(react.len(), 1);
        assert!(!react[0].dev);
    }

    #[test]
    fn missing_sections_are_empty() {
        let deps = parse_package_json(r#"{"name": "empty", "version": "1.0.0"}"#).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_package_json("{").is_err());
    }
}
