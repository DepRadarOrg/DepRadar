//! Runtime monitors for popular packages.
//!
//! These modules compare successive metadata snapshots from the crawler and
//! emit findings when suspicious changes are detected.

pub mod maintainers;
pub mod scripts;
