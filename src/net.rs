//! Shared polite HTTP client for every crawler.
//!
//! Crawling package registries means talking to servers that ask (or require)
//! callers to be good neighbours, so the rules are implemented once here
//! instead of per crawler:
//!
//! * a descriptive User-Agent that identifies DepRadar and links the project,
//! * a request timeout, so a hung server can't stall a crawl forever, and
//! * a per-host token-bucket rate limit ([`TokenBucket`], capacity one:
//!   one token refills every `per_host_rate`, so at most one request per
//!   `per_host_rate` per host, keyed by the host of the URL we build).
//!
//! The client is blocking ([`reqwest::blocking`]), matching the rest of the
//! off-chain toolkit; the async OSV helpers behind the `async` feature are
//! the exception, not the rule.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;

/// Default rate limit: one request per second per host.
pub const DEFAULT_PER_HOST_RATE: Duration = Duration::from_secs(1);
/// Default request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
/// Descriptive User-Agent: what we are, where we come from, how to reach us.
///
/// crates.io's crawler policy (<https://crates.io/policies>) requires a
/// User-Agent that names the tool and a way to contact its operator; keep
/// this honest if you fork or redeploy the crawler.
pub const DEFAULT_USER_AGENT: &str = concat!(
    "DepRadar/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/DepRadarOrg/DepRadar; dependency-security crawler)"
);

/// Errors from [`PoliteClient`].
#[derive(Debug)]
pub enum NetError {
    /// The URL handed to [`PoliteClient::get`] didn't parse.
    InvalidUrl { url: String },
    /// The request itself failed (network error, timeout, TLS, etc).
    Request(reqwest::Error),
    /// The response body wasn't the JSON shape asked for.
    Decode(reqwest::Error),
    /// A non-success status code.
    Status(reqwest::StatusCode),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::InvalidUrl { url } => write!(f, "invalid URL: {url}"),
            NetError::Request(err) => write!(f, "request failed: {err}"),
            NetError::Decode(err) => write!(f, "failed to decode response: {err}"),
            NetError::Status(status) => write!(f, "unexpected status: {status}"),
        }
    }
}

impl std::error::Error for NetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NetError::Request(err) | NetError::Decode(err) => Some(err),
            NetError::InvalidUrl { .. } | NetError::Status(_) => None,
        }
    }
}

impl From<reqwest::Error> for NetError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_decode() {
            NetError::Decode(err)
        } else {
            NetError::Request(err)
        }
    }
}

/// A token bucket of capacity one: `tokens` refills at `1 / period` per
/// second, and a request consumes the single token.
///
/// Capacity one (rather than a burst) is deliberate — registries ask for a
/// steady polite rate, not a burst followed by a stall.
#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn full(now: Instant) -> Self {
        TokenBucket {
            tokens: 1.0,
            last_refill: now,
        }
    }

    /// Refills for elapsed time and takes a token if one is available,
    /// returning how long to sleep until one will be instead.
    fn take_or_delay(&mut self, period: Duration, now: Instant) -> Option<Duration> {
        let elapsed = now.saturating_duration_since(self.last_refill);
        self.last_refill = now;
        self.tokens = (self.tokens + elapsed.as_secs_f64() / period.as_secs_f64()).min(1.0);
        if self.tokens >= 1.0 {
            self.tokens = 0.0;
            None
        } else {
            Some(Duration::from_secs_f64(
                (1.0 - self.tokens) * period.as_secs_f64(),
            ))
        }
    }
}

/// Blocking HTTP client with a per-host rate limit, a descriptive User-Agent
/// and a request timeout. See the module docs for the politeness rules.
#[derive(Debug)]
pub struct PoliteClient {
    http: reqwest::blocking::Client,
    /// Minimum spacing between two requests to the same host.
    per_host_rate: Duration,
    buckets: Mutex<HashMap<String, TokenBucket>>,
}

impl Default for PoliteClient {
    fn default() -> Self {
        Self::new(DEFAULT_PER_HOST_RATE, DEFAULT_USER_AGENT, DEFAULT_TIMEOUT)
            .expect("static reqwest client config is valid")
    }
}

impl PoliteClient {
    /// A client that sends `user_agent`, times out after `timeout`, and never
    /// issues more than one request per `per_host_rate` to a given host.
    pub fn new(
        per_host_rate: Duration,
        user_agent: &str,
        timeout: Duration,
    ) -> Result<Self, reqwest::Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(user_agent)
            .timeout(timeout)
            .build()?;
        Ok(PoliteClient {
            http,
            per_host_rate,
            buckets: Mutex::new(HashMap::new()),
        })
    }

    /// GET `url`, waiting for this client's per-host rate limit first.
    ///
    /// With `require_success`, any non-2xx status becomes
    /// [`NetError::Status`]; otherwise the response is returned as-is and
    /// the caller inspects the status itself.
    pub fn get(
        &self,
        url: &str,
        require_success: bool,
    ) -> Result<reqwest::blocking::Response, NetError> {
        self.await_turn(url)?;
        let response = self.http.get(url).send().map_err(NetError::Request)?;
        let status = response.status();
        if require_success && !status.is_success() {
            return Err(NetError::Status(status));
        }
        Ok(response)
    }

    /// GET `url` and decode the body as `T`, erroring on non-2xx.
    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, NetError> {
        let response = self.get(url, true)?;
        response.json().map_err(NetError::Decode)
    }

    /// GET `url` with an additional header, applying the same per-host rate
    /// limit and timeout as [`Self::get_json`].
    pub fn get_json_with_header<T: DeserializeOwned>(
        &self,
        url: &str,
        name: &str,
        value: &str,
    ) -> Result<T, NetError> {
        self.await_turn(url)?;
        let response = self
            .http
            .get(url)
            .header(name, value)
            .send()
            .map_err(NetError::Request)?;
        if !response.status().is_success() {
            return Err(NetError::Status(response.status()));
        }
        response.json().map_err(NetError::Decode)
    }

    /// GET `url` and return the body as text, erroring on non-2xx.
    pub fn get_text(&self, url: &str) -> Result<String, NetError> {
        let response = self.get(url, true)?;
        response.text().map_err(NetError::Request)
    }

    /// Blocks until one request to `url`'s host is allowed, then consumes it.
    fn await_turn(&self, url: &str) -> Result<(), NetError> {
        let parsed = reqwest::Url::parse(url).map_err(|_| NetError::InvalidUrl {
            url: url.to_string(),
        })?;
        let host = parsed.host_str().unwrap_or("").to_string();
        loop {
            let delay = {
                let mut buckets = self
                    .buckets
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let now = Instant::now();
                let bucket = buckets
                    .entry(host.clone())
                    .or_insert_with(|| TokenBucket::full(now));
                bucket.take_or_delay(self.per_host_rate, now)
            };
            match delay {
                None => return Ok(()),
                Some(delay) => std::thread::sleep(delay),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fast client for mock-server tests: no point waiting a real second.
    fn fast_client(rate: Duration) -> PoliteClient {
        PoliteClient::new(rate, "DepRadar test agent", Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn bucket_allows_one_immediate_request_then_delays() {
        let start = Instant::now();
        let mut bucket = TokenBucket::full(start);
        let period = Duration::from_millis(100);

        assert_eq!(bucket.take_or_delay(period, start), None);
        // Immediately after, the bucket is empty: we're told to wait ~period.
        let delay = bucket
            .take_or_delay(period, start + Duration::from_millis(1))
            .expect("second immediate request must be delayed");
        assert!(delay >= Duration::from_millis(90), "delay was {delay:?}");
        assert!(delay <= period);
    }

    #[test]
    fn bucket_refills_over_time() {
        let start = Instant::now();
        let mut bucket = TokenBucket::full(start);
        let period = Duration::from_millis(100);
        assert_eq!(bucket.take_or_delay(period, start), None);
        // One full period later, the token is back.
        assert_eq!(
            bucket.take_or_delay(period, start + period),
            None,
            "a full period of refill must restore the token"
        );
    }

    #[test]
    fn invalid_url_is_rejected_before_any_request() {
        let client = fast_client(Duration::from_millis(1));
        assert!(matches!(
            client.get("not a url", true),
            Err(NetError::InvalidUrl { .. })
        ));
    }

    /// N requests to a mock server take at least `(N - 1) * rate` — the
    /// first is immediate (the bucket starts full), every later one waits.
    #[test]
    fn n_requests_take_at_least_the_expected_minimum_time() {
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", "/").expect(4).create();

        let rate = Duration::from_millis(40);
        let client = fast_client(rate);

        let requests = 4;
        let started = Instant::now();
        for _ in 0..requests {
            let response = client.get(&server.url(), true).unwrap();
            assert_eq!(response.status(), 200);
        }
        let elapsed = started.elapsed();

        mock.assert();
        let minimum = rate * u32::try_from(requests - 1).unwrap();
        assert!(
            elapsed >= minimum,
            "{requests} requests at {rate:?} apart took {elapsed:?}, expected at least {minimum:?}"
        );
    }

    /// Different hosts don't share a bucket.
    #[test]
    fn rate_limit_is_per_host() {
        let mut a = mockito::Server::new();
        let mut b = mockito::Server::new();
        let mock_a = a.mock("GET", "/").expect(1).create();
        let mock_b = b.mock("GET", "/").expect(1).create();

        let client = fast_client(Duration::from_secs(60));

        // Two different hosts: both immediate, despite the 60s rate.
        assert_eq!(client.get(&a.url(), true).unwrap().status(), 200);
        assert_eq!(client.get(&b.url(), true).unwrap().status(), 200);
        mock_a.assert();
        mock_b.assert();
    }
}
