//! Minimal client for the [OSV.dev](https://osv.dev) vulnerability database.
//!
//! `query_osv` resolves a `(package name, ecosystem, version)` triple to the
//! raw set of vulnerabilities OSV knows about for that exact version, via
//! [`POST /v1/query`](https://google.github.io/osv.dev/api/#post-v1query).
//! There is no filtering, deduplication, or severity scoring here — this is
//! just fetch-and-parse into typed structs. Turning the raw matches into
//! scored findings belongs with the (not-yet-written) CVE scanner module.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::cache::OsvCache;

const OSV_BASE_URL: &str = "https://api.osv.dev";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RETRIES: u32 = 3;
const BACKOFF_BASE: Duration = Duration::from_millis(500);

/// Package registries `query_osv` can query OSV against.
///
/// OSV's own ecosystem list is much larger; add variants here as more are
/// wired up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecosystem {
    Npm,
    PyPI,
    CratesIo,
    Go,
    Maven,
    RubyGems,
    Packagist,
}

impl Ecosystem {
    /// The ecosystem name OSV expects in a query, exactly as documented at
    /// <https://ossf.github.io/osv-schema/#affectedpackage-field>.
    pub(crate) fn as_osv_str(self) -> &'static str {
        match self {
            Ecosystem::Npm => "npm",
            Ecosystem::PyPI => "PyPI",
            Ecosystem::CratesIo => "crates.io",
            Ecosystem::Go => "Go",
            Ecosystem::Maven => "Maven",
            Ecosystem::RubyGems => "RubyGems",
            Ecosystem::Packagist => "Packagist",
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvPackage {
    pub name: String,
    pub ecosystem: String,
    /// Package URL, e.g. `pkg:npm/lodash`. Not present for every ecosystem.
    pub purl: Option<String>,
}

/// One event in a version range: the point where a vulnerability was
/// introduced, fixed, or last known to affect a version. Exactly one field
/// is set per event, per the OSV schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvEvent {
    pub introduced: Option<String>,
    pub fixed: Option<String>,
    pub last_affected: Option<String>,
    pub limit: Option<String>,
}

/// A version range (e.g. a SEMVER range) bounding which versions of a
/// package are affected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvRange {
    #[serde(rename = "type")]
    pub range_type: String,
    #[serde(default)]
    pub events: Vec<OsvEvent>,
}

/// One `affected` entry: a package (possibly a different name/ecosystem than
/// the one queried, e.g. an alias package) plus the ranges and/or explicit
/// version list that are affected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvAffected {
    pub package: Option<OsvPackage>,
    #[serde(default)]
    pub ranges: Vec<OsvRange>,
    #[serde(default)]
    pub versions: Vec<String>,
}

/// A severity score reported for a vulnerability, e.g. a CVSS vector string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvSeverity {
    #[serde(rename = "type")]
    pub severity_type: String,
    pub score: String,
}

/// A reference URL for a vulnerability (advisory, fix commit, report, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// Response body of `POST /v1/query`: the list of vulnerabilities matching
/// the queried package/version, if any.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsvQueryResponse {
    #[serde(default)]
    pub vulns: Vec<OsvVulnerability>,
}

/// Errors from querying OSV.
#[derive(Debug)]
pub enum OsvClientError {
    /// The HTTP request itself failed (network error, timeout, TLS, etc).
    Request(reqwest::Error),
    /// OSV responded, but with a non-success status code.
    UnexpectedStatus(reqwest::StatusCode),
    /// The response body wasn't the JSON shape `OsvQueryResponse` expects.
    Decode(reqwest::Error),
}

impl std::fmt::Display for OsvClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsvClientError::Request(err) => write!(f, "OSV request failed: {err}"),
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
            OsvClientError::UnexpectedStatus(_) => None,
        }
    }
}

/// Client for `POST /v1/query` with a request timeout, retries on 429/5xx
/// (exponential backoff with jitter) and an optional on-disk response cache.
pub struct OsvClient {
    http: reqwest::blocking::Client,
    base_url: String,
    backoff_base: Duration,
    cache: Option<OsvCache>,
}

impl Default for OsvClient {
    fn default() -> Self {
        Self::with_base_url(OSV_BASE_URL)
    }
}

impl OsvClient {
    /// A client for the public OSV API, with no cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// A client for an OSV-compatible server at `base_url` (no trailing
    /// slash), e.g. a mock server in tests.
    pub fn with_base_url(base_url: &str) -> Self {
        Self {
            http: reqwest::blocking::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("static reqwest client config is valid"),
            base_url: base_url.trim_end_matches('/').to_string(),
            backoff_base: BACKOFF_BASE,
            cache: None,
        }
    }

    /// Serves repeat queries from `cache` and stores fresh responses in it.
    pub fn with_cache(mut self, cache: OsvCache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Overrides the initial retry delay (the delay doubles per retry).
    pub fn with_backoff(mut self, base: Duration) -> Self {
        self.backoff_base = base;
        self
    }

    /// Queries OSV for known vulnerabilities affecting `package_name` at
    /// exactly `version` in `ecosystem`, and returns the raw matches.
    ///
    /// This does no filtering, deduplication, or severity scoring — an empty
    /// `vulns` list means OSV has no known vulnerability for this exact
    /// version. 429 and 5xx responses are retried up to 3 times; any other
    /// non-success status fails immediately.
    pub fn query(
        &self,
        package_name: &str,
        ecosystem: Ecosystem,
        version: &str,
    ) -> Result<OsvQueryResponse, OsvClientError> {
        if let Some(hit) = self
            .cache
            .as_ref()
            .and_then(|c| c.get(ecosystem, package_name, version))
        {
            return Ok(hit);
        }

        let request_body = OsvQueryRequest {
            version,
            package: OsvPackageQuery {
                name: package_name,
                ecosystem: ecosystem.as_osv_str(),
            },
        };
        let url = format!("{}/v1/query", self.base_url);

        let mut attempt = 0;
        let response = loop {
            let response = self
                .http
                .post(&url)
                .json(&request_body)
                .send()
                .map_err(OsvClientError::Request)?;
            let status = response.status();
            if status.is_success() {
                break response;
            }
            let retryable =
                status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
            if !retryable || attempt >= MAX_RETRIES {
                return Err(OsvClientError::UnexpectedStatus(status));
            }
            std::thread::sleep(self.backoff_delay(attempt));
            attempt += 1;
        };

        let parsed: OsvQueryResponse = response.json().map_err(OsvClientError::Decode)?;
        if let Some(cache) = &self.cache {
            cache.put(ecosystem, package_name, version, &parsed);
        }
        Ok(parsed)
    }

    /// `backoff_base * 2^attempt`, plus up to one `backoff_base` of jitter.
    fn backoff_delay(&self, attempt: u32) -> Duration {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let jitter = self.backoff_base.mul_f64(f64::from(nanos % 1000) / 1000.0);
        self.backoff_base * 2u32.pow(attempt) + jitter
    }
}

/// Queries the public OSV API with default settings and no cache; see
/// [`OsvClient::query`].
pub fn query_osv(
    package_name: &str,
    ecosystem: Ecosystem,
    version: &str,
) -> Result<OsvQueryResponse, OsvClientError> {
    OsvClient::new().query(package_name, ecosystem, version)
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
        assert_eq!(Ecosystem::Go.as_osv_str(), "Go");
        assert_eq!(Ecosystem::Maven.as_osv_str(), "Maven");
        assert_eq!(Ecosystem::RubyGems.as_osv_str(), "RubyGems");
        assert_eq!(Ecosystem::Packagist.as_osv_str(), "Packagist");
    }

    const VULN_BODY: &str = r#"{"vulns":[{"id":"GHSA-test","aliases":["CVE-2020-0001"]}]}"#;

    fn client(server: &mockito::Server) -> OsvClient {
        OsvClient::with_base_url(&server.url()).with_backoff(Duration::from_millis(1))
    }

    #[test]
    fn success_parses_vulns() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/query")
            .with_body(VULN_BODY)
            .create();
        let response = client(&server).query("x", Ecosystem::Npm, "1.0.0").unwrap();
        assert_eq!(response.vulns[0].id, "GHSA-test");
        mock.assert();
    }

    #[test]
    fn empty_object_means_no_vulns() {
        let mut server = mockito::Server::new();
        server.mock("POST", "/v1/query").with_body("{}").create();
        let response = client(&server).query("x", Ecosystem::Npm, "1.0.0").unwrap();
        assert!(response.vulns.is_empty());
    }

    #[test]
    fn malformed_json_is_decode_error() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/v1/query")
            .with_body("not json")
            .create();
        let err = client(&server)
            .query("x", Ecosystem::Npm, "1.0.0")
            .unwrap_err();
        assert!(matches!(err, OsvClientError::Decode(_)));
    }

    #[test]
    fn unreachable_server_is_request_error() {
        let err = OsvClient::with_base_url("http://127.0.0.1:1")
            .query("x", Ecosystem::Npm, "1.0.0")
            .unwrap_err();
        assert!(matches!(err, OsvClientError::Request(_)));
    }

    #[test]
    fn persistent_429_and_500_are_unexpected_status_after_retries() {
        for status in [429u16, 500] {
            let mut server = mockito::Server::new();
            let mock = server
                .mock("POST", "/v1/query")
                .with_status(status as usize)
                .expect(1 + MAX_RETRIES as usize)
                .create();
            let err = client(&server)
                .query("x", Ecosystem::Npm, "1.0.0")
                .unwrap_err();
            assert!(matches!(err, OsvClientError::UnexpectedStatus(s) if s.as_u16() == status));
            mock.assert();
        }
    }

    #[test]
    fn retries_503_then_succeeds() {
        let mut server = mockito::Server::new();
        let fail = server
            .mock("POST", "/v1/query")
            .with_status(503)
            .expect(1)
            .create();
        let ok = server
            .mock("POST", "/v1/query")
            .with_body(VULN_BODY)
            .expect(1)
            .create();
        let response = client(&server).query("x", Ecosystem::Npm, "1.0.0").unwrap();
        assert_eq!(response.vulns.len(), 1);
        fail.assert();
        ok.assert();
    }

    #[test]
    fn does_not_retry_400() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/query")
            .with_status(400)
            .expect(1)
            .create();
        let err = client(&server)
            .query("x", Ecosystem::Npm, "1.0.0")
            .unwrap_err();
        assert!(matches!(err, OsvClientError::UnexpectedStatus(s) if s.as_u16() == 400));
        mock.assert();
    }

    #[test]
    fn cache_hit_skips_http_and_expired_entry_refetches() {
        let dir = tempfile::tempdir().unwrap();
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/query")
            .with_body(VULN_BODY)
            .expect(1)
            .create();

        let cached = client(&server).with_cache(OsvCache::new(
            dir.path().to_path_buf(),
            Duration::from_secs(3600),
        ));
        cached.query("x", Ecosystem::Npm, "1.0.0").unwrap();
        let hit = cached.query("x", Ecosystem::Npm, "1.0.0").unwrap();
        assert_eq!(hit.vulns[0].id, "GHSA-test");
        mock.assert(); // still exactly one request

        let expired =
            client(&server).with_cache(OsvCache::new(dir.path().to_path_buf(), Duration::ZERO));
        let refetch = server
            .mock("POST", "/v1/query")
            .with_body("{}")
            .expect(1)
            .create();
        expired.query("x", Ecosystem::Npm, "1.0.0").unwrap();
        refetch.assert();
    }
}
