//! Combosquatting signal: a popular name with bait words attached.
//!
//! Combosquats (`lodash-utils`, `requests-secure`) add a plausible-sounding
//! word to a popular package name instead of misspelling it, so their edit
//! distance is large and neither the distance nor the homoglyph signal
//! fires. [`combosquat`] splits a candidate on `-`, `_` and `.` and flags it
//! when the tokens are a popular package plus nothing but [`BAIT_WORDS`].

/// Words attackers commonly attach to a popular package name. Extend this
/// list to widen the signal; every entry must be lowercase.
pub const BAIT_WORDS: &[&str] = &[
    "utils", "util", "secure", "security", "js", "dev", "core", "plus", "api", "lib", "sdk", "cli",
    "helper", "helpers", "tools", "official", "pro", "fix", "patch", "latest", "new", "v2", "node",
    "py", "python",
];

/// Byte ranges of the tokens of `name` separated by `-`, `_` or `.`.
fn token_ranges(name: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (i, c) in name.char_indices() {
        if matches!(c, '-' | '_' | '.') {
            ranges.push((start, i));
            start = i + c.len_utf8();
        }
    }
    ranges.push((start, name.len()));
    ranges
}

/// Reports whether `candidate` is a popular package name combined with bait
/// words.
///
/// The candidate is split into tokens on `-`, `_` and `.`. It is flagged
/// with `Some(popular)` when a run of consecutive tokens is exactly a
/// popular package (so multi-token names like `babel-core` count) and every
/// remaining token is in [`BAIT_WORDS`], with at least one remaining token.
/// The longest matching popular name wins, so `babel-core-secure` reports
/// `babel-core` rather than `babel`.
///
/// A candidate that is itself in `dataset` is never flagged, so real
/// packages such as `react-dom` stay clean. `candidate` is expected to be
/// lowercased already.
pub fn combosquat(candidate: &str, dataset: &[String]) -> Option<String> {
    if dataset.iter().any(|name| name == candidate) {
        return None;
    }

    let tokens = token_ranges(candidate);
    let is_bait = |&(start, end): &(usize, usize)| BAIT_WORDS.contains(&&candidate[start..end]);

    let mut best: Option<&String> = None;
    for first in 0..tokens.len() {
        for last in first..tokens.len() {
            let rest_is_bait = tokens[..first]
                .iter()
                .chain(&tokens[last + 1..])
                .all(is_bait);
            let has_rest = first > 0 || last + 1 < tokens.len();
            if !rest_is_bait || !has_rest {
                continue;
            }
            let span = &candidate[tokens[first].0..tokens[last].1];
            if let Some(popular) = dataset.iter().find(|name| *name == span) {
                if best.is_none_or(|best| popular.len() > best.len()) {
                    best = Some(popular);
                }
            }
        }
    }
    best.cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typosquat::dataset::load_popular_packages;

    #[test]
    fn popular_name_with_bait_suffix_is_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(
            combosquat("lodash-utils", &dataset).as_deref(),
            Some("lodash")
        );
    }

    #[test]
    fn real_popular_package_is_not_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(combosquat("react-dom", &dataset), None);
    }

    #[test]
    fn multi_token_popular_name_with_bait_is_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(
            combosquat("cross-spawn-secure", &dataset).as_deref(),
            Some("cross-spawn")
        );
    }

    #[test]
    fn bait_prefix_and_other_separators_are_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(
            combosquat("secure-react", &dataset).as_deref(),
            Some("react")
        );
        assert_eq!(
            combosquat("express_api", &dataset).as_deref(),
            Some("express")
        );
    }

    #[test]
    fn non_bait_extra_word_is_not_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(combosquat("lodash-banana", &dataset), None);
    }

    #[test]
    fn bait_words_alone_are_not_flagged() {
        let dataset = load_popular_packages();
        assert_eq!(combosquat("secure-utils", &dataset), None);
    }
}
