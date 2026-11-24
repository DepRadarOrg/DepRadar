//! Name normalization applied before any typosquat signal compares a
//! candidate against the popular-package dataset.
//!
//! # Case
//!
//! npm package names are lowercase, so [`normalize_name`] lowercases a
//! candidate before it is scored. `React` is therefore compared as `react`
//! (and reported as an exact match, not a one-edit typo).
//!
//! # Scoped names
//!
//! An npm scoped name (`@scope/name`) has two independent parts. A raw
//! Levenshtein distance over the whole string lets edits cross the `/`
//! boundary and compares a scope against a bare name, which gives misleading
//! results. [`split_npm_name`] separates the two parts, and [`name_distance`]
//! compares them pairwise:
//!
//! - both scoped: scope-to-scope distance plus name-to-name distance, so
//!   `@typess/node` vs `@types/node` is 1 (scope) + 0 (name);
//! - both unscoped: plain distance between the bare names;
//! - one scoped and one not: plain distance over the full strings. The `@`
//!   and `/` alone keep such pairs far apart, so they rarely land inside the
//!   scanner's threshold. Dropping a scope (`types-node`) is handled by the
//!   separate scope-confusion signal instead.

use super::distance::levenshtein_distance;

/// Splits an npm package name into its scope (without the leading `@`) and
/// its bare name.
///
/// `@types/node` gives `(Some("types"), "node")`; `react` gives
/// `(None, "react")`. A string that starts with `@` but has no `/` is not a
/// valid scoped name and is returned whole as an unscoped name.
pub fn split_npm_name(name: &str) -> (Option<&str>, &str) {
    if let Some(rest) = name.strip_prefix('@') {
        if let Some((scope, bare)) = rest.split_once('/') {
            return (Some(scope), bare);
        }
    }
    (None, name)
}

/// Lowercases `name` so it can be compared against the (all-lowercase) npm
/// dataset.
pub fn normalize_name(name: &str) -> String {
    name.to_lowercase()
}

/// Maps every separator (`-`, `_`, `.`) to a single canonical `-` so that
/// names differing only in separator choice compare equal.
///
/// `date_fns` gives `date-fns` and `lodash.merge` gives `lodash-merge`. The
/// input is lowercased first, so the result is directly comparable against
/// the (all-lowercase) dataset.
pub fn normalize_separators(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| match c {
            '-' | '_' | '.' => '-',
            other => other,
        })
        .collect()
}

/// Edit distance between two package names that compares scoped names
/// scope-to-scope and name-to-name (see the module docs).
pub fn name_distance(a: &str, b: &str) -> usize {
    match (split_npm_name(a), split_npm_name(b)) {
        ((Some(scope_a), bare_a), (Some(scope_b), bare_b)) => {
            levenshtein_distance(scope_a, scope_b) + levenshtein_distance(bare_a, bare_b)
        }
        ((None, bare_a), (None, bare_b)) => levenshtein_distance(bare_a, bare_b),
        _ => levenshtein_distance(a, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_scoped_name() {
        assert_eq!(split_npm_name("@types/node"), (Some("types"), "node"));
    }

    #[test]
    fn unscoped_name_has_no_scope() {
        assert_eq!(split_npm_name("react"), (None, "react"));
        assert_eq!(split_npm_name("lodash.merge"), (None, "lodash.merge"));
    }

    #[test]
    fn at_sign_without_slash_is_not_a_scope() {
        assert_eq!(split_npm_name("@types"), (None, "@types"));
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize_name("React"), "react");
        assert_eq!(normalize_name("@Types/Node"), "@types/node");
    }

    #[test]
    fn separators_map_to_dash() {
        assert_eq!(normalize_separators("date_fns"), "date-fns");
        assert_eq!(normalize_separators("lodash.merge"), "lodash-merge");
        assert_eq!(normalize_separators("date-fns"), "date-fns");
    }

    #[test]
    fn separator_normalisation_lowercases() {
        assert_eq!(normalize_separators("Date_Fns"), "date-fns");
    }

    #[test]
    fn separator_swaps_compare_equal() {
        assert_eq!(
            normalize_separators("date_fns"),
            normalize_separators("date-fns")
        );
        assert_eq!(
            normalize_separators("lodash.merge"),
            normalize_separators("lodash-merge")
        );
    }

    #[test]
    fn non_separator_characters_are_untouched() {
        assert_eq!(normalize_separators("react"), "react");
    }

    #[test]
    fn scoped_names_are_compared_part_by_part() {
        assert_eq!(name_distance("@types/node", "@types/node"), 0);
        assert_eq!(name_distance("@typess/node", "@types/node"), 1);
        assert_eq!(name_distance("@types/nod", "@types/node"), 1);
    }

    #[test]
    fn unscoped_names_use_plain_distance() {
        assert_eq!(name_distance("expresss", "express"), 1);
    }
}
