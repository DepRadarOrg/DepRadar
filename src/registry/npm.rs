//! npm registry metadata: the context a scanner needs to judge a package
//! (#94).
//!
//! A version published an hour ago by a brand-new maintainer with no
//! repository is far more suspicious than one published by a maintainer of
//! a decade-old package. [`fetch_npm_metadata`] pulls exactly the fields
//! that tell that story from `https://registry.npmjs.org/<name>`:
//! maintainers, creation/modification times, repository URL, the latest
//! version and how many versions exist.
//!
//! Scoped names (`@scope/name`) are URL-encoded on the wire — the `/`
//! becomes `%2F`, per the registry's documented URL shape
//! <https://github.com/npm/registry/blob/master/docs/REGISTRY-API.md#getpackage>.

use serde::Deserialize;

use crate::net::{NetError, PoliteClient};

/// Base URL of the public npm registry.
pub const NPM_REGISTRY_URL: &str = "https://registry.npmjs.org";

/// Errors from [`fetch_npm_metadata`].
#[derive(Debug)]
pub enum NpmError {
    /// The HTTP request failed (or came back non-2xx).
    Net(NetError),
    /// The response body wasn't the JSON shape we expect.
    Malformed,
}

impl std::fmt::Display for NpmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NpmError::Net(err) => write!(f, "npm registry request failed: {err}"),
            NpmError::Malformed => write!(f, "npm registry response is not the expected shape"),
        }
    }
}

impl std::error::Error for NpmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NpmError::Net(err) => Some(err),
            NpmError::Malformed => None,
        }
    }
}

impl From<NetError> for NpmError {
    fn from(err: NetError) -> Self {
        NpmError::Net(err)
    }
}

/// One maintainer of a package.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Maintainer {
    /// npm username, e.g. `reqwest_bot`.
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// Registry metadata for one package, as of the fetch.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PackageMeta {
    /// Package name exactly as the registry returned it.
    pub name: String,
    #[serde(default)]
    pub maintainers: Vec<Maintainer>,
    /// RFC 3339 creation time, e.g. `2011-03-14T00:51:29.834Z`.
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    /// Repository URL, when the package declares one.
    #[serde(default)]
    pub repository: Option<String>,
    /// Value of `dist-tags.latest`.
    #[serde(default)]
    pub latest: Option<String>,
    /// Number of published versions.
    #[serde(default)]
    pub versions_count: usize,
    /// Deprecation message for the latest version, when npm marks it
    /// deprecated. `None` means the version is not deprecated.
    #[serde(default)]
    pub deprecated: Option<String>,
    /// Whether the version was yanked. Always `false` for npm (the registry
    /// has no yank concept); carried here so crates.io metadata can share
    /// the same shape.
    #[serde(default)]
    pub yanked: bool,
}
=======

/// The subset of the registry document [`fetch_npm_metadata`] reads.
///
/// `repository` is loosely typed on purpose: the registry serves either a
/// bare string or an object with a `url` field, depending on the package.
#[derive(Deserialize)]
struct NpmDocument {
    name: String,
    #[serde(default)]
    maintainers: Vec<Maintainer>,
    #[serde(default)]
    time: NpmTimes,
    #[serde(default)]
    repository: Option<serde_json::Value>,
    #[serde(default, rename = "dist-tags")]
    dist_tags: DistTags,
    #[serde(default)]
    versions: serde_json::Map<String, serde_json::Value>,
}

#[derive(Default, Deserialize)]
struct NpmTimes {
    created: Option<String>,
    modified: Option<String>,
}

#[derive(Default, Deserialize)]
struct DistTags {
    #[serde(rename = "latest")]
    latest: Option<String>,
}

/// Percent-encodes a package name for the registry URL path.
///
/// Scoped names contain a `/`, which must become `%2F`; everything else is
/// left alone — npm names are already URL-safe.
pub fn encode_package_name(name: &str) -> String {
    name.replace('/', "%2F")
}

/// Pulls the registry document for `name` and reduces it to [`PackageMeta`].
///
/// Requests go through [`PoliteClient`] (rate limit, User-Agent, timeout),
/// so a loop over many names stays polite. Scoped names are encoded: `@types
/// /node` is fetched from `/@types%2Fnode`.
pub fn fetch_npm_metadata(name: &str) -> Result<PackageMeta, NpmError> {
    fetch_npm_metadata_at(NPM_REGISTRY_URL, name)
}

/// Like [`fetch_npm_metadata`], against another registry base URL, e.g. a
/// mock server in tests.
pub fn fetch_npm_metadata_at(base_url: &str, name: &str) -> Result<PackageMeta, NpmError> {
    let client = PoliteClient::default();
    fetch_npm_metadata_with(&client, base_url, name)
}

/// Like [`fetch_npm_metadata_at`], with an explicit client — the variant
/// crawlers use, so a whole run shares one rate-limit budget.
pub fn fetch_npm_metadata_with(
    client: &PoliteClient,
    base_url: &str,
    name: &str,
) -> Result<PackageMeta, NpmError> {
    let url = format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        encode_package_name(name)
    );
    let document: NpmDocument = client.get_json(&url)?;
    Ok(PackageMeta::from_document(document))
}

impl PackageMeta {
    fn from_document(document: NpmDocument) -> Self {
        let latest = document.dist_tags.latest;
        let deprecated = latest
            .as_deref()
            .and_then(|version| document.versions.get(version))
            .and_then(deprecation_message);
        PackageMeta {
            name: document.name,
            maintainers: document.maintainers,
            created: document.time.created,
            modified: document.time.modified,
            repository: repository_url(document.repository.as_ref()),
            latest,
            versions_count: document.versions.len(),
            deprecated,
            yanked: false,
        }
    }
}

/// Extracts the `deprecated` message from a version object.
///
/// npm marks a deprecated version with a `deprecated` string on that
/// version's metadata. Some tooling writes `false`/`null` instead; treat
/// anything that isn't a non-empty string as "not deprecated".
fn deprecation_message(version: &serde_json::Value) -> Option<String> {
    version
        .get("deprecated")
        .and_then(|value| value.as_str())
        .filter(|message| !message.is_empty())
        .map(str::to_string)
}

/// The registry serves `repository` as either a bare string or an object
/// with a `url` field; accept both.
fn repository_url(repository: Option<&serde_json::Value>) -> Option<String> {
    match repository? {
        serde_json::Value::String(url) => Some(url.clone()),
        serde_json::Value::Object(fields) => fields
            .get("url")
            .and_then(|url| url.as_str())
            .map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed copy of a real registry document (unscoped package).
    const LODASH_DOC: &str = r#"{
        "name": "lodash",
        "dist-tags": {"latest": "4.17.21"},
        "time": {
            "created": "2012-03-20T11:11:54.598Z",
            "modified": "2025-09-01T10:00:00.000Z"
        },
        "repository": {"type": "git", "url": "git+https://github.com/lodash/lodash.git"},
        "maintainers": [{"name": "jdalton", "email": "john.dalton@example.com"}],
        "versions": {"4.17.20": {}, "4.17.21": {}}
    }"#;

    /// Trimmed copy of a real scoped-package document; `repository` is the
    /// bare-string form and maintainers carry usernames.
    const SCOPED_DOC: &str = r#"{
        "name": "@types/node",
        "dist-tags": {"latest": "22.7.5"},
        "time": {
            "created": "2016-01-08T21:16:30.123Z",
            "modified": "2025-10-01T09:30:00.000Z"
        },
        "repository": "github:DefinitelyTyped/DefinitelyTyped",
        "maintainers": [{"username": "types-bot", "email": "types@example.com"}],
        "versions": {"22.7.4": {}, "22.7.5": {}}
    }"#;

    #[test]
    fn parses_metadata_from_a_registry_document() {
        let doc: NpmDocument = serde_json::from_str(LODASH_DOC).unwrap();
        let meta = PackageMeta::from_document(doc);
        assert_eq!(meta.name, "lodash");
        assert_eq!(meta.latest.as_deref(), Some("4.17.21"));
        assert_eq!(meta.created.as_deref(), Some("2012-03-20T11:11:54.598Z"));
        assert_eq!(meta.modified.as_deref(), Some("2025-09-01T10:00:00.000Z"));
        assert_eq!(
            meta.repository.as_deref(),
            Some("git+https://github.com/lodash/lodash.git")
        );
        assert_eq!(meta.versions_count, 2);
        assert_eq!(meta.maintainers.len(), 1);
        assert_eq!(meta.maintainers[0].name.as_deref(), Some("jdalton"));
    }

    #[test]
    fn parses_a_scoped_package_document() {
        let doc: NpmDocument = serde_json::from_str(SCOPED_DOC).unwrap();
        let meta = PackageMeta::from_document(doc);
        assert_eq!(meta.name, "@types/node");
        assert_eq!(
            meta.repository.as_deref(),
            Some("github:DefinitelyTyped/DefinitelyTyped")
        );
        assert_eq!(meta.versions_count, 2);
        assert_eq!(meta.maintainers[0].username.as_deref(), Some("types-bot"));
    }

    #[test]
    fn scoped_names_are_url_encoded() {
        assert_eq!(encode_package_name("@types/node"), "@types%2Fnode");
        assert_eq!(encode_package_name("lodash"), "lodash");
    }

    /// The fetch hits `/@types%2Fnode` — the `/` in the scoped name must not
    /// turn into an extra path segment.
    #[test]
    fn fetch_uses_the_encoded_path_for_scoped_names() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/@types%2Fnode")
            .with_body(SCOPED_DOC)
            .expect(1)
            .create();

        let meta = fetch_npm_metadata_at(&server.url(), "@types/node").unwrap();
        assert_eq!(meta.name, "@types/node");
        mock.assert();
    }

    #[test]
    fn fetch_works_for_plain_names() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/lodash")
            .with_body(LODASH_DOC)
            .expect(1)
            .create();

        let meta = fetch_npm_metadata_at(&server.url(), "lodash").unwrap();
        assert_eq!(meta.versions_count, 2);
        mock.assert();
    }

    #[test]
    fn missing_fields_stay_none_rather_than_failing() {
        let doc: NpmDocument = serde_json::from_str(r#"{"name": "bare", "versions": {}}"#).unwrap();
        let meta = PackageMeta::from_document(doc);
        assert_eq!(meta.name, "bare");
        assert!(meta.maintainers.is_empty());
        assert!(meta.created.is_none());
        assert!(meta.repository.is_none());
        assert!(meta.latest.is_none());
        assert_eq!(meta.versions_count, 0);
        assert!(meta.deprecated.is_none());
        assert!(!meta.yanked);
    }

    /// A deprecated latest version carries its message through.
    const DEPRECATED_DOC: &str = r#"{
        "name": "request",
        "dist-tags": {"latest": "2.88.2"},
        "time": {
            "created": "2011-03-14T00:51:29.834Z",
            "modified": "2020-02-11T00:00:00.000Z"
        },
        "versions": {
            "2.88.1": {},
            "2.88.2": {"deprecated": "request has been deprecated, see https://github.com/request/request/issues/3142"}
        }
    }"#;

    #[test]
    fn parses_the_deprecation_message_of_the_latest_version() {
        let doc: NpmDocument = serde_json::from_str(DEPRECATED_DOC).unwrap();
        let meta = PackageMeta::from_document(doc);
        assert_eq!(meta.latest.as_deref(), Some("2.88.2"));
        assert_eq!(
            meta.deprecated.as_deref(),
            Some("request has been deprecated, see https://github.com/request/request/issues/3142")
        );
        assert!(!meta.yanked);
    }

    #[test]
    fn non_deprecated_latest_version_has_no_message() {
        let doc: NpmDocument = serde_json::from_str(LODASH_DOC).unwrap();
        let meta = PackageMeta::from_document(doc);
        assert!(meta.deprecated.is_none());
        assert!(!meta.yanked);
    }

    /// A `deprecated: false` marker (written by some tooling) is not a
    /// deprecation.
    #[test]
    fn false_deprecated_flag_is_not_a_deprecation() {
        let doc: NpmDocument = serde_json::from_str(
            r#"{
                "name": "notdep",
                "dist-tags": {"latest": "1.0.0"},
                "versions": {"1.0.0": {"deprecated": false}}
            }"#,
        )
        .unwrap();
        let meta = PackageMeta::from_document(doc);
        assert!(meta.deprecated.is_none());
    }
}
