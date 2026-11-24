//! Severity scoring for OSV vulnerabilities.
//!
//! Parses CVSS v3.x vectors into a base score (per the FIRST.org v3.1
//! specification) and a [`Severity`] level, falling back to the
//! `database_specific.severity` label when no usable vector is present.

use super::osv_client::OsvVulnerability;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// Maps a CVSS v3 base score to its qualitative rating. A score of 0.0
    /// ("None" in CVSS) is reported as `Low`.
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s >= 9.0 => Severity::Critical,
            s if s >= 7.0 => Severity::High,
            s if s >= 4.0 => Severity::Medium,
            _ => Severity::Low,
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label.to_ascii_uppercase().as_str() {
            "LOW" => Some(Severity::Low),
            "MODERATE" | "MEDIUM" => Some(Severity::Medium),
            "HIGH" => Some(Severity::High),
            "CRITICAL" => Some(Severity::Critical),
            _ => None,
        }
    }
}

/// Label attached to OSV `MAL-` advisories, which report known malware
/// rather than a conventional vulnerability.
pub const MALICIOUS_PACKAGE_LABEL: &str = "malicious package";

/// A vulnerability's severity level, with the numeric score when it came
/// from a CVSS vector, and an optional label (e.g. for malware advisories).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeverityScore {
    pub base_score: Option<f64>,
    pub level: Severity,
    pub label: Option<&'static str>,
}

/// Computes the CVSS v3.0/v3.1 base score from a vector string such as
/// `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H`. Returns `None` if the
/// vector isn't a well-formed v3.x vector.
pub fn parse_cvss_v3(vector: &str) -> Option<f64> {
    let mut parts = vector.split('/');
    let version = parts.next()?.strip_prefix("CVSS:3.")?;
    if version != "0" && version != "1" {
        return None;
    }

    let (mut av, mut ac, mut pr, mut ui, mut s) = (None, None, None, None, None);
    let (mut c, mut i, mut a) = (None, None, None);
    for part in parts {
        let (key, value) = part.split_once(':')?;
        match key {
            "AV" => av = Some(value),
            "AC" => ac = Some(value),
            "PR" => pr = Some(value),
            "UI" => ui = Some(value),
            "S" => s = Some(value),
            "C" => c = Some(value),
            "I" => i = Some(value),
            "A" => a = Some(value),
            _ => {} // temporal/environmental metrics don't affect the base score
        }
    }

    let changed = match s? {
        "U" => false,
        "C" => true,
        _ => return None,
    };
    let av = match av? {
        "N" => 0.85,
        "A" => 0.62,
        "L" => 0.55,
        "P" => 0.2,
        _ => return None,
    };
    let ac = match ac? {
        "L" => 0.77,
        "H" => 0.44,
        _ => return None,
    };
    let pr = match (pr?, changed) {
        ("N", _) => 0.85,
        ("L", false) => 0.62,
        ("L", true) => 0.68,
        ("H", false) => 0.27,
        ("H", true) => 0.5,
        _ => return None,
    };
    let ui = match ui? {
        "N" => 0.85,
        "R" => 0.62,
        _ => return None,
    };
    let cia = |v: &str| match v {
        "H" => Some(0.56),
        "L" => Some(0.22),
        "N" => Some(0.0),
        _ => None,
    };
    let iss: f64 = 1.0 - (1.0 - cia(c?)?) * (1.0 - cia(i?)?) * (1.0 - cia(a?)?);

    let impact = if changed {
        7.52 * (iss - 0.029) - 3.25 * (iss - 0.02).powi(15)
    } else {
        6.42 * iss
    };
    if impact <= 0.0 {
        return Some(0.0);
    }
    let exploitability = 8.22 * av * ac * pr * ui;
    let raw = if changed {
        1.08 * (impact + exploitability)
    } else {
        impact + exploitability
    };
    Some(roundup(raw.min(10.0)))
}

/// CVSS v3.1 Roundup: smallest one-decimal value >= `x`, immune to float
/// noise.
fn roundup(x: f64) -> f64 {
    let int = (x * 100_000.0).round() as i64;
    if int % 10_000 == 0 {
        int as f64 / 100_000.0
    } else {
        ((int / 10_000) + 1) as f64 / 10.0
    }
}

/// Scores a vulnerability from its CVSS v3 vector, or else from the
/// `database_specific.severity` label. `None` if neither is available.
///
/// OSV `MAL-` advisories report known malware; they are always treated as
/// [`Severity::Critical`] with the [`MALICIOUS_PACKAGE_LABEL`], regardless
/// of any CVSS data.
pub fn assess(vuln: &OsvVulnerability) -> Option<SeverityScore> {
    if vuln.id.starts_with("MAL-") {
        return Some(SeverityScore {
            base_score: None,
            level: Severity::Critical,
            label: Some(MALICIOUS_PACKAGE_LABEL),
        });
    }

    let from_vector = vuln
        .severity
        .iter()
        .filter(|s| s.severity_type.starts_with("CVSS_V3"))
        .find_map(|s| parse_cvss_v3(&s.score));
    if let Some(score) = from_vector {
        return Some(SeverityScore {
            base_score: Some(score),
            level: Severity::from_score(score),
            label: None,
        });
    }

    let label = vuln.database_specific.as_ref()?.get("severity")?.as_str()?;
    Some(SeverityScore {
        base_score: None,
        level: Severity::from_label(label)?,
        label: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_vectors_to_published_scores() {
        let cases = [
            // Log4Shell CVE-2021-44228
            ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H", 10.0),
            // lodash CVE-2020-28500
            ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:L", 5.3),
            // Reflected XSS, typical
            ("CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:C/C:L/I:L/A:N", 6.1),
            // Local privilege escalation, typical
            ("CVSS:3.1/AV:L/AC:L/PR:L/UI:N/S:U/C:H/I:H/A:H", 7.8),
            // Unauthenticated RCE
            ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 9.8),
            ("CVSS:3.0/AV:N/AC:L/PR:L/UI:N/S:U/C:L/I:L/A:L", 6.3),
        ];
        for (vector, expected) in cases {
            assert_eq!(parse_cvss_v3(vector), Some(expected), "{vector}");
        }
    }

    #[test]
    fn rejects_malformed_or_non_v3_vectors() {
        assert_eq!(parse_cvss_v3("CVSS:2.0/AV:N"), None);
        assert_eq!(parse_cvss_v3("CVSS:3.1/AV:N/AC:L"), None);
        assert_eq!(parse_cvss_v3("not a vector"), None);
    }

    #[test]
    fn maps_scores_to_levels() {
        assert_eq!(Severity::from_score(0.0), Severity::Low);
        assert_eq!(Severity::from_score(3.9), Severity::Low);
        assert_eq!(Severity::from_score(4.0), Severity::Medium);
        assert_eq!(Severity::from_score(7.0), Severity::High);
        assert_eq!(Severity::from_score(9.0), Severity::Critical);
    }

    fn vuln(json: &str) -> OsvVulnerability {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn assess_prefers_vector_then_falls_back_to_database_specific() {
        let v = vuln(
            r#"{"id":"X","severity":[{"type":"CVSS_V3","score":"CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"}],
                "database_specific":{"severity":"LOW"}}"#,
        );
        assert_eq!(
            assess(&v),
            Some(SeverityScore {
                base_score: Some(9.8),
                level: Severity::Critical,
                label: None,
            })
        );

        let v = vuln(r#"{"id":"Y","database_specific":{"severity":"MODERATE"}}"#);
        assert_eq!(
            assess(&v),
            Some(SeverityScore {
                base_score: None,
                level: Severity::Medium,
                label: None,
            })
        );

        assert_eq!(assess(&vuln(r#"{"id":"Z"}"#)), None);
    }

    #[test]
    fn mal_advisories_are_critical_malicious_packages() {
        let v = vuln(r#"{"id":"MAL-2024-1234"}"#);
        assert_eq!(
            assess(&v),
            Some(SeverityScore {
                base_score: None,
                level: Severity::Critical,
                label: Some(MALICIOUS_PACKAGE_LABEL),
            })
        );

        // Malware wins even when a low-severity CVSS vector is present.
        let v = vuln(
            r#"{"id":"MAL-2024-5678","severity":[{"type":"CVSS_V3","score":"CVSS:3.1/AV:L/AC:H/PR:H/UI:R/S:U/C:N/I:N/A:N"}],
                "database_specific":{"severity":"LOW"}}"#,
        );
        let score = assess(&v).unwrap();
        assert_eq!(score.level, Severity::Critical);
        assert_eq!(score.label, Some(MALICIOUS_PACKAGE_LABEL));
    }
}
