//! Behavioral analysis of package metadata.
//!
//! This module detects suspicious behavioral patterns in package manifests,
//! such as install-time scripts that could be used as malware delivery vectors.

pub mod scripts;
