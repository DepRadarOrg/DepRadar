//! The typosquat scorer from the root crate, compiled to WebAssembly so the
//! dashboard can show why a bounty's package looks suspicious.
//!
//! The root `deprader` crate pulls in networking (`reqwest`) and filesystem
//! dependencies that don't build for `wasm32-unknown-unknown`, so this crate
//! mounts only its self-contained `typosquat` module (std + serde, with the
//! popular-package datasets embedded via `include_str!`).
//!
//! Build for the dashboard with:
//!
//! ```bash
//! wasm-pack build crates/typosquat-wasm --target web --release \
//!   --out-dir ../../frontend/public/wasm --out-name typosquat
//! ```

use serde::Serialize;
use wasm_bindgen::prelude::wasm_bindgen;

#[path = "../../../src/typosquat/mod.rs"]
pub mod typosquat;

use typosquat::scanner::{score_package, ScanResult};

/// One typosquat signal that fired for a candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Signal {
    /// Stable machine name of the signal, e.g. `"distance"`.
    pub kind: &'static str,
    /// Human-readable explanation, e.g. `edit distance 1 from "express"`.
    pub detail: String,
}

/// What the dashboard renders for a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Detection {
    pub candidate: String,
    pub flagged: bool,
    pub confidence: u8,
    pub signals: Vec<Signal>,
}

impl From<ScanResult> for Detection {
    fn from(result: ScanResult) -> Self {
        let mut signals = Vec::new();
        if let Some((name, distance)) = &result.distance_match {
            let rank = result
                .rank
                .map(|rank| format!(" (rank {rank})"))
                .unwrap_or_default();
            signals.push(Signal {
                kind: "distance",
                detail: format!("edit distance {distance} from popular package \"{name}\"{rank}"),
            });
        }
        if result.keyboard_typo {
            signals.push(Signal {
                kind: "keyboard",
                detail: "a single adjacent-key substitution on a QWERTY keyboard".to_string(),
            });
        }
        if let Some(name) = &result.homoglyph_match {
            signals.push(Signal {
                kind: "homoglyph",
                detail: format!("uses lookalike characters to imitate \"{name}\""),
            });
        }
        if let Some(name) = &result.digit_swap_match {
            signals.push(Signal {
                kind: "digit_swap",
                detail: format!("swaps letters for lookalike digits to imitate \"{name}\""),
            });
        }
        if let Some(name) = &result.scope_confusion {
            signals.push(Signal {
                kind: "scope_confusion",
                detail: format!("misspells or drops the scope of \"{name}\""),
            });
        }
        if let Some(name) = &result.combosquat {
            signals.push(Signal {
                kind: "combosquat",
                detail: format!("attaches bait words to \"{name}\""),
            });
        }
        Detection {
            flagged: result.is_likely_typosquat(),
            candidate: result.candidate,
            confidence: result.confidence,
            signals,
        }
    }
}

/// Scores `name` and returns its [`Detection`].
pub fn detect(name: &str) -> Detection {
    score_package(name).into()
}

/// Scores `name` and returns its [`Detection`] as JSON:
/// `{"candidate":..,"flagged":..,"confidence":..,"signals":[{"kind":..,"detail":..}]}`.
#[wasm_bindgen]
pub fn scan(name: &str) -> String {
    serde_json::to_string(&detect(name)).expect("Detection always serializes")
}
