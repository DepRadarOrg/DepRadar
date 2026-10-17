//! Edit-distance primitives for typosquat detection.
//!
//! A typosquatted package name is, almost by definition, a small number of
//! keystroke edits away from a popular one. [`levenshtein_distance`] measures
//! that gap and [`nearest_match`] finds the closest legitimate name in a
//! reference set. No scoring or thresholding lives here — callers decide what
//! counts as "suspiciously close".

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

/// Returns the name in `dataset` with the smallest [`levenshtein_distance`] to
/// `candidate`, together with that distance.
///
/// On a tie the earliest entry in `dataset` wins. An empty `dataset` yields
/// `(String::new(), usize::MAX)`.
pub fn nearest_match(candidate: &str, dataset: &[String]) -> (String, usize) {
    let mut best: Option<(&String, usize)> = None;

    for name in dataset {
        let distance = levenshtein_distance(candidate, name);
        if best.is_none_or(|(_, best_distance)| distance < best_distance) {
            best = Some((name, distance));
            if distance == 0 {
                break; // nothing can beat an exact match
            }
        }
    }

    match best {
        Some((name, distance)) => (name.clone(), distance),
        None => (String::new(), usize::MAX),
    }
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
        }
    }
}
