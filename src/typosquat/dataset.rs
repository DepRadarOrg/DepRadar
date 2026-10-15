//! Bundled lists of popular npm and PyPI package names.
//!
//! Typosquatting attacks work by publishing a package whose name is one edit
//! away from something popular (`reqeusts`, `expresss`, `lodahs`). Detecting
//! that needs a reference set of "names worth imitating" to compare against.
//! This module ships that reference set as two JSON arrays under
//! `data/popular-packages/` and exposes a single loader that returns them as
//! one combined list.
//!
//! The lists are a hand-curated, representative snapshot of widely-used
//! packages — not a live-fetched ranking. Refreshing them from the registries'
//! real download stats is a later task; nothing here depends on the exact
//! ordering or count.

/// Raw JSON text of the npm popular-package list, embedded at compile time.
const NPM_TOP_PACKAGES_JSON: &str = include_str!("../../data/popular-packages/npm-top1000.json");

/// Raw JSON text of the PyPI popular-package list, embedded at compile time.
const PYPI_TOP_PACKAGES_JSON: &str = include_str!("../../data/popular-packages/pypi-top1000.json");

/// Loads the bundled npm and PyPI popular-package lists and returns them as a
/// single combined list.
///
/// npm entries come first, in file order, followed by PyPI entries in file
/// order. Names are returned as-is; no normalisation or de-duplication is
/// applied — that belongs with the (not-yet-written) scoring logic.
pub fn load_popular_packages() -> Vec<String> {
    let mut combined = parse_string_array(NPM_TOP_PACKAGES_JSON);
    combined.extend(parse_string_array(PYPI_TOP_PACKAGES_JSON));
    combined
}

/// Minimal parser for a JSON document that is an array of strings.
///
/// The bundled data files are fully under our control and contain only a flat
/// array of plain string literals, so this walks the text collecting the
/// contents of each double-quoted span rather than pulling in a JSON
/// dependency. Standard string escapes (`\"`, `\\`, `\n`, `\t`, `\r`, `\/`)
/// are handled; `\uXXXX` is not, because package names never need it.
fn parse_string_array(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for c in raw.chars() {
        if !in_string {
            if c == '"' {
                in_string = true;
                current.clear();
            }
            continue;
        }

        if escaped {
            match c {
                'n' => current.push('\n'),
                't' => current.push('\t'),
                'r' => current.push('\r'),
                other => current.push(other), // covers '"', '\\', '/'
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            out.push(std::mem::take(&mut current));
            in_string = false;
        } else {
            current.push(c);
        }
    }

    out
}

/// Returns the zero-based position (rank) of `name` in the combined popular
/// package dataset, or `None` if it is not present.
///
/// The dataset is already ordered by popularity within each registry (npm
/// entries first, then PyPI), so a lower number means a more popular (and
/// more dangerous to typosquat) target. `load_popular_packages` defines that
/// order.
pub fn rank(name: &str) -> Option<usize> {
    load_popular_packages()
        .iter()
        .position(|entry| entry == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_returns_non_empty_list_including_known_packages() {
        let packages = load_popular_packages();

        assert!(
            !packages.is_empty(),
            "combined popular-package list should not be empty"
        );

        // `react` (npm) and `requests` (PyPI) are about as well-known as it
        // gets on each registry; if either is missing the data files are wrong.
        assert!(
            packages.iter().any(|name| name == "react"),
            "expected npm package 'react' in the combined list"
        );
        assert!(
            packages.iter().any(|name| name == "requests"),
            "expected PyPI package 'requests' in the combined list"
        );
    }
}
