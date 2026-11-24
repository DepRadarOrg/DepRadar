//! CVE / known-vulnerability matching.
//!
//! Starts with a client for the [OSV](https://osv.dev) API ([`osv_client`]),
//! which resolves a package name + ecosystem + version to the raw set of
//! known vulnerabilities affecting it. No filtering or scoring yet — that
//! belongs with the (not-yet-written) CVE scanner module.
//!
//! [`offline_db`] provides an air-gapped alternative: it downloads and
//! indexes OSV per-ecosystem zip dumps into a local directory and resolves
//! queries against that local index, returning the same response shape as
//! the network client.
//!
//! Also provides [`kev`], a client for CISA's Known Exploited Vulnerabilities
//! feed, used to flag findings that are being exploited in the wild.

pub mod cache;
pub mod kev;
pub mod offline_db;
pub mod osv_client;
pub mod remediation;
pub mod severity;

pub use kev::{is_known_exploited, vuln_is_known_exploited};
pub use offline_db::{OfflineDb, OfflineDbError};
pub use osv_client::{fetch_vuln, OsvVulnerability};

/// Label used to mark OSV `MAL-` advisories as malicious-package findings.
pub const MALICIOUS_PACKAGE_LABEL: &str = "malicious package";

/// Returns `true` when the given OSV advisory ID denotes a known-malware
/// (`MAL-`) advisory rather than an ordinary vulnerability.
pub fn is_malicious_advisory(id: &str) -> bool {
    id.starts_with("MAL-")
}
