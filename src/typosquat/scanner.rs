//! Combined typosquat scoring: the single entry point that ties the
//! edit-distance signal ([`super::distance`]) and the homoglyph signals
//! ([`super::homoglyph`]) together into one answer.
//!
//! Each underlying signal stays independent and boolean/threshold-free on its
//! own — this module is where "is this name suspicious, and why" finally gets
//! decided. [`score_package`] runs a candidate against every signal and
//! [`ScanResult`] reports which fired and how confident the overall verdict
//! is.

use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::bktree::BkTree;
use super::combosquat::combosquat;
use super::dataset::{popular_packages, popularity_rank, rank};
use super::distance::{edit_script, EditOp};
use super::homoglyph::{digit_swap_impersonation, homoglyph_impersonation};
use super::keyboard::keyboard_typo;
use super::normalize::normalize_name;
use super::scope::scope_confusion;

/// Maximum edit distance from a popular package name that counts as
/// "suspiciously close" rather than coincidental, scaled by the length (in
/// characters) of the candidate name.
///
/// A fixed threshold is far too loose for short names: `ms`, `qs` and `ws`
/// are all within two edits of each other, so a single cut-off would flag
/// most two- and three-letter packages. Short names get no tolerance, medium
/// names one edit, and longer names two:
///
/// | Candidate length | Threshold | Example                            |
/// |------------------|-----------|------------------------------------|
/// | 0-3              | 0         | `ms` vs `qs` is not flagged        |
/// | 4-6              | 1         | `reakt` -> `react` is flagged      |
/// | 7+               | 2         | `expresss` -> `express` is flagged |
///
/// A distance of 0 is an exact match — the real package, not a typosquat —
/// and is never flagged regardless of this threshold.
pub fn threshold_for(len: usize) -> usize {
    match len {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    }
}

/// Result of scoring a candidate package name against the typosquat
/// signals.
///
/// Serializes to JSON with snake_case keys for the CLI and report formats.
/// `distance_match` becomes a `[name, distance]` array, `edit_script` an
/// array of `{"op":..,"position":..,"char":..}` objects, `rank` a number or
/// `null`, and unset signals become `null`, e.g.
/// `{"candidate":"expresss","distance_match":["express",1],`
/// `"edit_script":[{"op":"insert","position":7,"char":"s"}],`
/// `"rank":111,"homoglyph_match":null,"digit_swap_match":null,`
/// `"keyboard_typo":false,"scope_confusion":null,"combosquat":null,`
/// `"confidence":40}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScanResult {
    /// The name that was scored, as given by the caller.
    pub candidate: String,
    /// Set when the nearest popular package is within
    /// [`threshold_for`] edits (and not an exact match): the matched
    /// name and the edit distance to it.
    pub distance_match: Option<(String, usize)>,
    /// The edit operations turning the matched popular package into the
    /// candidate (see [`edit_script`]). Empty when no distance match fired.
    /// Rendered by [`fmt::Display`] so a reviewer can see exactly what
    /// changed, e.g. `expresss` = `express` + inserted `s` at 7.
    #[serde(default)]
    pub edit_script: Vec<EditOp>,
    /// The matched popular package's rank in its dataset (0 is the most
    /// popular), or `None` when no distance match fired. Lower ranks are
    /// more dangerous near-misses and weigh more in [`confidence`].
    #[serde(default)]
    pub rank: Option<usize>,
    /// Set when [`homoglyph_impersonation`] found that the candidate, once
    /// lookalike characters are folded back to Latin, is a popular package:
    /// the matched name.
    pub homoglyph_match: Option<String>,
    /// Set when [`digit_swap_impersonation`] found that the candidate, once
    /// lookalike digits (`0`, `1`, `3`, `4`, `5`) are folded back to letters,
    /// is a popular package: the matched name.
    pub digit_swap_match: Option<String>,
    /// True when the distance match is a single substitution of a key
    /// adjacent on a QWERTY keyboard ([`keyboard_typo`]) — the most likely
    /// accidental install. Reported as an extra reason only; it does not
    /// change whether the candidate is flagged.
    pub keyboard_typo: bool,
    /// Set when [`scope_confusion`] found that the candidate imitates a
    /// popular scoped package by misspelling or dropping its scope
    /// (`@typess/node`, `types-node`): the imitated package.
    pub scope_confusion: Option<String>,
    /// Set when [`combosquat`] found that the candidate is a popular package
    /// name with bait words attached (`lodash-utils`): the popular package.
    pub combosquat: Option<String>,
    /// How likely the candidate is a typosquat, from 0 (no signal) to 100.
    /// See [`confidence`] for the formula.
    pub confidence: u8,
}

impl ScanResult {
    /// True if any signal fired.
    pub fn is_likely_typosquat(&self) -> bool {
        self.distance_match.is_some()
            || self.homoglyph_match.is_some()
            || self.digit_swap_match.is_some()
            || self.scope_confusion.is_some()
            || self.combosquat.is_some()
    }
}

impl fmt::Display for ScanResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.is_likely_typosquat() {
            return write!(f, "{}: no typosquat signal detected", self.candidate);
        }

        write!(
            f,
            "{}: likely typosquat (confidence {}/100)",
            self.candidate, self.confidence
        )?;
        if let Some((name, distance)) = &self.distance_match {
            write!(
                f,
                "\n  - edit distance {distance} from popular package \"{name}\""
            )?;
            if let Some(rank) = self.rank {
                write!(f, " (rank {rank})")?;
            }
            if !self.edit_script.is_empty() {
                write!(f, "\n  - {} = \"{name}\"", self.candidate)?;
                for op in &self.edit_script {
                    write!(f, " + {}", op.describe())?;
                }
            }
            if self.keyboard_typo {
                write!(
                    f,
                    "\n  - adjacent-key typo of \"{name}\" (likely accidental install)"
                )?;
            }
        }
        if let Some(name) = &self.homoglyph_match {
            write!(
                f,
                "\n  - homoglyph impersonation of popular package \"{name}\""
            )?;
        }
        if let Some(name) = &self.digit_swap_match {
            write!(
                f,
                "\n  - digit-for-letter swap impersonating popular package \"{name}\""
            )?;
        }
        if let Some(name) = &self.scope_confusion {
            write!(
                f,
                "\n  - scope confusion with popular scoped package \"{name}\""
            )?;
        }
        if let Some(name) = &self.combosquat {
            write!(
                f,
                "\n  - combosquat of popular package \"{name}\" with bait words"
            )?;
        }
        Ok(())
    }
}

/// Computes the 0–100 confidence score for a set of signals.
///
/// The score is the sum of three parts, and is 0 when no signal fired:
///
/// | Part        | Points                                                        |
/// |-------------|---------------------------------------------------------------|
/// | lookalike   | 50 if a non-distance signal fired (homoglyph, digit swap, scope confusion or combosquat), else 0 |
/// | distance    | `40 - 10 * distance` (30 at distance 1, 20 at 2) if the distance signal fired, else 0 |
/// | popularity  | by the best [`popularity_rank`] among the matched targets: 20 for rank < 10, 15 for < 100, 10 for < 500, 5 otherwise |
///
/// The maximum is 50 + 30 + 20 = 100. Because the lookalike part alone (50)
/// is worth more than the best distance part plus the best popularity part
/// (30 + 20), the ranking *both signals > lookalike only > distance only*
/// holds whatever the target's popularity:
///
/// - lookalike + distance: 75..=100
/// - lookalike only: 55..=70
/// - distance only: 25..=50
///
/// Worked examples against the bundled dataset:
///
/// - `react` → exact match, nothing fires: **0**.
/// - `expresss` → distance 1 from `express` (npm rank 111): 30 + 10 = **40**.
/// - full-width `ｒｅｑｕｅｓｔｓ` → homoglyph of `requests` (PyPI rank 44):
///   50 + 15 = **65**.
/// - `l0dash` → digit swap of `lodash` and distance 1 from it (npm rank
///   607): 50 + 30 + 5 = **85**.
/// - `reаct` with a Cyrillic `а` → homoglyph of `react` and distance 1 from
///   it (npm rank 0): 50 + 30 + 20 = **100**.
pub fn confidence(
    distance_match: Option<&(String, usize)>,
    homoglyph_match: Option<&str>,
    digit_swap_match: Option<&str>,
    scope_confusion: Option<&str>,
    combosquat: Option<&str>,
) -> u8 {
    let lookalike = homoglyph_match
        .or(digit_swap_match)
        .or(scope_confusion)
        .or(combosquat);
    if distance_match.is_none() && lookalike.is_none() {
        return 0;
    }

    let lookalike_points: usize = if lookalike.is_some() { 50 } else { 0 };
    let distance_points =
        distance_match.map_or(0, |(_, distance)| 40usize.saturating_sub(10 * distance));

    let best_rank = distance_match
        .map(|(name, _)| name.as_str())
        .into_iter()
        .chain(homoglyph_match)
        .chain(digit_swap_match)
        .chain(scope_confusion)
        .chain(combosquat)
        .filter_map(popularity_rank)
        .min();
    let popularity_points: usize = match best_rank {
        Some(rank) if rank < 10 => 20,
        Some(rank) if rank < 100 => 15,
        Some(rank) if rank < 500 => 10,
        _ => 5,
    };

    (lookalike_points + distance_points + popularity_points).min(100) as u8
}

/// BK-tree over the bundled dataset, built once on first use.
fn popular_index() -> &'static BkTree<'static> {
    static INDEX: OnceLock<BkTree<'static>> = OnceLock::new();
    INDEX.get_or_init(|| BkTree::new(popular_packages()))
}

/// Scores `name` against the bundled popular-package dataset using every
/// typosquat signal and returns why (if at all) it looks suspicious.
///
/// The dataset and its [`BkTree`] index are built on the first call and
/// reused afterwards, so callers only need a name.
pub fn score_package(name: &str) -> ScanResult {
    let dataset = popular_packages();
    let normalized = normalize_name(name);

    // Results are sorted by distance, then dataset order, so the first hit is
    // what a linear `nearest_match` scan would return. An exact match
    // (distance 0) is the real package and suppresses the signal.
    let threshold = threshold_for(normalized.chars().count());
    let distance_match = popular_index()
        .within(&normalized, threshold)
        .first()
        .filter(|&&(_, distance)| distance > 0)
        .map(|&(nearest, distance)| (nearest.to_string(), distance));
    let edit_script = distance_match
        .as_ref()
        .map(|(nearest, _)| edit_script(nearest, &normalized))
        .unwrap_or_default();
    let rank = distance_match
        .as_ref()
        .and_then(|(nearest, _)| rank(nearest));

    let homoglyph_match = homoglyph_impersonation(&normalized, dataset);
    let digit_swap_match = digit_swap_impersonation(&normalized, dataset);
    let keyboard_typo = distance_match
        .as_ref()
        .is_some_and(|(matched, _)| keyboard_typo(&normalized, matched));
    let scope_confusion = scope_confusion(&normalized, dataset);
    let combosquat = combosquat(&normalized, dataset);
    let confidence = confidence(
        distance_match.as_ref(),
        homoglyph_match.as_deref(),
        digit_swap_match.as_deref(),
        scope_confusion.as_deref(),
        combosquat.as_deref(),
    );

    ScanResult {
        candidate: name.to_string(),
        distance_match,
        edit_script,
        rank,
        homoglyph_match,
        digit_swap_match,
        keyboard_typo,
        scope_confusion,
        combosquat,
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_popular_package_is_not_flagged() {
        let result = score_package("react");
        assert_eq!(result.distance_match, None);
        assert_eq!(result.homoglyph_match, None);
        assert!(!result.is_likely_typosquat());
    }

    #[test]
    fn uppercase_popular_package_is_normalized_to_exact_match() {
        let result = score_package("React");
        assert_eq!(result.candidate, "React");
        assert!(!result.is_likely_typosquat());
    }

    #[test]
    fn unrelated_name_is_not_flagged() {
        // Long and distinctive enough that its nearest dataset neighbour is
        // far past the threshold (verified: distance 24 to the closest
        // entry, npm's `define-data-property`).
        let result = score_package("definitely-not-a-real-package-zzz123");
        assert_eq!(result.distance_match, None);
        assert_eq!(result.homoglyph_match, None);
        assert!(!result.is_likely_typosquat());
    }

    #[test]
    fn distance_only_match_is_flagged_with_reason() {
        // One inserted 's' — no homoglyphs involved.
        let result = score_package("expresss");
        assert_eq!(result.distance_match, Some(("express".to_string(), 1)));
        assert_eq!(result.homoglyph_match, None);
        assert!(result.is_likely_typosquat());
    }

    #[test]
    fn homoglyph_only_match_is_flagged_with_reason() {
        // "requests" (PyPI) written in full-width Latin: identical edit
        // distance behaviour to any other unrelated string, but flagged by
        // the homoglyph signal.
        let fullwidth = "\u{FF52}\u{FF45}\u{FF51}\u{FF55}\u{FF45}\u{FF53}\u{FF54}\u{FF53}";
        let result = score_package(fullwidth);
        assert_eq!(result.distance_match, None);
        assert_eq!(result.homoglyph_match.as_deref(), Some("requests"));
        assert!(result.is_likely_typosquat());
    }

    #[test]
    fn both_signals_can_fire_on_the_same_candidate() {
        // "react" with a Cyrillic 'а' (U+0430): a single-character
        // substitution, so it is both one edit away from "react" *and* a
        // homoglyph impersonation of it.
        let result = score_package("re\u{0430}ct");
        assert_eq!(result.distance_match, Some(("react".to_string(), 1)));
        assert_eq!(result.homoglyph_match.as_deref(), Some("react"));
        assert!(result.is_likely_typosquat());
    }

    #[test]
    fn display_reports_both_reasons_when_both_fire() {
        let result = score_package("re\u{0430}ct");
        let rendered = result.to_string();
        assert!(rendered.contains("likely typosquat"));
        assert!(rendered.contains("edit distance 1 from popular package \"react\""));
        assert!(rendered.contains("homoglyph impersonation of popular package \"react\""));
    }

    #[test]
    fn serializes_to_exact_json_for_distance_match() {
        let result = score_package("expresss");
        let json = serde_json::to_string(&result).expect("ScanResult serializes");
        assert_eq!(
            json,
            r#"{"candidate":"expresss","distance_match":["express",1],"edit_script":[{"op":"insert","position":7,"char":"s"}],"rank":111,"homoglyph_match":null,"digit_swap_match":null,"keyboard_typo":false,"scope_confusion":null,"combosquat":null,"confidence":40}"#
        );
    }

    #[test]
    fn display_renders_inserted_character() {
        let result = score_package("expresss");
        assert_eq!(
            result.edit_script,
            vec![EditOp::Insert {
                position: 7,
                ch: 's'
            }]
        );
        assert!(result
            .to_string()
            .contains("expresss = \"express\" + inserted 's' at 7"));
    }

    #[test]
    fn display_renders_deleted_character() {
        // "express" with the final 's' dropped.
        let result = score_package("expres");
        assert_eq!(result.distance_match, Some(("express".to_string(), 1)));
        assert_eq!(
            result.edit_script,
            vec![EditOp::Delete {
                position: 6,
                ch: 's'
            }]
        );
        assert!(result
            .to_string()
            .contains("expres = \"express\" + deleted 's' at 6"));
    }

    #[test]
    fn display_renders_substituted_character() {
        let result = score_package("reqzests");
        assert_eq!(result.distance_match, Some(("requests".to_string(), 1)));
        assert_eq!(
            result.edit_script,
            vec![EditOp::Substitute {
                position: 3,
                from: 'u',
                to: 'z'
            }]
        );
        assert!(result
            .to_string()
            .contains("reqzests = \"requests\" + substituted 'z' for 'u' at 3"));
    }

    #[test]
    fn display_renders_transposition() {
        // "react" with 'a' and 'c' swapped.
        let result = score_package("recat");
        assert_eq!(result.distance_match, Some(("react".to_string(), 1)));
        assert_eq!(
            result.edit_script,
            vec![EditOp::Transpose { position: 2 }]
        );
        assert!(result
            .to_string()
            .contains("recat = \"react\" + transposed 'a' and 'c' at 2"));
    }

    #[test]
    fn rank_is_reported_and_react_outranks_a_tail_entry() {
        let react = score_package("re\u{0430}ct");
        assert_eq!(react.rank, Some(0));
        let tail = rank("lodash").expect("lodash is in the dataset");
        assert!(tail > 0, "lodash should not be rank 0");
        assert!(react.rank.unwrap() < tail);
    }

    #[test]
    fn rank_is_none_without_a_distance_match() {
        let result = score_package("react");
        assert_eq!(result.rank, None);
        assert!(result.edit_script.is_empty());
    }

    #[test]
    fn json_round_trips_back_to_the_same_result() {
        for name in ["expresss", "re\u{0430}ct", "react"] {
            let result = score_package(name);
            let json = serde_json::to_string(&result).expect("ScanResult serializes");
            let back: ScanResult = serde_json::from_str(&json).expect("ScanResult deserializes");
            assert_eq!(back, result, "round trip changed the result for {name:?}");
        }
    }

    #[test]
    fn adjacent_key_typo_is_reported_as_a_reason() {
        let result = score_package("reqiests");
        assert_eq!(result.distance_match, Some(("requests".to_string(), 1)));
        assert!(result.keyboard_typo);
        assert!(result
            .to_string()
            .contains("adjacent-key typo of \"requests\""));
    }

    #[test]
    fn non_adjacent_substitution_is_not_a_keyboard_typo() {
        let result = score_package("reqzests");
        assert_eq!(result.distance_match, Some(("requests".to_string(), 1)));
        assert!(!result.keyboard_typo);
    }

    #[test]
    fn misspelled_scope_is_flagged_as_scope_confusion() {
        let result = score_package("@typess/node");
        assert_eq!(result.scope_confusion.as_deref(), Some("@types/node"));
        assert!(result.is_likely_typosquat());
        assert!(result
            .to_string()
            .contains("scope confusion with popular scoped package \"@types/node\""));
    }

    #[test]
    fn dropped_scope_is_flagged_as_scope_confusion() {
        let result = score_package("types-node");
        assert_eq!(result.scope_confusion.as_deref(), Some("@types/node"));
    }

    #[test]
    fn legitimate_scoped_package_is_not_flagged() {
        let result = score_package("@types/node");
        assert_eq!(result.scope_confusion, None);
        assert!(!result.is_likely_typosquat());
    }

    #[test]
    fn combosquat_is_flagged_with_reason() {
        let result = score_package("lodash-utils");
        assert_eq!(result.combosquat.as_deref(), Some("lodash"));
        assert!(result.is_likely_typosquat());
        assert!(result
            .to_string()
            .contains("combosquat of popular package \"lodash\""));
    }

    #[test]
    fn real_multi_word_package_is_not_a_combosquat() {
        let result = score_package("react-dom");
        assert_eq!(result.combosquat, None);
    }

    #[test]
    fn display_reports_no_signal_for_clean_name() {
        let result = score_package("react");
        assert_eq!(result.to_string(), "react: no typosquat signal detected");
    }
}
