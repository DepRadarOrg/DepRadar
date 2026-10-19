//! Accepted-risk suppressions from a `.depradarignore` file.
//!
//! The file is TOML: one `[[ignore]]` table per suppression.
//!
//! ```toml
//! [[ignore]]
//! package = "lodash"
//! version = "4.17.4"                 # optional: only this version
//! advisory = "GHSA-29mw-wpgm-hmr9"   # optional: only this advisory
//! reason = "Only used at build time; upgrade tracked in #123"
//! expires = 2026-12-31               # optional: last day it applies
//! ```
//!
//! `package` and a non-empty `reason` are required, so every suppression
//! says why it exists. An entry without `advisory` suppresses every finding
//! for the package (typosquat findings included); one with `advisory` only
//! suppresses that vulnerability. Once `expires` has passed, the entry stops
//! suppressing anything and the scan reports a warning instead.
//!
//! Only the subset of TOML these files need is parsed: `[[ignore]]` headers,
//! bare keys, basic (`"..."`) and literal (`'...'`) strings, local dates
//! (`2026-12-31`) and `#` comments. Anything else is a parse error rather
//! than being silently ignored.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{Finding, FindingKind};

/// Default ignore file name, looked up next to the scanned lockfile.
pub const IGNORE_FILE_NAME: &str = ".depradarignore";

const KNOWN_KEYS: [&str; 5] = ["package", "version", "advisory", "reason", "expires"];

/// A calendar date (no time zone), as used for `expires`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// Parses a `YYYY-MM-DD` date, checking that the day exists.
    pub fn parse(text: &str) -> Option<Date> {
        let bytes = text.as_bytes();
        let well_formed = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
        if !well_formed {
            return None;
        }

        let year: i32 = text[0..4].parse().ok()?;
        let month: u8 = text[5..7].parse().ok()?;
        let day: u8 = text[8..10].parse().ok()?;
        let valid = (1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day);
        valid.then_some(Date { year, month, day })
    }

    /// Today's date in UTC, from the system clock.
    pub fn today() -> Date {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        Date::from_days_since_epoch((seconds / 86_400) as i64)
    }

    /// Converts a count of days since 1970-01-01 to a civil date, using
    /// Howard Hinnant's `civil_from_days` algorithm.
    fn from_days_since_epoch(days: i64) -> Date {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Date {
            year: year as i32,
            month: month as u8,
            day: day as u8,
        }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// One suppression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreEntry {
    pub package: String,
    pub version: Option<String>,
    pub advisory: Option<String>,
    pub reason: String,
    /// Last day the suppression applies.
    pub expires: Option<Date>,
}

impl IgnoreEntry {
    /// True once `today` is past the expiry date.
    pub fn is_expired(&self, today: Date) -> bool {
        self.expires.is_some_and(|expires| today > expires)
    }

    /// True if this entry covers `finding` (ignoring expiry).
    pub fn matches(&self, finding: &Finding) -> bool {
        if self.package != finding.package {
            return false;
        }
        if let Some(version) = &self.version {
            if finding.version.as_deref() != Some(version.as_str()) {
                return false;
            }
        }
        match (&self.advisory, &finding.kind) {
            (None, _) => true,
            (Some(advisory), FindingKind::Vulnerability { advisory_id }) => {
                advisory.eq_ignore_ascii_case(advisory_id)
            }
            (Some(_), FindingKind::Typosquat { .. }) => false,
            (Some(_), FindingKind::InstallScript { .. }) => false,
        }
    }
}

/// A finding that an ignore entry suppressed, with the entry's justification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suppressed {
    pub finding: Finding,
    pub reason: String,
    pub expires: Option<Date>,
}

/// The result of [`IgnoreFile::apply`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Applied {
    /// Findings no active entry matched.
    pub kept: Vec<Finding>,
    pub suppressed: Vec<Suppressed>,
    /// One warning per expired entry.
    pub warnings: Vec<String>,
}

/// A parsed `.depradarignore` file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IgnoreFile {
    pub entries: Vec<IgnoreEntry>,
}

impl IgnoreFile {
    /// Reads and parses the ignore file at `path`.
    pub fn load(path: &Path) -> Result<IgnoreFile, IgnoreError> {
        let contents = std::fs::read_to_string(path).map_err(IgnoreError::Io)?;
        IgnoreFile::parse(&contents)
    }

    /// Parses the contents of an ignore file.
    pub fn parse(contents: &str) -> Result<IgnoreFile, IgnoreError> {
        let mut entries = Vec::new();
        let mut current: Option<RawEntry> = None;

        for (index, raw_line) in contents.lines().enumerate() {
            let line_no = index + 1;
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }

            if line == "[[ignore]]" {
                if let Some(entry) = current.take() {
                    entries.push(entry.finish()?);
                }
                current = Some(RawEntry {
                    line: line_no,
                    fields: BTreeMap::new(),
                });
                continue;
            }
            if line.starts_with('[') {
                return Err(parse_error(
                    line_no,
                    format!("unexpected table `{line}`; only [[ignore]] entries are supported"),
                ));
            }

            let Some((key, value)) = line.split_once('=') else {
                return Err(parse_error(line_no, "expected `key = value`"));
            };
            let key = key.trim();
            let Some(entry) = current.as_mut() else {
                return Err(parse_error(
                    line_no,
                    format!("`{key}` must be inside an [[ignore]] entry"),
                ));
            };
            if !KNOWN_KEYS.contains(&key) {
                return Err(parse_error(
                    line_no,
                    format!(
                        "unknown key `{key}` (expected one of: {})",
                        KNOWN_KEYS.join(", ")
                    ),
                ));
            }
            let value =
                parse_value(value.trim()).map_err(|message| parse_error(line_no, message))?;
            if entry.fields.insert(key.to_string(), value).is_some() {
                return Err(parse_error(line_no, format!("duplicate key `{key}`")));
            }
        }

        if let Some(entry) = current {
            entries.push(entry.finish()?);
        }
        Ok(IgnoreFile { entries })
    }

    /// Splits `findings` into those still reported and those suppressed by
    /// an unexpired entry, and warns about every expired entry.
    pub fn apply(&self, findings: Vec<Finding>, today: Date) -> Applied {
        let mut applied = Applied::default();

        let active: Vec<&IgnoreEntry> = self
            .entries
            .iter()
            .filter(|entry| {
                if !entry.is_expired(today) {
                    return true;
                }
                let expires = entry.expires.map(|d| d.to_string()).unwrap_or_default();
                applied.warnings.push(format!(
                    "{IGNORE_FILE_NAME}: suppression for {} expired on {expires} and no \
                     longer applies (reason was: {})",
                    describe(entry),
                    entry.reason
                ));
                false
            })
            .collect();

        for finding in findings {
            match active.iter().find(|entry| entry.matches(&finding)) {
                Some(entry) => applied.suppressed.push(Suppressed {
                    finding,
                    reason: entry.reason.clone(),
                    expires: entry.expires,
                }),
                None => applied.kept.push(finding),
            }
        }

        applied
    }
}

/// `package[@version][ (advisory)]`, for warnings.
fn describe(entry: &IgnoreEntry) -> String {
    let mut text = entry.package.clone();
    if let Some(version) = &entry.version {
        text.push('@');
        text.push_str(version);
    }
    if let Some(advisory) = &entry.advisory {
        text.push_str(&format!(" ({advisory})"));
    }
    text
}

/// Errors reading or parsing an ignore file.
#[derive(Debug)]
pub enum IgnoreError {
    Io(std::io::Error),
    Parse { line: usize, message: String },
}

impl fmt::Display for IgnoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IgnoreError::Io(err) => write!(f, "failed to read {IGNORE_FILE_NAME}: {err}"),
            IgnoreError::Parse { line, message } => {
                write!(f, "{IGNORE_FILE_NAME} line {line}: {message}")
            }
        }
    }
}

impl std::error::Error for IgnoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IgnoreError::Io(err) => Some(err),
            IgnoreError::Parse { .. } => None,
        }
    }
}

fn parse_error(line: usize, message: impl Into<String>) -> IgnoreError {
    IgnoreError::Parse {
        line,
        message: message.into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    String(String),
    Date(Date),
}

/// An `[[ignore]]` table as written, before required keys are checked.
struct RawEntry {
    /// Line of the `[[ignore]]` header, for errors about missing keys.
    line: usize,
    fields: BTreeMap<String, Value>,
}

impl RawEntry {
    fn finish(mut self) -> Result<IgnoreEntry, IgnoreError> {
        let line = self.line;
        let mut string = |key: &str| match self.fields.remove(key) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            Some(Value::Date(_)) => Err(parse_error(line, format!("`{key}` must be a string"))),
        };

        let package = string("package")?
            .filter(|package| !package.trim().is_empty())
            .ok_or_else(|| parse_error(line, "[[ignore]] entry is missing `package`"))?;
        let reason = string("reason")?
            .filter(|reason| !reason.trim().is_empty())
            .ok_or_else(|| {
                parse_error(
                    line,
                    format!("[[ignore]] entry for `{package}` is missing a `reason`"),
                )
            })?;
        let version = string("version")?;
        let advisory = string("advisory")?;

        let expires = match self.fields.remove("expires") {
            None => None,
            Some(Value::Date(date)) => Some(date),
            Some(Value::String(text)) => Some(Date::parse(&text).ok_or_else(|| {
                parse_error(
                    line,
                    format!("`expires` must be a YYYY-MM-DD date, got `{text}`"),
                )
            })?),
        };

        Ok(IgnoreEntry {
            package,
            version,
            advisory,
            reason,
            expires,
        })
    }
}

/// Drops a `#` comment that isn't inside a string.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, c) in line.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return &line[..index],
            None => {}
        }
    }
    line
}

/// Parses a value: a basic string, a literal string or a bare date.
fn parse_value(text: &str) -> Result<Value, String> {
    if let Some(rest) = text.strip_prefix('"') {
        let mut value = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return end_of_value(chars.as_str(), Value::String(value)),
                '\\' => match chars.next() {
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    other => {
                        return Err(format!(
                            "unsupported escape `\\{}` in string",
                            other.map(String::from).unwrap_or_default()
                        ))
                    }
                },
                _ => value.push(c),
            }
        }
        return Err("unterminated string".to_string());
    }

    if let Some(rest) = text.strip_prefix('\'') {
        let Some(end) = rest.find('\'') else {
            return Err("unterminated string".to_string());
        };
        return end_of_value(&rest[end + 1..], Value::String(rest[..end].to_string()));
    }

    Date::parse(text).map(Value::Date).ok_or_else(|| {
        format!("unsupported value `{text}`: use a quoted string or a YYYY-MM-DD date")
    })
}

fn end_of_value(trailing: &str, value: Value) -> Result<Value, String> {
    if trailing.trim().is_empty() {
        Ok(value)
    } else {
        Err(format!("unexpected `{}` after value", trailing.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Ecosystem;

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    fn vulnerability(package: &str, version: &str, advisory_id: &str) -> Finding {
        Finding {
            package: package.to_string(),
            version: Some(version.to_string()),
            ecosystem: Ecosystem::Npm,
            direct: true,
            path: vec!["my-app".to_string(), package.to_string()],
            kind: FindingKind::Vulnerability {
                advisory_id: advisory_id.to_string(),
            },
        }
    }

    const LODASH_IGNORE: &str = r#"
# Accepted risk, reviewed by the security team.
[[ignore]]
package = "lodash"
version = "4.17.4"
advisory = "GHSA-29mw-wpgm-hmr9"
reason = "Only used in build tooling; upgrade tracked in #123"
expires = 2026-12-31
"#;

    #[test]
    fn parses_a_full_entry() {
        let file = IgnoreFile::parse(LODASH_IGNORE).unwrap();
        assert_eq!(
            file.entries,
            vec![IgnoreEntry {
                package: "lodash".to_string(),
                version: Some("4.17.4".to_string()),
                advisory: Some("GHSA-29mw-wpgm-hmr9".to_string()),
                reason: "Only used in build tooling; upgrade tracked in #123".to_string(),
                expires: Some(date("2026-12-31")),
            }]
        );
    }

    #[test]
    fn matching_entry_suppresses_the_finding() {
        let file = IgnoreFile::parse(LODASH_IGNORE).unwrap();
        let findings = vec![
            vulnerability("lodash", "4.17.4", "GHSA-29mw-wpgm-hmr9"),
            vulnerability("lodash", "4.17.4", "GHSA-other"),
            vulnerability("lodash", "4.17.21", "GHSA-29mw-wpgm-hmr9"),
        ];

        let applied = file.apply(findings.clone(), date("2026-12-31"));

        assert_eq!(applied.suppressed.len(), 1);
        assert_eq!(applied.suppressed[0].finding, findings[0]);
        assert_eq!(
            applied.suppressed[0].reason,
            "Only used in build tooling; upgrade tracked in #123"
        );
        assert_eq!(applied.kept, findings[1..].to_vec());
        assert!(applied.warnings.is_empty());
    }

    #[test]
    fn expired_entry_warns_and_stops_suppressing() {
        let file = IgnoreFile::parse(LODASH_IGNORE).unwrap();
        let findings = vec![vulnerability("lodash", "4.17.4", "GHSA-29mw-wpgm-hmr9")];

        let applied = file.apply(findings.clone(), date("2027-01-01"));

        assert!(applied.suppressed.is_empty());
        assert_eq!(applied.kept, findings);
        assert_eq!(applied.warnings.len(), 1);
        assert!(applied.warnings[0].contains("expired on 2026-12-31"));
    }

    #[test]
    fn entry_without_advisory_covers_every_finding_for_the_package() {
        let file = IgnoreFile::parse(
            "[[ignore]]\npackage = 'lodash'\nreason = \"vendored, patched in-tree\"\n",
        )
        .unwrap();
        let findings = vec![
            vulnerability("lodash", "4.17.4", "GHSA-a"),
            vulnerability("lodash", "4.17.21", "GHSA-b"),
            vulnerability("express", "4.0.0", "GHSA-c"),
        ];

        let applied = file.apply(findings, date("2026-01-01"));

        assert_eq!(applied.suppressed.len(), 2);
        assert_eq!(applied.kept.len(), 1);
        assert_eq!(applied.kept[0].package, "express");
    }

    #[test]
    fn missing_reason_is_a_parse_error() {
        let err = IgnoreFile::parse("[[ignore]]\npackage = \"lodash\"\n").unwrap_err();
        match err {
            IgnoreError::Parse { line, message } => {
                assert_eq!(line, 1);
                assert!(message.contains("missing a `reason`"), "{message}");
            }
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    #[test]
    fn empty_reason_is_a_parse_error() {
        let err = IgnoreFile::parse("[[ignore]]\npackage = \"lodash\"\nreason = \"  \"\n");
        assert!(matches!(err, Err(IgnoreError::Parse { line: 1, .. })));
    }

    #[test]
    fn rejects_unknown_keys_bad_dates_and_stray_keys() {
        assert!(IgnoreFile::parse("[[ignore]]\npakage = \"x\"\n").is_err());
        assert!(IgnoreFile::parse(
            "[[ignore]]\npackage = \"x\"\nreason = \"y\"\nexpires = 2026-02-30\n"
        )
        .is_err());
        assert!(IgnoreFile::parse("package = \"x\"\n").is_err());
    }

    #[test]
    fn comments_inside_strings_are_kept() {
        let file =
            IgnoreFile::parse("[[ignore]]\npackage = \"x\" # trailing\nreason = \"see #42\"\n")
                .unwrap();
        assert_eq!(file.entries[0].package, "x");
        assert_eq!(file.entries[0].reason, "see #42");
    }

    #[test]
    fn converts_days_since_epoch_to_dates() {
        assert_eq!(Date::from_days_since_epoch(0), date("1970-01-01"));
        assert_eq!(Date::from_days_since_epoch(19_782), date("2024-02-29"));
        assert_eq!(Date::from_days_since_epoch(20_742), date("2026-10-16"));
    }
}
