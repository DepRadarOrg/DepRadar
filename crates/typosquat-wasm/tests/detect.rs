use typosquat_wasm::{detect, scan};

#[test]
fn flags_a_distance_typosquat() {
    let detection = detect("expresss");
    assert!(detection.flagged);
    assert!(detection.confidence > 0);
    assert_eq!(detection.signals[0].kind, "distance");
    assert!(detection.signals[0].detail.contains("\"express\""));
}

#[test]
fn popular_package_has_no_signals() {
    let detection = detect("express");
    assert!(!detection.flagged);
    assert!(detection.signals.is_empty());
}

#[test]
fn scan_returns_json() {
    let json: serde_json::Value = serde_json::from_str(&scan("lodash-utils")).unwrap();
    assert_eq!(json["candidate"], "lodash-utils");
    assert_eq!(json["flagged"], true);
    assert!(json["signals"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["kind"] == "combosquat"));
}
