//! Minimal client for the [OSV.dev](https://osv.dev) vulnerability database.
//!
//! `query_osv` resolves a `(package name, ecosystem, version)` triple to the
//! raw set of vulnerabilities OSV knows about for that exact version, via
//! [`POST /v1/query`](https://google.github.io/osv.dev/api/#post-v1query).
//! There is no filtering, deduplication, or severity scoring here — this is
//! just fetch-and-parse into typed structs. Turning the raw matches into
//! scored findings belongs with the (not-yet-written) CVE scanner module.

use serde::{Deserialize, Serialize};

const DEFAULT_OSV_BASE_URL: &str = "https://api.osv.dev";
const USER_AGENT: &str = concat!(
    "depradar/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/DepRadarOrg/DepRadar)"
);

/// Package registries `query_osv` can query OSV against.
///
/// OSV's own ecosystem list is much larger, but the crawler only targets
/// these three registries today; add variants here as more are wired up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecosystem {
    Npm,
    PyPI,
    CratesIo,
}

impl Ecosystem {
    /// The ecosystem name OSV expects in a query, exactly as documented at
    /// <https://ossf.github.io/osv-schema/#affectedpackage-field>.
    fn as_osv_str(self) -> &'static str {
        match self {
            Ecosystem::Npm => "npm",
            Ecosystem::PyPI => "PyPI",
            Ecosystem::CratesIo => "crates.io",
        }
    }
}

/// Request body for `POST /v1/query`: a single package name + ecosystem,
/// pinned to one version.
#[derive(Debug, Serialize)]
struct OsvQueryRequest<'a> {
    version: &'a str,
    package: OsvPackageQuery<'a>,
}

#[derive(Debug, Serialize)]
struct OsvPackageQuery<'a> {
    name: &'a str,
    ecosystem: &'a str,
}

/// The package identity as OSV echoes it back inside an `affected` entry.
#[derive(Debug, Clone, Deserialize)]
pub struct OsvPackage {
    pub name: String,
    pub ecosystem: String,
    /// Package URL, e.g. `pkg:npm/lodash`. Not present for every ecosystem.
    pub purl: Option<String>,
}

/// One event in a version range: the point where a vulnerability was
/// introduced, fixed, or last known to affect a version. Exactly one field
/// is set per event, per the OSV schema.
#[derive(Debug, Clone, Deserialize)]
pub struct OsvEvent {
    pub introduced: Option<String>,
    pub fixed: Option<String>,
    pub last_affected: Option<String>,
    pub limit: Option<String>,
}

/// A version range (e.g. a SEMVER range) bounding which versions of a
/// package are affected.
#[derive(Debug, Clone, Deserialize)]
pub struct OsvRange {
    #[serde(rename = "type")]
    pub range_type: String,
    #[serde(default)]
    pub events: Vec<OsvEvent>,
}

/// One `affected` entry: a package (possibly a different name/ecosystem than
/// the one queried, e.g. an alias package) plus the ranges and/or explicit
/// version list that are affected.
#[derive(Debug, Clone, Deserialize)]
pub struct OsvAffected {
    pub package: Option<OsvPackage>,
    #[serde(default)]
    pub ranges: Vec<OsvRange>,
    #[serde(default)]
    pub versions: Vec<String>,
}

/// A severity score reported for a vulnerability, e.g. a CVSS vector string.
#[derive(Debug, Clone, Deserialize)]
pub struct OsvSeverity {
    #[serde(rename = "type")]
    pub severity_type: String,
    pub score: String,
}

/// A reference URL for a vulnerability (advisory, fix commit, report, etc).
#[derive(Debug, Clone, Deserialize)]
pub struct OsvReference {
    #[serde(rename = "type")]
    pub reference_type: String,
    pub url: String,
}

/// One vulnerability record as returned by OSV.
///
/// This mirrors the subset of the [OSV schema](https://ossf.github.io/osv-schema/)
/// that `POST /v1/query` populates. `database_specific` fields vary by
/// source and aren't modeled here.
#[derive(Debug, Clone, Deserialize)]
pub struct OsvVulnerability {
    pub id: String,
    pub summary: Option<String>,
    pub details: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub modified: Option<String>,
    pub published: Option<String>,
    #[serde(default)]
    pub severity: Vec<OsvSeverity>,
    #[serde(default)]
    pub affected: Vec<OsvAffected>,
    #[serde(default)]
    pub references: Vec<OsvReference>,
    /// RFC 3339 timestamp set when OSV has withdrawn the record.
    pub withdrawn: Option<String>,
}

/// Response body of `POST /v1/query`: the list of vulnerabilities matching
/// the queried package/version, if any.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OsvQueryResponse {
    #[serde(default)]
    pub vulns: Vec<OsvVulnerability>,
}

/// Errors from querying OSV.
#[derive(Debug)]
pub enum OsvClientError {
    /// The HTTP request itself failed (network error, timeout, TLS, etc).
    Request(reqwest::Error),
    /// OSV responded with 429. `retry_after` is the `Retry-After` header in
    /// seconds, when present and numeric.
    RateLimited { retry_after: Option<u64> },
    /// OSV responded with 404.
    NotFound,
    /// OSV responded, but with another non-success status code.
    UnexpectedStatus(reqwest::StatusCode),
    /// The response body wasn't the JSON shape `OsvQueryResponse` expects.
    Decode(reqwest::Error),
}

impl std::fmt::Display for OsvClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsvClientError::Request(err) => write!(f, "OSV request failed: {err}"),
            OsvClientError::RateLimited {
                retry_after: Some(secs),
            } => write!(f, "OSV rate limit hit, retry after {secs}s"),
            OsvClientError::RateLimited { retry_after: None } => {
                write!(f, "OSV rate limit hit")
            }
            OsvClientError::NotFound => write!(f, "OSV returned 404 not found"),
            OsvClientError::UnexpectedStatus(status) => {
                write!(f, "OSV returned unexpected status: {status}")
            }
            OsvClientError::Decode(err) => write!(f, "failed to decode OSV response: {err}"),
        }
    }
}

impl std::error::Error for OsvClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OsvClientError::Request(err) | OsvClientError::Decode(err) => Some(err),
            OsvClientError::RateLimited { .. }
            | OsvClientError::NotFound
            | OsvClientError::UnexpectedStatus(_) => None,
        }
    }
}

/// Merges records that refer to the same issue, i.e. whose `id`/`aliases`
/// overlap (typically a GHSA record and its CVE).
///
/// The GHSA record is kept as primary; otherwise the first record seen wins.
/// The merged record's `id` and `aliases` together contain every ID from the
/// records it absorbed.
pub fn dedupe_by_alias(vulns: Vec<OsvVulnerability>) -> Vec<OsvVulnerability> {
    let ids = |v: &OsvVulnerability| -> Vec<String> {
        std::iter::once(v.id.clone())
            .chain(v.aliases.iter().cloned())
            .collect()
    };

    let mut groups: Vec<(Vec<String>, OsvVulnerability)> = Vec::new();
    for vuln in vulns {
        let vuln_ids = ids(&vuln);
        let Some(pos) = groups
            .iter()
            .position(|(known, _)| vuln_ids.iter().any(|id| known.contains(id)))
        else {
            groups.push((vuln_ids, vuln));
            continue;
        };

        let (known, primary) = &mut groups[pos];
        for id in vuln_ids {
            if !known.contains(&id) {
                known.push(id);
            }
        }
        if vuln.id.starts_with("GHSA-") && !primary.id.starts_with("GHSA-") {
            *primary = vuln;
        }
    }

    groups
        .into_iter()
        .map(|(all_ids, mut primary)| {
            primary.aliases = all_ids.into_iter().filter(|id| *id != primary.id).collect();
            primary
        })
        .collect()
}

/// Client for the OSV API with a configurable base URL, for mirrors,
/// staging and mock servers.
pub struct OsvClient {
    base_url: String,
    http: reqwest::blocking::Client,
}

impl Default for OsvClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OsvClient {
    /// A client pointed at the public OSV API.
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_OSV_BASE_URL)
    }

    /// A client pointed at `base_url` (e.g. `http://127.0.0.1:8080`), without
    /// a trailing path; `/v1/query` is appended to it.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("static reqwest client config is valid");
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
        }
    }

    /// Queries OSV for known vulnerabilities affecting `package_name` at
    /// exactly `version` in `ecosystem`.
    ///
    /// Withdrawn records (those with a `withdrawn` timestamp) are filtered
    /// out: OSV keeps them around after retracting them, but they are false
    /// alarms and must not be reported as findings. Beyond that there is no
    /// deduplication (see [`dedupe_by_alias`]) or severity scoring; an empty
    /// `vulns` list means OSV has no known vulnerability for this version.
    pub fn query_osv(
        &self,
        package_name: &str,
        ecosystem: Ecosystem,
        version: &str,
    ) -> Result<OsvQueryResponse, OsvClientError> {
        let request_body = OsvQueryRequest {
            version,
            package: OsvPackageQuery {
                name: package_name,
                ecosystem: ecosystem.as_osv_str(),
            },
        };

        let response = self
            .http
            .post(format!("{}/v1/query", self.base_url))
            .json(&request_body)
            .send()
            .map_err(OsvClientError::Request)?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok());
            return Err(OsvClientError::RateLimited { retry_after });
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(OsvClientError::NotFound);
        }
        if !status.is_success() {
            return Err(OsvClientError::UnexpectedStatus(status));
        }

        let mut parsed: OsvQueryResponse = response.json().map_err(OsvClientError::Decode)?;
        parsed.vulns.retain(|v| v.withdrawn.is_none());
        Ok(parsed)
    }
}

/// Queries OSV using a default [`OsvClient`]. See [`OsvClient::query_osv`].
pub fn query_osv(
    package_name: &str,
    ecosystem: Ecosystem,
    version: &str,
) -> Result<OsvQueryResponse, OsvClientError> {
    OsvClient::new().query_osv(package_name, ecosystem, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `lodash@4.17.4` (npm) predates the fix for GHSA-29mw-wpgm-hmr9 /
    /// CVE-2020-28500 (a ReDoS in `trim`/`toNumber`/`trimEnd`, fixed in
    /// 4.17.21), so OSV is expected to report at least that one match.
    ///
    /// This hits the real OSV API and is skipped (not failed) if the
    /// request can't complete — e.g. no network access in a sandboxed CI
    /// runner — since that's an environment limitation, not a bug in the
    /// client.
    #[test]
    fn query_osv_finds_known_vulnerability_in_lodash() {
        let result = query_osv("lodash", Ecosystem::Npm, "4.17.4");

        let response = match result {
            Ok(response) => response,
            Err(err) => {
                eprintln!("skipping: OSV query failed ({err}), likely no network access");
                return;
            }
        };

        assert!(
            !response.vulns.is_empty(),
            "expected lodash@4.17.4 to have at least one known vulnerability"
        );
        assert!(
            response
                .vulns
                .iter()
                .any(|v| v.aliases.iter().any(|a| a == "CVE-2020-28500")),
            "expected CVE-2020-28500 among lodash@4.17.4's vulnerabilities, got: {:?}",
            response.vulns.iter().map(|v| &v.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ecosystem_as_osv_str_matches_osv_schema_names() {
        assert_eq!(Ecosystem::Npm.as_osv_str(), "npm");
        assert_eq!(Ecosystem::PyPI.as_osv_str(), "PyPI");
        assert_eq!(Ecosystem::CratesIo.as_osv_str(), "crates.io");
    }

    /// Serves one canned HTTP response on localhost and returns the base URL
    /// plus a receiver for the raw request the server saw.
    fn mock_server(
        status_line: &str,
        extra_headers: &str,
        body: &str,
    ) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status_line}\r\n{extra_headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap();
            tx.send(String::from_utf8_lossy(&buf[..n]).into_owned())
                .unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });
        (url, rx)
    }

    fn vuln(id: &str, aliases: &[&str]) -> OsvVulnerability {
        serde_json::from_value(serde_json::json!({ "id": id, "aliases": aliases })).unwrap()
    }

    #[test]
    fn dedupe_by_alias_merges_ghsa_and_cve_keeping_ghsa_primary() {
        let merged = dedupe_by_alias(vec![
            vuln("CVE-2020-28500", &["GHSA-29mw-wpgm-hmr9"]),
            vuln("GHSA-29mw-wpgm-hmr9", &["CVE-2020-28500"]),
            vuln("GHSA-other", &[]),
        ]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "GHSA-29mw-wpgm-hmr9");
        assert_eq!(merged[0].aliases, vec!["CVE-2020-28500"]);
        assert_eq!(merged[1].id, "GHSA-other");
    }

    #[test]
    fn client_sends_user_agent_to_custom_base_url() {
        let (url, rx) = mock_server("200 OK", "", "{}");
        let response = OsvClient::with_base_url(url)
            .query_osv("lodash", Ecosystem::Npm, "4.17.4")
            .unwrap();
        assert!(response.vulns.is_empty());
        let request = rx.recv().unwrap().to_lowercase();
        assert!(request.starts_with("post /v1/query"));
        assert!(request.contains(&format!("user-agent: {}", USER_AGENT.to_lowercase())));
    }

    #[test]
    fn query_filters_out_withdrawn_records() {
        let body = r#"{"vulns":[{"id":"GHSA-live"},{"id":"GHSA-gone","withdrawn":"2024-01-01T00:00:00Z"}]}"#;
        let (url, _rx) = mock_server("200 OK", "", body);
        let response = OsvClient::with_base_url(url)
            .query_osv("lodash", Ecosystem::Npm, "4.17.4")
            .unwrap();
        let ids: Vec<_> = response.vulns.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["GHSA-live"]);
    }

    #[test]
    fn rate_limit_with_retry_after_is_typed() {
        let (url, _rx) = mock_server("429 Too Many Requests", "Retry-After: 30\r\n", "{}");
        let err = OsvClient::with_base_url(url)
            .query_osv("lodash", Ecosystem::Npm, "1.0.0")
            .unwrap_err();
        assert!(matches!(
            err,
            OsvClientError::RateLimited {
                retry_after: Some(30)
            }
        ));
    }

    #[test]
    fn rate_limit_without_retry_after_is_typed() {
        let (url, _rx) = mock_server("429 Too Many Requests", "", "{}");
        let err = OsvClient::with_base_url(url)
            .query_osv("lodash", Ecosystem::Npm, "1.0.0")
            .unwrap_err();
        assert!(matches!(
            err,
            OsvClientError::RateLimited { retry_after: None }
        ));
    }

    #[test]
    fn not_found_is_typed() {
        let (url, _rx) = mock_server("404 Not Found", "", "{}");
        let err = OsvClient::with_base_url(url)
            .query_osv("lodash", Ecosystem::Npm, "1.0.0")
            .unwrap_err();
        assert!(matches!(err, OsvClientError::NotFound));
    }
}
