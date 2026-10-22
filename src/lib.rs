//! DepRadar off-chain toolkit.
//!
//! This crate hosts the crawler and scanner modules that feed findings to the
//! on-chain bounty router (`contracts/deprader_router`). Only the pieces that
//! have landed are wired up here.
//!
//! [`pipeline::scan_lockfile`] is the end-to-end entry point: it parses a
//! project lockfile ([`lockfile`]) and runs the typosquat ([`typosquat`]) and
//! known-vulnerability ([`cve`]) checks over every dependency in it, minus
//! anything accepted in a `.depradarignore` file ([`ignore`]). [`report`]
//! rates each finding's severity and renders the result, e.g. as SARIF.
//! [`behavioral`] analyzes the contents of an unpacked package itself —
//! install-time `setup.py` calls, shipped native binaries, and risky
//! changes between two versions. [`license`] fetches dependency license
//! metadata from registries and flags licenses outside a configurable
//! allowlist ([`LicensePolicy`]).

pub mod behavioral;
pub mod config;
pub mod crawler;
pub mod cve;
pub mod ignore;
pub mod license;
pub mod lockfile;
pub mod model;
pub mod net;
pub mod pipeline;
pub mod registry;
pub mod report;
pub mod typosquat;

pub use license::LicensePolicy;
pub use pipeline::{scan_lockfile, Report, ScanOptions};