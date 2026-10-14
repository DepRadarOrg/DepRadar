//! DepRadar off-chain toolkit.
//!
//! This crate hosts the crawler and scanner modules that feed findings to the
//! on-chain bounty router (`contracts/deprader_router`). Only the pieces that
//! have landed are wired up here.

pub mod cve;
pub mod model;
pub mod typosquat;
