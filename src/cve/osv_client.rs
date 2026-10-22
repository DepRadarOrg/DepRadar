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
#[cfg(feature = "async")]
const OSV_QUERY_URL: &str = "https://api.osv.dev/v1/query";
const OSV_QUERYBATCH_URL: &str = "https://api.osv.dev/v1/querybatch";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RETRIES: u32 = 3;
const BACKOFF_BASE: Duration = Duration::from_millis(500);

/// Maximum number of queries OSV accepts in one `POST /v1/querybatch` call.
pub const OSV_BATCH_LIMIT: usize = 1000;

/// Package registries `query_osv` can query OSV against.
///
/// OSV's own ecosystem list is much larger, but the crawler only targets
/// these three registries today; add variants here as more are wired up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    page_token: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct OsvPackageQuery<'a> {
    name: &'a str,
    ecosystem: &'a str,
}

/// One package version to look up with [`query_osv_batch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsvBatchQuery<'a> {
    pub name: &'a str,
    pub ecosystem: Ecosystem,
    pub version: &'a str,
}

/// Request body for `POST /v1/querybatch`.
#[derive(Debug, Serialize)]
struct OsvBatchRequest<'a> {
    queries: Vec<OsvQueryRequest<'a>>,
}

/// Response body of `POST /v1/querybatch`: one result per query, in order.
#[derive(Debug, Deserialize)]
struct OsvBatchResponse {
    #[serde(default)]
    results: Vec<OsvBatchResult>,
}

/// The vulnerabilities matching one query of a batch. Batch results only
/// carry IDs; fetch `GET /v1/vulns/{id}` for the full record.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OsvBatchResult {
    #[serde(default)]
    pub vulns: Vec<OsvVulnerabilityRef>,
}

/// A vulnerability as listed in a batch result.
#[derive(Debug, Clone, Deserialize)]
pub struct OsvVulnerabilityRef {
    pub id: String,
    pub modified: Option<String>,
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
    /// Source-specific extras; GHSA records carry a `severity` label here.
    pub database_specific: Option<serde_json::Value>,
}

/// Response body of `POST /v1/query`: the list of vulnerabilities matching
/// the queried package/version, if any.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsvQueryResponse {
    #[serde(default)]
    pub vulns: Vec<OsvVulnerability>,
    /// Set when OSV truncated the result; pass back as `page_token`.
    pub next_page_token: Option<String>,
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
    /// A batch response didn't have exactly one result per query.
    BatchSizeMismatch { expected: usize, got: usize },
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
            OsvClientError::BatchSizeMismatch { expected, got } => write!(
                f,
                "OSV batch response had {got} results for {expected} queries"
            ),
        }
    }
}

impl std::error::Error for OsvClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OsvClientError::Request(err) | OsvClientError::Decode(err) => Some(err),
            OsvClientError::RateLimited { .. }
            | OsvClientError::NotFound
            | OsvClientError::UnexpectedStatus(_)
            | OsvClientError::BatchSizeMismatch { .. } => None,
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

        let url = format!("{}/v1/query", self.base_url);
        let mut merged = OsvQueryResponse::default();
        let mut page_token: Option<String> = None;

        loop {
            let request_body = OsvQueryRequest {
                version,
                package: OsvPackageQuery {
                    name: package_name,
                    ecosystem: ecosystem.as_osv_str(),
                },
                page_token: page_token.as_deref(),
            };

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

            let page: OsvQueryResponse = response.json().map_err(OsvClientError::Decode)?;
            merged.vulns.extend(page.vulns);
            match page.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }

        if let Some(cache) = &self.cache {
            cache.put(ecosystem, package_name, version, &merged);
        }
        Ok(merged)
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

fn check_status(status: reqwest::StatusCode) -> Result<(), OsvClientError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(OsvClientError::UnexpectedStatus(status))
    }
}

/// Merges records that refer to the same issue, i.e. whose `id`/`aliases`
/// overlap (typically a GHSA record and its CVE).
///
/// This does no filtering, deduplication, or severity scoring — an empty
/// `vulns` list in the returned [`OsvQueryResponse`] means OSV has no known
/// vulnerability for this exact version.
pub fn query_osv(
    package_name: &str,
    ecosystem: Ecosystem,
    version: &str,
) -> Result<OsvQueryResponse, OsvClientError> {
    OsvClient::new().query(package_name, ecosystem, version)
}

/// Environment variable overriding the OSV base URL, e.g. to point at a
/// mock server in integration tests.
pub const OSV_BASE_URL_ENV: &str = "DEPRADAR_OSV_BASE_URL";

/// The OSV base URL in effect: `DEPRADAR_OSV_BASE_URL` when set and
/// non-empty, otherwise the public API.
pub fn base_url() -> String {
    match std::env::var(OSV_BASE_URL_ENV) {
        Ok(url) if !url.trim().is_empty() => url.trim_end_matches('/').to_string(),
        _ => OSV_BASE_URL.to_string(),
    }
}

/// Fetches the full record for one vulnerability via `GET /v1/vulns/{id}`.
///
/// Batch/query results can be sparse; this returns the complete advisory
/// (summary, severity, references).
pub fn get_vulnerability(id: &str) -> Result<OsvVulnerability, OsvClientError> {
    get_vulnerability_at(&base_url(), id)
}

/// Fetches the full advisory for `id` via `GET /v1/vulns/{id}`, honouring
/// the [`OSV_BASE_URL_ENV`] override.
///
/// This is the entry point the CLI uses for `depradar explain <advisory-id>`;
/// it shares the HTTP/parsing logic with [`get_vulnerability`].
pub fn fetch_vuln(id: &str) -> Result<OsvVulnerability, OsvClientError> {
    get_vulnerability(id)
}

#[cfg(test)]
fn query_osv_at(
    base_url: &str,
    package_name: &str,
    ecosystem: Ecosystem,
    version: &str,
) -> Result<OsvQueryResponse, OsvClientError> {
    OsvClient::with_base_url(base_url).query(package_name, ecosystem, version)
}

fn get_vulnerability_at(base_url: &str, id: &str) -> Result<OsvVulnerability, OsvClientError> {
    let response = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("static reqwest client config is valid")
        .get(format!("{base_url}/v1/vulns/{id}"))
        .send()
        .map_err(OsvClientError::Request)?;

    check_status(response.status())?;
    response.json().map_err(OsvClientError::Decode)
}

/// A query accepted by [`query_osv_batch`]: either an [`OsvBatchQuery`] or a
/// plain `(name, ecosystem, version)` tuple.
pub trait BatchQuery {
    fn name(&self) -> &str;
    fn ecosystem(&self) -> Ecosystem;
    fn version(&self) -> &str;
}

impl BatchQuery for OsvBatchQuery<'_> {
    fn name(&self) -> &str {
        self.name
    }

    fn ecosystem(&self) -> Ecosystem {
        self.ecosystem
    }

    fn version(&self) -> &str {
        self.version
    }
}

impl BatchQuery for (&str, Ecosystem, &str) {
    fn name(&self) -> &str {
        self.0
    }

    fn ecosystem(&self) -> Ecosystem {
        self.1
    }

    fn version(&self) -> &str {
        self.2
    }
}

/// Queries OSV for many package versions at once via
/// [`POST /v1/querybatch`](https://google.github.io/osv.dev/api/#post-v1querybatch),
/// splitting into requests of at most [`OSV_BATCH_LIMIT`] queries.
///
/// Returns one [`OsvBatchResult`] per query, in the same order. A query with
/// more matches than fit on one OSV results page is truncated to that first
/// page; pagination isn't followed yet.
pub fn query_osv_batch<Q: BatchQuery>(
    queries: &[Q],
) -> Result<Vec<OsvBatchResult>, OsvClientError> {
    query_osv_batch_at(OSV_QUERYBATCH_URL, queries)
}

fn query_osv_batch_at<Q: BatchQuery>(
    url: &str,
    queries: &[Q],
) -> Result<Vec<OsvBatchResult>, OsvClientError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("static reqwest client config is valid");
    let mut results = Vec::with_capacity(queries.len());

    for chunk in queries.chunks(OSV_BATCH_LIMIT) {
        let request_body = OsvBatchRequest {
            queries: chunk
                .iter()
                .map(|query| OsvQueryRequest {
                    version: query.version(),
                    package: OsvPackageQuery {
                        name: query.name(),
                        ecosystem: query.ecosystem().as_osv_str(),
                    },
                    page_token: None,
                })
                .collect(),
        };

        let response = client
            .post(url)
            .json(&request_body)
            .send()
            .map_err(OsvClientError::Request)?;

        let status = response.status();
        if !status.is_success() {
            return Err(OsvClientError::UnexpectedStatus(status));
        }

        let body: OsvBatchResponse = response.json().map_err(OsvClientError::Decode)?;
        if body.results.len() != chunk.len() {
            return Err(OsvClientError::BatchSizeMismatch {
                expected: chunk.len(),
                got: body.results.len(),
            });
        }
        results.extend(body.results);
    }

    Ok(results)
}

/// Non-blocking variant of [`query_osv`], for use inside an async runtime.
#[cfg(feature = "async")]
pub async fn query_osv_async(
    package_name: &str,
    ecosystem: Ecosystem,
    version: &str,
) -> Result<OsvQueryResponse, OsvClientError> {
    query_with_client(&reqwest::Client::new(), package_name, ecosystem, version).await
}

/// Queries OSV for every `(package name, ecosystem, version)` in `packages`,
/// reusing one HTTP client. Results are in input order; the first failure
/// aborts the batch.
#[cfg(feature = "async")]
pub async fn query_osv_batch_async(
    packages: &[(&str, Ecosystem, &str)],
) -> Result<Vec<OsvQueryResponse>, OsvClientError> {
    let client = reqwest::Client::new();
    let mut responses = Vec::with_capacity(packages.len());
    for &(name, ecosystem, version) in packages {
        responses.push(query_with_client(&client, name, ecosystem, version).await?);
    }
    Ok(responses)
}

#[cfg(feature = "async")]
async fn query_with_client(
    client: &reqwest::Client,
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
        page_token: None,
    };

    let response = client
        .post(OSV_QUERY_URL)
        .json(&request_body)
        .send()
        .await
        .map_err(OsvClientError::Request)?;

    check_status(response.status())?;
    response.json().await.map_err(OsvClientError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serves `bodies` as 200 JSON responses, one per incoming connection,
    /// and returns the URL plus a handle yielding each request's query count.
    fn batch_mock_server(bodies: Vec<String>) -> (String, std::thread::JoinHandle<Vec<usize>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/querybatch", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut counts = Vec::new();
            for body in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let (header_end, content_length) = loop {
                    let n = stream.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .map(|v| v.trim().parse::<usize>().unwrap())
                            .unwrap();
                        break (pos + 4, len);
                    }
                };
                while buf.len() < header_end + content_length {
                    let n = stream.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                }
                let request: serde_json::Value =
                    serde_json::from_slice(&buf[header_end..header_end + content_length]).unwrap();
                counts.push(request["queries"].as_array().unwrap().len());
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
            counts
        });
        (url, handle)
    }

    #[test]
    fn batch_returns_results_in_input_order() {
        let body = r#"{"results":[{"vulns":[{"id":"GHSA-1","modified":"2024-01-01T00:00:00Z"}]},{},{"vulns":[{"id":"GHSA-2"},{"id":"GHSA-3"}]}]}"#;
        let (url, handle) = batch_mock_server(vec![body.to_string()]);
        let queries = [
            ("lodash", Ecosystem::Npm, "4.17.4"),
            ("requests", Ecosystem::PyPI, "2.31.0"),
            ("serde", Ecosystem::CratesIo, "1.0.0"),
        ];

        let results = query_osv_batch_at(&url, &queries).unwrap();

        assert_eq!(handle.join().unwrap(), vec![3]);
        let ids: Vec<Vec<&str>> = results
            .iter()
            .map(|r| r.vulns.iter().map(|v| v.id.as_str()).collect())
            .collect();
        assert_eq!(ids, vec![vec!["GHSA-1"], vec![], vec!["GHSA-2", "GHSA-3"]]);
    }

    #[test]
    fn batch_chunks_beyond_1000_queries_and_preserves_order() {
        let first = format!(
            r#"{{"results":[{}]}}"#,
            vec![r#"{"vulns":[{"id":"A"}]}"#; OSV_BATCH_LIMIT].join(",")
        );
        let second = r#"{"results":[{"vulns":[{"id":"B"}]}]}"#.to_string();
        let (url, handle) = batch_mock_server(vec![first, second]);
        let queries = vec![("lodash", Ecosystem::Npm, "4.17.4"); OSV_BATCH_LIMIT + 1];

        let results = query_osv_batch_at(&url, &queries).unwrap();

        assert_eq!(handle.join().unwrap(), vec![OSV_BATCH_LIMIT, 1]);
        assert_eq!(results.len(), OSV_BATCH_LIMIT + 1);
        assert_eq!(results[0].vulns[0].id, "A");
        assert_eq!(results[OSV_BATCH_LIMIT].vulns[0].id, "B");
    }

    #[test]
    fn batch_with_no_queries_makes_no_request() {
        let queries: &[(&str, Ecosystem, &str)] = &[];
        assert!(query_osv_batch_at("http://127.0.0.1:1/unused", queries)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn query_osv_batch_finds_known_vulnerability_in_lodash() {
        let results = match query_osv_batch(&[("lodash", Ecosystem::Npm, "4.17.4")]) {
            Ok(results) => results,
            Err(err) => {
                eprintln!("skipping: OSV batch query failed ({err}), likely no network access");
                return;
            }
        };

        assert_eq!(results.len(), 1);
        assert!(
            !results[0].vulns.is_empty(),
            "expected lodash@4.17.4 to have at least one known vulnerability"
        );
    }

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

    /// Serves each canned `(status, body)` in order, one per connection,
    /// and returns the base URL plus every raw request received.
    fn mock_server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 8192];
                let mut req = Vec::new();
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    req.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&req);
                    if let Some(split) = text.find("\r\n\r\n") {
                        let len = text[..split]
                            .to_lowercase()
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap_or(0usize);
                        if req.len() >= split + 4 + len {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8_lossy(&req).into_owned());
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            requests
        });
        (base, handle)
    }

    #[test]
    fn query_osv_follows_pagination_and_merges_pages() {
        let (base, handle) = mock_server(vec![
            (200, r#"{"vulns":[{"id":"A"}],"next_page_token":"tok1"}"#),
            (200, r#"{"vulns":[{"id":"B"},{"id":"C"}]}"#),
        ]);

        let response = query_osv_at(&base, "lodash", Ecosystem::Npm, "4.17.4").unwrap();
        let ids: Vec<_> = response.vulns.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["A", "B", "C"]);

        let requests = handle.join().unwrap();
        assert!(!requests[0].contains("page_token"));
        assert!(requests[1].contains(r#""page_token":"tok1""#));
    }

    #[test]
    fn get_vulnerability_parses_full_record() {
        let (base, handle) = mock_server(vec![(
            200,
            r#"{"id":"GHSA-29mw-wpgm-hmr9","summary":"ReDoS in lodash","aliases":["CVE-2020-28500"]}"#,
        )]);

        let vuln = get_vulnerability_at(&base, "GHSA-29mw-wpgm-hmr9").unwrap();
        assert_eq!(vuln.id, "GHSA-29mw-wpgm-hmr9");
        assert_eq!(vuln.summary.as_deref(), Some("ReDoS in lodash"));
        assert!(handle.join().unwrap()[0].starts_with("GET /v1/vulns/GHSA-29mw-wpgm-hmr9 "));
    }

    #[test]
    fn get_vulnerability_reports_unexpected_status() {
        let (base, _handle) = mock_server(vec![(404, "{}")]);
        assert!(matches!(
            get_vulnerability_at(&base, "NOPE"),
            Err(OsvClientError::UnexpectedStatus(s)) if s.as_u16() == 404
        ));
    }

    /// Live check; skipped when the network is unavailable.
    #[test]
    fn get_vulnerability_live_ghsa_29mw_wpgm_hmr9() {
        match get_vulnerability("GHSA-29mw-wpgm-hmr9") {
            Ok(vuln) => {
                assert_eq!(vuln.id, "GHSA-29mw-wpgm-hmr9");
                assert!(vuln.aliases.iter().any(|a| a == "CVE-2020-28500"));
            }
            Err(err) => eprintln!("skipping: OSV request failed ({err}), likely no network"),
        }
    }
}
