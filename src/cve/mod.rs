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

pub mod cache;
pub mod offline_db;
pub mod osv_client;
pub mod remediation;
pub mod severity;

pub use offline_db::{OfflineDb, OfflineDbError};
pub use osv_client::{fetch_vuln, OsvVulnerability};