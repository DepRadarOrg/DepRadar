//! Edit-distance primitives for typosquat detection.
//!
//! A typosquatted package name is, almost by definition, a small number of
//! keystroke edits away from a popular one. [`levenshtein_distance`] measures
//! that gap, [`nearest_match`] finds the closest legitimate name in a
//! reference set, and [`nearest_matches`] returns the closest `n`. No scoring or thresholding lives here — callers decide what
//! counts as "suspiciously close".
//!
//! [`edit_script`] goes one step further and explains *how* two names differ:
//! it returns the concrete sequence of [`EditOp`]s (insert, delete, substitute,
//! or transposition, each with a position) that turns one name into the other.
//! This lets a reviewer see `expresss` = `express` + inserted `s` at 7 rather
//! than just "edit distance 1".

/// A single edit operation turning one string into another.
///
/// Positions are zero-based character (Unicode scalar value) indices. For
/// [`EditOp::Insert`] the position is the index in the *target* string where
/// the character was inserted; for [`EditOp::Delete`] it is the index in the
/// *source* string of the removed character; for [`EditOp::Substitute`] it is
/// the index in both strings where one character replaced another; and for
/// [`EditOp::Transpose`] it is the index of the first of the two swapped
/// characters in the source string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// A character was inserted at `pos` in the target.
    Insert { pos: usize, ch: char },
    /// The character `ch` at `pos` in the source was deleted.
    Delete { pos: usize, ch: char },
    /// The character at `pos` was replaced by `to`.
    Substitute { pos: usize, from: char, to: char },
    /// Two adjacent characters at `pos` and `pos + 1` were swapped.
    Transpose { pos: usize, first: char, second: char },
}

impl EditOp {
    /// The position associated with this operation.
    pub fn pos(&self) -> usize {
        match *self {
            EditOp::Insert { pos, .. }
            | EditOp::Delete { pos, .. }
            | EditOp::Substitute { pos, .. }
            | EditOp::Transpose { pos, .. } => pos,
        }
    }
}

/// Standard Levenshtein edit distance between `a` and `b`: the minimum number
/// of single-character insertions, deletions, or substitutions that turns one
/// string into the other.
///
/// Operates on Unicode scalar values (`char`), so multi-byte characters count
/// as one edit. Runs in `O(a.len() * b.len())` time and `O(b.len())` extra
/// space using the usual two-row dynamic-programming table.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    // `prev[j]` holds the distance between the processed prefix of `a` and the
    // first `j` characters of `b`. It starts as the distance from the empty
    // prefix of `a`, which is just `j` deletions.
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];

    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let substitution_cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1) // delete a[i]
                .min(curr[j] + 1) // insert b[j]
                .min(prev[j] + substitution_cost); // substitute / match
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

/// Computes the edit script that turns `from` into `to`, using the same
/// Levenshtein model as [`levenshtein_distance`] (insertions, deletions and
/// substitutions), plus a transposition shortcut: when swapping two adjacent
/// characters is cheaper than the plain Levenshtein path, a single
/// [`EditOp::Transpose`] is emitted instead of a delete + insert pair.
///
/// The returned operations are ordered by position. The number of operations
/// equals the distance reported by [`edit_distance_with_transpositions`], which
/// may be smaller than [`levenshtein_distance`] when a transposition applies.
///
/// Positions are zero-based character indices; see [`EditOp`] for the exact
/// meaning per variant.
pub fn edit_script(from: &str, to: &str) -> Vec<EditOp> {
    let a: Vec<char> = from.chars().collect();
    let b: Vec<char> = to.chars().collect();
    let n = a.len();
    let m = b.len();

    // `dp[i][j]` is the edit distance (with transpositions) between the first
    // `i` chars of `a` and the first `j` chars of `b`.
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=m {
        dp[0][j] = j;
    }

    for i in 1..=n {
        for j in 1..=m {
            let substitution_cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let mut best = (dp[i - 1][j] + 1) // delete a[i-1]
                .min(dp[i][j - 1] + 1) // insert b[j-1]
                .min(dp[i - 1][j - 1] + substitution_cost); // substitute / match
            if i >= 2 && j >= 2 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(dp[i - 2][j - 2] + 1); // transpose
            }
            dp[i][j] = best;
        }
    }

    // Walk back through the table, collecting operations in reverse.
    let mut ops: Vec<EditOp> = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && a[i - 1] == b[j - 1]
            && dp[i][j] == dp[i - 1][j - 1]
        {
            // Match: no operation.
            i -= 1;
            j -= 1;
        } else if i >= 2
            && j >= 2
            && a[i - 1] == b[j - 2]
            && a[i - 2] == b[j - 1]
            && dp[i][j] == dp[i - 2][j - 2] + 1
        {
            ops.push(EditOp::Transpose {
                pos: i - 2,
                first: a[i - 2],
                second: a[i - 1],
            });
            i -= 2;
            j -= 2;
        } else if i > 0 && j > 0 && dp[i][j] == dp[i - 1][j - 1] + 1 {
            ops.push(EditOp::Substitute {
                pos: i - 1,
                from: a[i - 1],
                to: b[j - 1],
            });
            i -= 1;
            j -= 1;
        } else if i > 0 && dp[i][j] == dp[i - 1][j] + 1 {
            ops.push(EditOp::Delete {
                pos: i - 1,
                ch: a[i - 1],
            });
            i -= 1;
        } else {
            // Must be an insertion of b[j-1].
            ops.push(EditOp::Insert {
                pos: j - 1,
                ch: b[j - 1],
            });
            j -= 1;
        }
    }

    ops.reverse();
    ops
}

/// Edit distance between `a` and `b` allowing insertions, deletions,
/// substitutions and adjacent transpositions (the Damerau–Levenshtein
/// "optimal string alignment" variant).
///
/// This is never larger than [`levenshtein_distance`] and is the distance
/// reported alongside [`edit_script`].
pub fn edit_distance_with_transpositions(a: &str, b: &str) -> usize {
    edit_script(a, b).len()
}

/// Returns up to `n` names from `dataset` closest to `candidate`, each with
/// its [`levenshtein_distance`], sorted by distance and then by dataset order.
///
/// `n == 0` or an empty `dataset` yields an empty vector; `n` larger than the
/// dataset returns every entry. Ties keep the order the names appear in
/// `dataset`, so the first element always agrees with [`nearest_match`].
pub fn nearest_matches(candidate: &str, dataset: &[String], n: usize) -> Vec<(String, usize)> {
    if n == 0 {
        return Vec::new();
    }

    let mut scored: Vec<(&String, usize)> = dataset
        .iter()
        .map(|name| (name, levenshtein_distance(candidate, name)))
        .collect();
    // Stable sort: equal distances stay in dataset order.
    scored.sort_by_key(|&(_, distance)| distance);
    scored.truncate(n);

    scored
        .into_iter()
        .map(|(name, distance)| (name.clone(), distance))
        .collect()
}

/// Returns the name in `dataset` with the smallest [`levenshtein_distance`] to
/// `candidate`, together with that distance.
///
/// A thin wrapper over [`nearest_matches`] with `n = 1`. On a tie the earliest
/// entry in `dataset` wins. An empty `dataset` yields
/// `(String::new(), usize::MAX)`.
pub fn nearest_match(candidate: &str, dataset: &[String]) -> (String, usize) {
    nearest_matches(candidate, dataset, 1)
        .into_iter()
        .next()
        .unwrap_or((String::new(), usize::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typosquat::dataset::load_popular_packages;

    #[test]
    fn distance_between_identical_strings_is_zero() {
        assert_eq!(levenshtein_distance("express", "express"), 0);
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    #[test]
    fn distance_handles_empty_operand() {
        assert_eq!(levenshtein_distance("", "react"), 5);
        assert_eq!(levenshtein_distance("react", ""), 5);
    }

    #[test]
    fn one_character_off_typosquat_has_distance_one() {
        // dropped 's' — the classic install-time typo
        assert_eq!(levenshtein_distance("expres", "express"), 1);
        // extra 's'
        assert_eq!(levenshtein_distance("expresss", "express"), 1);
        // single substitution
        assert_eq!(levenshtein_distance("requezts", "requests"), 1);
    }

    #[test]
    fn distance_counts_transposition_as_two_edits() {
        // plain Levenshtein has no transposition op: swapping two chars is one
        // deletion + one insertion (or two substitutions).
        assert_eq!(levenshtein_distance("recat", "react"), 2);
    }

    #[test]
    fn nearest_match_identifies_misspelled_popular_package() {
        let dataset = load_popular_packages();

        // deliberately misspelled 'express' (npm) — one inserted character
        let (name, distance) = nearest_match("expresss", &dataset);
        assert_eq!(name, "express");
        assert_eq!(distance, 1);

        // deliberately misspelled 'requests' (PyPI) — one substituted character
        let (name, distance) = nearest_match("requezts", &dataset);
        assert_eq!(name, "requests");
        assert_eq!(distance, 1);
    }

    #[test]
    fn edit_script_reports_insertion() {
        // `expresss` = `express` + inserted `s` at position 7.
        let ops = edit_script("express", "expresss");
        assert_eq!(
            ops,
            vec![EditOp::Insert {
                pos: 7,
                ch: 's'
            }]
        );
        assert_eq!(edit_distance_with_transpositions("express", "expresss"), 1);
    }

    #[test]
    fn edit_script_reports_deletion() {
        // `expres` = `express` with the `s` at position 6 deleted.
        let ops = edit_script("express", "expres");
        assert_eq!(
            ops,
            vec![EditOp::Delete {
                pos: 6,
                ch: 's'
            }]
        );
        assert_eq!(edit_distance_with_transpositions("express", "expres"), 1);
    }

    #[test]
    fn edit_script_reports_substitution() {
        // `requezts` vs `requests`: `z` at position 5 replaces `s`.
        let ops = edit_script("requests", "requezts");
        assert_eq!(
            ops,
            vec![EditOp::Substitute {
                pos: 5,
                from: 's',
                to: 'z'
            }]
        );
        assert_eq!(
            edit_distance_with_transpositions("requests", "requezts"),
            1
        );
    }

    #[test]
    fn edit_script_reports_transposition() {
        // `recat` vs `react`: `c` and `a` swapped at positions 2 and 3.
        let ops = edit_script("react", "recat");
        assert_eq!(
            ops,
            vec![EditOp::Transpose {
                pos: 2,
                first: 'a',
                second: 'c'
            }]
        );
        // Transposition is a single edit, unlike plain Levenshtein (2).
        assert_eq!(edit_distance_with_transpositions("react", "recat"), 1);
        assert_eq!(levenshtein_distance("react", "recat"), 2);
    }

    #[test]
    fn edit_script_of_identical_strings_is_empty() {
        assert!(edit_script("express", "express").is_empty());
        assert!(edit_script("", "").is_empty());
    }

    #[test]
    fn edit_script_handles_empty_operands() {
        assert_eq!(
            edit_script("", "ab"),
            vec![
                EditOp::Insert { pos: 0, ch: 'a' },
                EditOp::Insert { pos: 1, ch: 'b' },
            ]
        );
        assert_eq!(
            edit_script("ab", ""),
            vec![
                EditOp::Delete { pos: 0, ch: 'a' },
                EditOp::Delete { pos: 1, ch: 'b' },
            ]
        );
    }

    #[test]
    fn edit_script_length_matches_distance() {
        for (a, b) in [
            ("express", "expresss"),
            ("express", "expres"),
            ("requests", "requezts"),
            ("react", "recat"),
            ("", "react"),
            ("react", ""),
        ] {
            assert_eq!(edit_script(a, b).len(), edit_distance_with_transpositions(a, b));
        }
    }

    #[test]
    fn nearest_match_returns_exact_entry_with_distance_zero() {
        let dataset = load_popular_packages();
        let (name, distance) = nearest_match("react", &dataset);
        assert_eq!(name, "react");
        assert_eq!(distance, 0);
    }

    #[test]
    fn nearest_match_on_empty_dataset_is_sentinel() {
        let (name, distance) = nearest_match("anything", &[]);
        assert_eq!(name, "");
        assert_eq!(distance, usize::MAX);
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        fn char_len(s: &str) -> usize {
            s.chars().count()
        }

        proptest! {
            #[test]
            fn distance_is_symmetric(a in any::<String>(), b in any::<String>()) {
                prop_assert_eq!(levenshtein_distance(&a, &b), levenshtein_distance(&b, &a));
            }

            #[test]
            fn distance_to_self_is_zero(a in any::<String>()) {
                prop_assert_eq!(levenshtein_distance(&a, &a), 0);
            }

            #[test]
            fn distance_satisfies_triangle_inequality(
                a in any::<String>(),
                b in any::<String>(),
                c in any::<String>(),
            ) {
                prop_assert!(
                    levenshtein_distance(&a, &c)
                        <= levenshtein_distance(&a, &b) + levenshtein_distance(&b, &c)
                );
            }

            #[test]
            fn distance_is_at_most_the_longer_length(a in any::<String>(), b in any::<String>()) {
                prop_assert!(levenshtein_distance(&a, &b) <= char_len(&a).max(char_len(&b)));
            }

            #[test]
            fn edit_script_length_matches_its_distance(a in any::<String>(), b in any::<String>()) {
                prop_assert_eq!(
                    edit_script(&a, &b).len(),
                    edit_distance_with_transpositions(&a, &b)
                );
            }

            #[test]
            fn transposition_distance_is_at_most_levenshtein(
                a in any::<String>(),
                b in any::<String>(),
            ) {
                prop_assert!(
                    edit_distance_with_transpositions(&a, &b) <= levenshtein_distance(&a, &b)
                );
            }
        }
    }
}
