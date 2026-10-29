//! CVE / known-vulnerability matching.
//!
//! Starts with a client for the [OSV](https://osv.dev) API ([`osv_client`]),
//! which resolves a package name + ecosystem + version to the raw set of
//! known vulnerabilities affecting it. No filtering or scoring yet — that
//! belongs with the (not-yet-written) CVE scanner module.

pub mod cache;
pub mod osv_client;
pub mod remediation;
pub mod severity;

pub use osv_client::{fetch_vuln, OsvVulnerability};