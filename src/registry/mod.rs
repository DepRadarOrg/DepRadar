//! Registry metadata lookups for individual packages.
//!
//! * [`npm`] — npm registry metadata: maintainers, publish times,
//!   repository, latest version (#94).
//! * [`crates`] — crates.io registry metadata: yanked versions and
//!   license information (#178, #179).

pub mod crates;
pub mod npm;