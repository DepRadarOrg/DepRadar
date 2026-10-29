//! Heuristics for obfuscated JavaScript.
//!
//! Malware on npm hides its payload in encoded blobs and decodes it at
//! runtime. [`score_javascript`] scores a source file on four independent
//! signals:
//!
//! | Signal | Weight | Notes |
//! | --- | --- | --- |
//! | Base64 run of 200+ characters | 3 | counted inside string literals |
//! | Hex run of 200+ characters | 4 | counted inside string literals |
//! | `eval(` / `new Function(` / `atob(` | 2 / 3 / 2 | in code, not strings or comments |
//! | Shannon entropy ≥ 4.8 on a 128+ character string | 2 each, max 4 | |
//! | Source line longer than 2,000 characters | 1 | minified files get this alone |
//!
//! A file is reported as an [`Obfuscation`](ObfuscationFinding) finding at or
//! above [`OBFUSCATION_THRESHOLD`] (4). The threshold is deliberately above
//! the single long-line weight: ordinary minified bundles are not flagged.
//! Scores at or above [`HIGH_SEVERITY_THRESHOLD`] (8) are rated high,
//! otherwise medium. This is a review prompt, not a verdict.

use std::path::Path;

use crate::cve::severity::Severity;

use super::{collect_files, BehavioralFinding};

/// Score at which a file is reported.
pub const OBFUSCATION_THRESHOLD: u32 = 4;
/// Score at which the finding is rated high rather than medium.
pub const HIGH_SEVERITY_THRESHOLD: u32 = 8;
/// Files larger than this are skipped by [`scan_tree`].
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
/// Entropy at or above this on a long literal is suspicious.
pub const HIGH_ENTROPY: f64 = 4.8;
/// Minimum literal length for the entropy signal.
pub const ENTROPY_MIN_LEN: usize = 128;
/// Minimum literal run for an encoded blob.
pub const BLOB_MIN_LEN: usize = 200;
/// Line length that counts as "very long" (minified).
pub const LONG_LINE: usize = 2_000;

const WEIGHT_BASE64_BLOB: u32 = 3;
const WEIGHT_HEX_BLOB: u32 = 4;
const WEIGHT_EVAL: u32 = 2;
const WEIGHT_FUNCTION: u32 = 3;
const WEIGHT_ATOB: u32 = 2;
const WEIGHT_ENTROPY: u32 = 2;
const WEIGHT_LONG_LINE: u32 = 1;

/// Per-file score plus the evidence behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct ObfuscationScore {
    pub score: u32,
    pub base64_blobs: usize,
    pub hex_blobs: usize,
    pub entropy_strings: usize,
    pub eval_calls: usize,
    pub function_constructors: usize,
    pub atob_calls: usize,
    pub long_lines: usize,
    /// Human-readable reasons, one per signal that fired.
    pub reasons: Vec<String>,
}

impl ObfuscationScore {
    /// True when the score reaches [`OBFUSCATION_THRESHOLD`].
    pub fn is_obfuscated(&self) -> bool {
        self.score >= OBFUSCATION_THRESHOLD
    }

    /// Severity for a reported file.
    pub fn severity(&self) -> Severity {
        if self.score >= HIGH_SEVERITY_THRESHOLD {
            Severity::High
        } else {
            Severity::Medium
        }
    }

    fn add(&mut self, weight: u32, reason: String) {
        self.score = self.score.saturating_add(weight);
        self.reasons.push(reason);
    }
}

/// The finding produced for one obfuscated file.
#[derive(Debug, Clone)]
pub struct ObfuscationFinding {
    pub file: String,
    pub score: ObfuscationScore,
}

impl ObfuscationFinding {
    /// Renders the finding as the shared behavioral type.
    pub fn to_finding(&self) -> BehavioralFinding {
        BehavioralFinding {
            file: self.file.clone(),
            line: None,
            severity: self.score.severity(),
            message: format!(
                "obfuscated JavaScript (score {}/{}): {}",
                self.score.score,
                OBFUSCATION_THRESHOLD,
                self.score.reasons.join("; ")
            ),
        }
    }
}

/// Scores one JavaScript source file. The score is deterministic and never
/// executes the code.
pub fn score_javascript(source: &str) -> ObfuscationScore {
    let mut result = ObfuscationScore {
        score: 0,
        base64_blobs: 0,
        hex_blobs: 0,
        entropy_strings: 0,
        eval_calls: 0,
        function_constructors: 0,
        atob_calls: 0,
        long_lines: 0,
        reasons: Vec::new(),
    };

    let literals = string_literals(source);
    for literal in &literals {
        for run in encoded_runs(literal) {
            match run.kind {
                BlobKind::Base64 => result.base64_blobs += 1,
                BlobKind::Hex => result.hex_blobs += 1,
            }
        }
        if literal.len() >= ENTROPY_MIN_LEN && shannon_entropy(literal) >= HIGH_ENTROPY {
            result.entropy_strings += 1;
        }
    }

    if result.base64_blobs > 0 {
        result.add(
            WEIGHT_BASE64_BLOB,
            format!(
                "{} base64 blob(s) over {BLOB_MIN_LEN} chars",
                result.base64_blobs
            ),
        );
    }
    if result.hex_blobs > 0 {
        result.add(
            WEIGHT_HEX_BLOB,
            format!("{} hex blob(s) over {BLOB_MIN_LEN} chars", result.hex_blobs),
        );
    }
    if result.entropy_strings > 0 {
        let weight =
            WEIGHT_ENTROPY * u32::try_from(result.entropy_strings.min(2)).unwrap_or_default();
        result.add(
            weight,
            format!(
                "{} high-entropy string literal(s) (entropy >= {HIGH_ENTROPY})",
                result.entropy_strings
            ),
        );
    }

    let code = mask_comments_and_strings(source);
    result.eval_calls = count_bare_calls(&code, "eval");
    result.function_constructors = count_function_constructors(&code);
    result.atob_calls = count_bare_calls(&code, "atob");
    if result.eval_calls > 0 {
        result.add(
            WEIGHT_EVAL,
            format!("eval( used {} time(s)", result.eval_calls),
        );
    }
    if result.function_constructors > 0 {
        result.add(
            WEIGHT_FUNCTION,
            format!(
                "new Function( used {} time(s)",
                result.function_constructors
            ),
        );
    }
    if result.atob_calls > 0 {
        result.add(
            WEIGHT_ATOB,
            format!("atob( used {} time(s)", result.atob_calls),
        );
    }

    result.long_lines = source.lines().filter(|line| line.len() > LONG_LINE).count();
    if result.long_lines > 0 {
        result.add(
            WEIGHT_LONG_LINE,
            format!("{} line(s) over {LONG_LINE} chars", result.long_lines),
        );
    }

    result
}

/// Alias for [`score_javascript`], matching the module's noun-first style.
pub fn obfuscation_score(source: &str) -> ObfuscationScore {
    score_javascript(source)
}

/// Scores `source` as `file` and returns a finding when it crosses the
/// threshold.
pub fn scan_obfuscation(file: &str, source: &str) -> Option<BehavioralFinding> {
    let finding = ObfuscationFinding {
        file: file.to_string(),
        score: score_javascript(source),
    };
    finding.score.is_obfuscated().then(|| finding.to_finding())
}

/// Alias for [`scan_obfuscation`].
pub fn scan_javascript(file: &str, source: &str) -> Option<BehavioralFinding> {
    scan_obfuscation(file, source)
}

/// Recursively scans `root` for `.js`, `.mjs` and `.cjs` files, returning a
/// finding per file at or above the threshold, sorted by path.
///
/// Files over [`MAX_FILE_BYTES`], non-UTF-8 files and unreadable files are
/// skipped: one odd file shouldn't abort a package scan.
pub fn scan_tree(root: &Path) -> std::io::Result<Vec<BehavioralFinding>> {
    let mut findings = Vec::new();
    for file in collect_files(root)? {
        let is_js = [".js", ".mjs", ".cjs"]
            .iter()
            .any(|ext| file.ends_with(ext));
        if !is_js {
            continue;
        }
        let path = root.join(&file);
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(finding) = scan_obfuscation(&file, &source) {
            findings.push(finding);
        }
    }
    Ok(findings)
}

/// Shannon entropy in bits per byte.
pub fn shannon_entropy(input: &str) -> f64 {
    if input.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for byte in input.bytes() {
        counts[byte as usize] += 1;
    }
    let len = input.len() as f64;
    counts
        .iter()
        .filter(|count| **count > 0)
        .map(|count| {
            let p = *count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlobKind {
    Base64,
    Hex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EncodedRun {
    kind: BlobKind,
    len: usize,
}

/// Finds maximal base64/hex runs of at least [`BLOB_MIN_LEN`] characters.
/// Base64 runs need three character classes so a long identifier isn't a
/// blob; hex runs need both a digit and an `a`–`f` letter.
fn encoded_runs(input: &str) -> Vec<EncodedRun> {
    let bytes = input.as_bytes();
    let mut runs = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let Some((kind, len)) = run_at(bytes, start) else {
            start += 1;
            continue;
        };
        if len >= BLOB_MIN_LEN {
            runs.push(EncodedRun { kind, len });
        }
        start += len;
    }
    runs
}

fn run_at(bytes: &[u8], start: usize) -> Option<(BlobKind, usize)> {
    if !is_base64_byte(bytes[start]) {
        return None;
    }
    let mut end = start;
    while end < bytes.len() && is_base64_byte(bytes[end]) {
        end += 1;
    }
    let run = &bytes[start..end];
    if run.iter().all(|byte| is_hex_byte(*byte)) {
        let has_digit = run.iter().any(u8::is_ascii_digit);
        let has_hex_letter = run
            .iter()
            .any(|byte| matches!(byte, b'a'..=b'f' | b'A'..=b'F'));
        if has_digit && has_hex_letter {
            return Some((BlobKind::Hex, run.len()));
        }
    }
    let classes = [
        run.iter().any(u8::is_ascii_uppercase),
        run.iter().any(u8::is_ascii_lowercase),
        run.iter().any(u8::is_ascii_digit),
        run.iter().any(|byte| matches!(byte, b'+' | b'/' | b'=')),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if classes >= 3 {
        Some((BlobKind::Base64, run.len()))
    } else {
        None
    }
}

fn is_base64_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
}

fn is_hex_byte(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

/// Contents of `'...'`, `"..."` and `` `...` `` literals, escapes preserved.
fn string_literals(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let quote = bytes[i];
        if !matches!(quote, b'\'' | b'"' | b'`') {
            i += 1;
            continue;
        }
        let start = i + 1;
        i = start;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                byte if byte == quote => break,
                _ => i += 1,
            }
        }
        if i <= bytes.len() {
            literals.push(&source[start..i.min(bytes.len())]);
        }
        i = i.saturating_add(1);
    }
    literals
}

/// Replaces comments and the contents of string/template literals with
/// spaces, preserving newlines so line numbers and code markers stay intact.
fn mask_comments_and_strings(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = vec![b' '; bytes.len()];
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                out[i] = b'\n';
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() {
                    if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        i += 2;
                        break;
                    }
                    if bytes[i] == b'\n' {
                        out[i] = b'\n';
                    }
                    i += 1;
                }
            }
            quote @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    if bytes[i] == b'\n' {
                        out[i] = b'\n';
                    }
                    i += 1;
                }
            }
            byte => {
                out[i] = byte;
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Counts `name(` where `name` is a standalone identifier.
fn count_bare_calls(code: &str, name: &str) -> usize {
    let bytes = code.as_bytes();
    let needle = name.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if bytes[i..].starts_with(needle) {
            let before_ok = i == 0 || !is_identifier_byte(bytes[i - 1]);
            let after = i + needle.len();
            let after_ok = after < bytes.len() && !is_identifier_byte(bytes[after]);
            let call = bytes[after..]
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace())
                == Some(b'(');
            if before_ok && after_ok && call {
                count += 1;
                i = after;
                continue;
            }
        }
        i += 1;
    }
    count
}

/// Counts `new Function(` (whitespace tolerated around `Function`).
fn count_function_constructors(code: &str) -> usize {
    let bytes = code.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"new") {
            let before_ok = i == 0 || !is_identifier_byte(bytes[i - 1]);
            let mut j = i + 3;
            if before_ok && j < bytes.len() && bytes[j].is_ascii_whitespace() {
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if bytes[j..].starts_with(b"Function") {
                    let after = j + "Function".len();
                    let boundary = after == bytes.len() || !is_identifier_byte(bytes[after]);
                    let call = bytes[after..]
                        .iter()
                        .copied()
                        .find(|byte| !byte.is_ascii_whitespace())
                        == Some(b'(');
                    if boundary && call {
                        count += 1;
                        i = after;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    count
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_matches_known_shapes() {
        assert_eq!(shannon_entropy(""), 0.0);
        assert_eq!(shannon_entropy("aaaa"), 0.0);
        // A uniform 64-symbol alphabet carries 6 bits per byte.
        let alphabet: String = ('A'..='Z')
            .chain('a'..='z')
            .chain('0'..='9')
            .chain(['+', '/'])
            .collect();
        assert!((shannon_entropy(&alphabet) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn minified_but_benign_is_not_flagged() {
        let mut source = String::from("var t=function(e){return e+1};");
        while source.len() < 6_000 {
            source.push_str("var a=function(b){return b*2};");
        }
        let score = score_javascript(&source);
        assert_eq!(score.long_lines, 1);
        assert!(score.score < OBFUSCATION_THRESHOLD, "{score:?}");
        assert!(!score.is_obfuscated());
        assert!(scan_obfuscation("bundle.min.js", &source).is_none());
    }

    #[test]
    fn obfuscated_sample_is_flagged() {
        let blob =
            "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9wcXJzdHV2d3h5ejAxMjM0NTY3ODk=";
        let padded = blob.repeat(3);
        let source = format!("const p = \"{padded}\";\nconst decoded = atob(p);\neval(decoded);\n");
        let score = score_javascript(&source);
        assert!(score.base64_blobs >= 1, "{score:?}");
        assert_eq!(score.atob_calls, 1);
        assert_eq!(score.eval_calls, 1);
        assert!(score.is_obfuscated(), "{score:?}");
        let finding = scan_obfuscation("payload.js", &source).expect("must be flagged");
        assert_eq!(finding.file, "payload.js");
        assert_eq!(finding.severity, Severity::High);
        assert!(finding.message.contains("obfuscated JavaScript"));
    }

    #[test]
    fn dynamic_code_signals_are_detected() {
        let score = score_javascript("eval(x); new Function('return 1')(); atob(y);");
        assert_eq!(score.eval_calls, 1);
        assert_eq!(score.function_constructors, 1);
        assert_eq!(score.atob_calls, 1);
        assert!(score.is_obfuscated());
    }

    #[test]
    fn hex_blob_over_200_chars_is_flagged() {
        let hex = "0123456789abcdef".repeat(14); // 224 chars, mixed digits/letters
        let source = format!("var payload = \"{hex}\";");
        let score = score_javascript(&source);
        assert_eq!(score.hex_blobs, 1, "{score:?}");
        assert!(score.is_obfuscated(), "{score:?}");
    }

    #[test]
    fn short_encoded_strings_are_not_blobs() {
        let source = "var a = \"abcdef0123456789\"; var b = atob(a);";
        let score = score_javascript(source);
        assert_eq!(score.base64_blobs, 0);
        assert_eq!(score.hex_blobs, 0);
        assert_eq!(score.atob_calls, 1);
        assert!(!score.is_obfuscated(), "{score:?}");
    }

    #[test]
    fn markers_in_strings_and_comments_do_not_count() {
        let source = "// eval(payload)\n/* new Function(x) */\nconst s = \"eval(x) atob(y)\";\n";
        let score = score_javascript(source);
        assert_eq!(score.eval_calls, 0, "{score:?}");
        assert_eq!(score.function_constructors, 0, "{score:?}");
        assert_eq!(score.atob_calls, 0, "{score:?}");
    }

    #[test]
    fn identifiers_containing_marker_names_do_not_count() {
        let score = score_javascript("mediaeval(x); myeval(y); newFunction(z); myatob(q);");
        assert_eq!(score.eval_calls, 0, "{score:?}");
        assert_eq!(score.function_constructors, 0, "{score:?}");
        assert_eq!(score.atob_calls, 0, "{score:?}");
    }

    #[test]
    fn long_line_alone_is_below_threshold() {
        let source = format!("var x = {};", "1+".repeat(3_000));
        let score = score_javascript(&source);
        assert_eq!(score.long_lines, 1);
        assert_eq!(score.score, 1);
        assert!(!score.is_obfuscated());
    }

    #[test]
    fn scan_tree_only_reports_javascript() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("clean.js"), "module.exports = 1;\n").unwrap();
        std::fs::write(root.join("notes.txt"), "eval(atob('x'))\n").unwrap();
        let blob =
            "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9wcXJzdHV2d3h5ejAxMjM0NTY3ODk="
                .repeat(3);
        std::fs::write(
            root.join("payload.js"),
            format!("const p=\"{blob}\";eval(atob(p));"),
        )
        .unwrap();
        let findings = scan_tree(root).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "payload.js");
    }
}
