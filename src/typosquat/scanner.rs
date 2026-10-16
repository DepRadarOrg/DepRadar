//! Combined typosquat scoring: the single entry point that ties the
//! edit-distance signal ([`super::distance`]) and the homoglyph signal
//! ([`super::homoglyph`]) together into one answer.
//!
//! Each underlying signal stays independent and boolean/threshold-free on its
//! own — this module is where "is this name suspicious, and why" finally gets
//! decided. [`score_package`] runs a candidate against both signals and
//! [`ScanResult`] reports which (if either, or both) fired.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::dataset::load_popular_packages;
use super::distance::nearest_match;
use super::homoglyph::homoglyph_impersonation;

/// Maximum edit distance from a popular package name that counts as
/// "suspiciously close" rather than coincidental.
///
/// A distance of 0 is an exact match — the real package, not a typosquat —
/// and is never flagged regardless of this threshold. 1-2 covers the classic
/// single-keystroke typo (`expresss`, `requezts`); anything farther is
/// treated as unrelated.
pub const DISTANCE_THRESHOLD: usize = 2;

/// Result of scoring a candidate package name against both typosquat
/// signals.
///
/// Serializes to JSON with snake_case keys for the CLI and report formats.
/// `distance_match` becomes a `[name, distance]` array and unset signals
/// become `null`, e.g.
/// `{"candidate":"expresss","distance_match":["express",1],"homoglyph_match":null}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScanResult {
    /// The name that was scored, as given by the caller.
    pub candidate: String,
    /// Set when [`nearest_match`] found a popular package within
    /// [`DISTANCE_THRESHOLD`] edits (and not an exact match): the matched
    /// name and the edit distance to it.
    pub distance_match: Option<(String, usize)>,
    /// Set when [`homoglyph_impersonation`] found that the candidate, once
    /// lookalike characters are folded back to Latin, is a popular package:
    /// the matched name.
    pub homoglyph_match: Option<String>,
}

impl ScanResult {
    /// True if either signal fired.
    pub fn is_likely_typosquat(&self) -> bool {
        self.distance_match.is_some() || self.homoglyph_match.is_some()
    }
}

impl fmt::Display for ScanResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.is_likely_typosquat() {
            return write!(f, "{}: no typosquat signal detected", self.candidate);
        }

        write!(f, "{}: likely typosquat", self.candidate)?;
        if let Some((name, distance)) = &self.distance_match {
            write!(
                f,
                "\n  - edit distance {distance} from popular package \"{name}\""
            )?;
        }
        if let Some(name) = &self.homoglyph_match {
            write!(
                f,
                "\n  - homoglyph impersonation of popular package \"{name}\""
            )?;
        }
        Ok(())
    }
}

/// Scores `name` against the bundled popular-package dataset using both
/// typosquat signals and returns why (if at all) it looks suspicious.
///
/// Loads [`load_popular_packages`] internally, so callers only need a name.
pub fn score_package(name: &str) -> ScanResult {
    let dataset = load_popular_packages();

    let (nearest_name, distance) = nearest_match(name, &dataset);
    let distance_match =
        (distance > 0 && distance <= DISTANCE_THRESHOLD).then_some((nearest_name, distance));

    let homoglyph_match = homoglyph_impersonation(name, &dataset);

    ScanResult {
        candidate: name.to_string(),
        distance_match,
        homoglyph_match,
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
            r#"{"candidate":"expresss","distance_match":["express",1],"homoglyph_match":null}"#
        );
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
    fn display_reports_no_signal_for_clean_name() {
        let result = score_package("react");
        assert_eq!(result.to_string(), "react: no typosquat signal detected");
    }
}
