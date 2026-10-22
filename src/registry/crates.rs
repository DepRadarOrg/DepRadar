//! crates.io registry client.
//!
//! Fetches crate metadata from `https://crates.io/api/v1/crates/<name>` and
//! exposes, per version, the `yanked` flag and the crate `license`.

use serde::Deserialize;

use crate::registry::npm::RegistryError;
use crate::registry::PoliteClient;

/// Base URL for the crates.io API.
const CRATES_IO_API: &str = "https://crates.io/api/v1/crates";

/// Metadata for a single crate version.
#[derive(Debug, Clone, Deserialize)]
pub struct CrateVersionMeta {
    /// The version string, e.g. `1.2.3`.
    pub num: String,
    /// Whether the version was yanked from crates.io.
    #[serde(default)]
    pub yanked: bool,
    /// The license expression declared for this version, if any.
    #[serde(default)]
    pub license: Option<String>,
}

/// Metadata for a crate, including all of its versions.
#[derive(Debug, Clone, Deserialize)]
pub struct CrateMeta {
    /// The crate name.
    pub name: String,
    /// The license declared at the crate level, if any.
    #[serde(default)]
    pub license: Option<String>,
    /// All published versions of the crate.
    #[serde(default)]
    pub versions: Vec<CrateVersionMeta>,
}

/// The top-level response envelope returned by the crates.io API.
#[derive(Debug, Deserialize)]
struct CrateResponse {
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    versions: Vec<CrateVersionMeta>,
}

impl CrateMeta {
    /// Returns the metadata for the given version, if present.
    pub fn version(&self, version: &str) -> Option<&CrateVersionMeta> {
        self.versions.iter().find(|v| v.num == version)
    }

    /// Returns the license for the given version, falling back to the
    /// crate-level license.
    pub fn license_for(&self, version: &str) -> Option<&str> {
        self.version(version)
            .and_then(|v| v.license.as_deref())
            .or(self.license.as_deref())
    }
}

/// Fetches crate metadata for `name` using the default [`PoliteClient`].
pub async fn fetch_crate_metadata(name: &str) -> Result<CrateMeta, RegistryError> {
    let client = PoliteClient::new();
    fetch_crate_metadata_with(&client, name).await
}

/// Fetches crate metadata for `name` using the provided [`PoliteClient`].
pub async fn fetch_crate_metadata_with(
    client: &PoliteClient,
    name: &str,
) -> Result<CrateMeta, RegistryError> {
    let url = format!("{CRATES_IO_API}/{name}");
    fetch_crate_metadata_at(client, &url, name).await
}

/// Fetches crate metadata for `name` from an explicit `url`.
///
/// This is primarily useful for tests, which can point at a local fixture
/// server.
pub async fn fetch_crate_metadata_at(
    client: &PoliteClient,
    url: &str,
    name: &str,
) -> Result<CrateMeta, RegistryError> {
    let body = client.get(url).await?;
    let response: CrateResponse = serde_json::from_str(&body)?;

    Ok(CrateMeta {
        name: name.to_string(),
        license: response.license,
        versions: response.versions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const YANKED_FIXTURE: &str = r#"{
        "license": "MIT OR Apache-2.0",
        "versions": [
            { "num": "1.0.0", "yanked": false, "license": "MIT OR Apache-2.0" },
            { "num": "1.0.1", "yanked": true, "license": "MIT OR Apache-2.0" }
        ]
    }"#;

    const NON_YANKED_FIXTURE: &str = r#"{
        "license": "MIT",
        "versions": [
            { "num": "2.0.0", "yanked": false, "license": "MIT" }
        ]
    }"#;

    const NO_LICENSE_FIXTURE: &str = r#"{
        "versions": [
            { "num": "0.1.0", "yanked": false }
        ]
    }"#;

    fn parse(name: &str, body: &str) -> CrateMeta {
        let response: CrateResponse = serde_json::from_str(body).unwrap();
        CrateMeta {
            name: name.to_string(),
            license: response.license,
            versions: response.versions,
        }
    }

    #[test]
    fn detects_yanked_version() {
        let meta = parse("example", YANKED_FIXTURE);
        assert_eq!(meta.name, "example");
        assert_eq!(meta.versions.len(), 2);

        let yanked = meta.version("1.0.1").expect("version present");
        assert!(yanked.yanked);

        let stable = meta.version("1.0.0").expect("version present");
        assert!(!stable.yanked);
    }

    #[test]
    fn detects_non_yanked_version() {
        let meta = parse("example", NON_YANKED_FIXTURE);
        let version = meta.version("2.0.0").expect("version present");
        assert!(!version.yanked);
        assert!(meta.version("9.9.9").is_none());
    }

    #[test]
    fn exposes_license() {
        let meta = parse("example", YANKED_FIXTURE);
        assert_eq!(meta.license.as_deref(), Some("MIT OR Apache-2.0"));
        assert_eq!(meta.license_for("1.0.0"), Some("MIT OR Apache-2.0"));
    }

    #[test]
    fn license_falls_back_to_crate_level() {
        let meta = parse("example", NO_LICENSE_FIXTURE);
        assert!(meta.license.is_none());
        assert!(meta.license_for("0.1.0").is_none());
    }
}