//! EPSS (Exploit Prediction Scoring System) client.
//!
//! Queries the FIRST EPSS API for exploit-probability scores of CVE aliases
//! and caches results in the OSV cache directory keyed by CVE ID with a TTL.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Default EPSS API endpoint.
pub const DEFAULT_EPSS_API: &str = "https://api.first.org/data/v1/epss";

/// Default cache TTL for EPSS scores (24 hours).
pub const DEFAULT_EPSS_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// A single EPSS score entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpssScore {
    /// The CVE identifier.
    pub cve: String,
    /// The EPSS probability (0.0 - 1.0).
    pub epss: f64,
    /// The percentile ranking (0.0 - 1.0).
    #[serde(default)]
    pub percentile: Option<f64>,
}

/// Raw API response from the EPSS endpoint.
#[derive(Debug, Deserialize)]
struct EpssResponse {
    #[serde(default)]
    data: Vec<EpssRawEntry>,
}

/// A single raw entry in the EPSS API response.
#[derive(Debug, Deserialize)]
struct EpssRawEntry {
    cve: String,
    epss: String,
    #[serde(default)]
    percentile: Option<String>,
}

/// On-disk cache entry for a single CVE.
#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    /// Unix timestamp (seconds) when the entry was fetched.
    fetched_at: u64,
    /// The cached score, if any.
    score: Option<EpssScore>,
}

/// EPSS client with caching support.
#[derive(Debug, Clone)]
pub struct EpssClient {
    api_url: String,
    cache_dir: PathBuf,
    ttl: Duration,
}

impl EpssClient {
    /// Create a new EPSS client.
    ///
    /// `cache_dir` is the directory used to store cached scores (typically the
    /// OSV cache directory).
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            api_url: DEFAULT_EPSS_API.to_string(),
            cache_dir: cache_dir.into(),
            ttl: DEFAULT_EPSS_TTL,
        }
    }

    /// Override the API URL (useful for tests).
    pub fn with_api_url(mut self, api_url: impl Into<String>) -> Self {
        self.api_url = api_url.into();
        self
    }

    /// Override the cache TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Return the cache directory used by this client.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    fn cache_path(&self, cve: &str) -> PathBuf {
        self.cache_dir.join("epss").join(format!("{cve}.json"))
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn read_cache(&self, cve: &str) -> Option<Option<EpssScore>> {
        let path = self.cache_path(cve);
        let contents = std::fs::read_to_string(&path).ok()?;
        let entry: CacheEntry = serde_json::from_str(&contents).ok()?;
        let now = Self::now_secs();
        if now.saturating_sub(entry.fetched_at) > self.ttl.as_secs() {
            return None;
        }
        Some(entry.score)
    }

    fn write_cache(&self, cve: &str, score: Option<&EpssScore>) {
        let path = self.cache_path(cve);
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        let entry = CacheEntry {
            fetched_at: Self::now_secs(),
            score: score.cloned(),
        };
        if let Ok(serialized) = serde_json::to_string(&entry) {
            let _ = std::fs::write(&path, serialized);
        }
    }

    /// Fetch EPSS scores for the given CVE IDs.
    ///
    /// Returns a map from CVE ID to score. CVEs without a score are omitted.
    /// Results are cached on disk keyed by CVE ID.
    pub fn fetch_scores(&self, cves: &[String]) -> HashMap<String, EpssScore> {
        let mut result = HashMap::new();
        let mut to_fetch: Vec<String> = Vec::new();

        for cve in cves {
            match self.read_cache(cve) {
                Some(Some(score)) => {
                    result.insert(cve.clone(), score);
                }
                Some(None) => {
                    // Cached negative result.
                }
                None => to_fetch.push(cve.clone()),
            }
        }

        if to_fetch.is_empty() {
            return result;
        }

        let fetched = self.fetch_from_api(&to_fetch);

        for cve in &to_fetch {
            let score = fetched.get(cve).cloned();
            self.write_cache(cve, score.as_ref());
            if let Some(score) = score {
                result.insert(cve.clone(), score);
            }
        }

        result
    }

    fn fetch_from_api(&self, cves: &[String]) -> HashMap<String, EpssScore> {
        let mut map = HashMap::new();
        if cves.is_empty() {
            return map;
        }

        let query = cves
            .iter()
            .map(|c| format!("cve={}", urlencode(c)))
            .collect::<Vec<_>>()
            .join("&");
        let url = format!("{}?{}", self.api_url, query);

        let body = match http_get(&url) {
            Ok(body) => body,
            Err(_) => return map,
        };

        let response: EpssResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(_) => return map,
        };

        for entry in response.data {
            if let Ok(epss) = entry.epss.parse::<f64>() {
                let percentile = entry
                    .percentile
                    .as_deref()
                    .and_then(|p| p.parse::<f64>().ok());
                map.insert(
                    entry.cve.clone(),
                    EpssScore {
                        cve: entry.cve,
                        epss,
                        percentile,
                    },
                );
            }
        }

        map
    }

    /// Look up the EPSS score for a vulnerability's CVE aliases.
    ///
    /// Returns the highest-scoring EPSS entry among the provided aliases, if
    /// any alias resolves to a CVE with a known score.
    pub fn lookup_for_aliases(&self, aliases: &[String]) -> Option<EpssScore> {
        let cves: Vec<String> = aliases
            .iter()
            .filter(|a| a.starts_with("CVE-"))
            .cloned()
            .collect();
        if cves.is_empty() {
            return None;
        }

        let scores = self.fetch_scores(&cves);
        scores
            .into_values()
            .max_by(|a, b| {
                a.epss
                    .partial_cmp(&b.epss)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

/// Convenience function to look up the EPSS score for a vulnerability's CVE
/// aliases using a client rooted at `cache_dir`.
pub fn lookup_epss_for_aliases(
    cache_dir: impl Into<PathBuf>,
    aliases: &[String],
) -> Option<EpssScore> {
    EpssClient::new(cache_dir).lookup_for_aliases(aliases)
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Perform a minimal blocking HTTP GET and return the response body.
fn http_get(url: &str) -> Result<String, String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("invalid url: {url}"))?;
    if scheme != "http" {
        return Err(format!("unsupported scheme: {scheme}"));
    }
    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let host_port = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };

    let mut stream =
        TcpStream::connect(&host_port).map_err(|e| format!("connect failed: {e}"))?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {authority}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write failed: {e}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("read failed: {e}"))?;

    let response = String::from_utf8_lossy(&response).into_owned();
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed response".to_string())?;

    let status_line = headers.lines().next().unwrap_or("");
    if !status_line.contains(" 200 ") {
        return Err(format!("unexpected status: {status_line}"));
    }

    // Handle chunked transfer encoding if present.
    let chunked = headers
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked"));
    if chunked {
        Ok(decode_chunked(body))
    } else {
        Ok(body.to_string())
    }
}

fn decode_chunked(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    loop {
        let Some(idx) = rest.find("\r\n") else {
            break;
        };
        let size_str = &rest[..idx];
        let size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let start = idx + 2;
        let end = start + size;
        if end > rest.len() {
            out.push_str(&rest[start..]);
            break;
        }
        out.push_str(&rest[start..end]);
        rest = &rest[end..];
        if rest.starts_with("\r\n") {
            rest = &rest[2..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn temp_cache_dir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("depradar-epss-test-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_score_from_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = r#"{"status":"OK","status-code":200,"data":[{"cve":"CVE-2021-44228","epss":"0.97000","percentile":"0.99900"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let cache_dir = temp_cache_dir();
        let client = EpssClient::new(&cache_dir)
            .with_api_url(format!("http://{addr}/data/v1/epss"));

        let aliases = vec!["CVE-2021-44228".to_string()];
        let score = client.lookup_for_aliases(&aliases).expect("score");
        assert_eq!(score.cve, "CVE-2021-44228");
        assert!((score.epss - 0.97).abs() < 1e-9);
        assert!((score.percentile.unwrap() - 0.999).abs() < 1e-9);

        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&cache_dir);
    }

    #[test]
    fn uses_cache_on_second_lookup() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = r#"{"data":[{"cve":"CVE-2020-0001","epss":"0.50000","percentile":"0.90000"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let cache_dir = temp_cache_dir();
        let client = EpssClient::new(&cache_dir)
            .with_api_url(format!("http://{addr}/data/v1/epss"));

        let aliases = vec!["CVE-2020-0001".to_string()];
        let first = client.lookup_for_aliases(&aliases).expect("first");
        assert!((first.epss - 0.5).abs() < 1e-9);

        // Second lookup should hit the cache; the server thread has exited.
        let second = client.lookup_for_aliases(&aliases).expect("second");
        assert!((second.epss - 0.5).abs() < 1e-9);

        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&cache_dir);
    }

    #[test]
    fn ignores_non_cve_aliases() {
        let cache_dir = temp_cache_dir();
        let client = EpssClient::new(&cache_dir);
        let aliases = vec!["GHSA-xxxx-yyyy-zzzz".to_string()];
        assert!(client.lookup_for_aliases(&aliases).is_none());
        let _ = std::fs::remove_dir_all(&cache_dir);
    }
}