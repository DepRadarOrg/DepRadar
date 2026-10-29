//! Safe download and unpacking of npm tarballs.
//!
//! Behavioral checks need the files inside a published package, but npm
//! tarballs are attacker-controlled archives. Nothing here ever extracts to a
//! path taken from the archive: [`unpack_tarball`] decompresses into memory
//! and validates every entry before returning it. The same limits apply when
//! the bytes are written out by a caller.
//!
//! Defences, all on by default ([`UnpackLimits`]):
//!
//! - the download body is capped at [`MAX_TARBALL_BYTES`] (50 MB),
//! - the sum of unpacked entry sizes is capped at
//!   [`MAX_UNPACKED_BYTES`] (50 MB), so a gzip bomb can't fill memory,
//! - at most [`MAX_FILES`] entries, each with a bounded path length,
//! - absolute paths, `..` components, backslashes and Windows drive
//!   prefixes are rejected,
//! - symlinks, hard links and every non-file/non-directory entry type are
//!   rejected,
//! - duplicate paths are rejected.
//!
//! `dist.integrity` is verified before unpacking when the caller has it:
//! [`download_verified`] checks the SHA-512 Subresource Integrity string and
//! returns the body only when it matches. No code from the archive is ever
//! executed.

use std::collections::BTreeSet;
use std::io::Read;

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha512};
use tar::EntryType;

use crate::net::{NetError, PoliteClient};

/// Largest tarball body accepted from the registry.
pub const MAX_TARBALL_BYTES: u64 = 50 * 1024 * 1024;
/// Largest total of unpacked file bytes accepted.
pub const MAX_UNPACKED_BYTES: u64 = 50 * 1024 * 1024;
/// Largest number of entries accepted.
pub const MAX_FILES: usize = 10_000;
/// Longest accepted entry path, in bytes.
pub const MAX_PATH_BYTES: usize = 1024;

/// Limits applied while unpacking. [`Default`] is the safe production set.
#[derive(Debug, Clone, Copy)]
pub struct UnpackLimits {
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    pub max_files: usize,
    pub max_path_bytes: usize,
}

impl Default for UnpackLimits {
    fn default() -> Self {
        UnpackLimits {
            max_total_bytes: MAX_UNPACKED_BYTES,
            max_file_bytes: MAX_UNPACKED_BYTES,
            max_files: MAX_FILES,
            max_path_bytes: MAX_PATH_BYTES,
        }
    }
}

/// Errors from downloading, verifying or unpacking an npm tarball.
#[derive(Debug)]
pub enum TarballError {
    /// The HTTP request failed or the registry returned a non-2xx status.
    Http(String),
    /// The body was larger than the configured limit.
    TooLarge { limit: u64 },
    /// `dist.integrity` had no SHA-512 SRI digest.
    MissingSha512,
    /// `dist.integrity` contained a malformed digest.
    InvalidIntegrity(String),
    /// The downloaded bytes did not match `dist.integrity`.
    IntegrityMismatch,
    /// The archive had more entries than allowed.
    TooManyFiles { limit: usize },
    /// One path was absolute, escaped the root, or was otherwise unsafe.
    UnsafePath(String),
    /// The archive contained a symlink or hard link.
    Link(String),
    /// The archive contained an entry type other than a regular file or dir.
    UnsupportedEntry(String),
    /// Two entries resolved to the same package-relative path.
    DuplicatePath(String),
    /// The gzip or tar stream was malformed.
    Archive(String),
}

impl std::fmt::Display for TarballError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TarballError::Http(err) => write!(f, "tarball download failed: {err}"),
            TarballError::TooLarge { limit } => {
                write!(f, "tarball exceeds the {limit}-byte limit")
            }
            TarballError::MissingSha512 => {
                write!(f, "dist.integrity has no sha512 digest")
            }
            TarballError::InvalidIntegrity(value) => {
                write!(f, "malformed sha512 integrity value: {value}")
            }
            TarballError::IntegrityMismatch => {
                write!(f, "tarball does not match dist.integrity")
            }
            TarballError::TooManyFiles { limit } => {
                write!(f, "tarball has more than {limit} entries")
            }
            TarballError::UnsafePath(path) => {
                write!(f, "unsafe path in tarball: {path}")
            }
            TarballError::Link(path) => {
                write!(f, "link entry rejected in tarball: {path}")
            }
            TarballError::UnsupportedEntry(path) => {
                write!(f, "unsupported entry type in tarball: {path}")
            }
            TarballError::DuplicatePath(path) => {
                write!(f, "duplicate path in tarball: {path}")
            }
            TarballError::Archive(err) => write!(f, "malformed tarball: {err}"),
        }
    }
}

impl std::error::Error for TarballError {}

impl From<NetError> for TarballError {
    fn from(err: NetError) -> Self {
        TarballError::Http(err.to_string())
    }
}

impl From<std::io::Error> for TarballError {
    fn from(err: std::io::Error) -> Self {
        TarballError::Archive(err.to_string())
    }
}

/// One regular file unpacked from a tarball.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFile {
    /// Package-relative path, `/`-separated. The npm `package/` prefix is
    /// stripped when every entry has it.
    pub path: String,
    pub data: Vec<u8>,
}

impl PackageFile {
    /// Reads the file as UTF-8 text, if it is valid UTF-8.
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.data).ok()
    }
}

/// The `dist` object embedded in an npm version document.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NpmDist {
    /// URL of the `.tgz` for this version.
    pub tarball: String,
    /// Subresource Integrity string, e.g. `sha512-...`.
    #[serde(default)]
    pub integrity: Option<String>,
}

/// Downloads `dist.tarball` and verifies `dist.integrity` before returning the
/// bytes. A missing integrity value is rejected: callers that genuinely have
/// no digest should use [`download_tarball`] and verify out of band.
pub fn download_npm_dist(dist: &NpmDist) -> Result<Vec<u8>, TarballError> {
    let integrity = dist
        .integrity
        .as_deref()
        .ok_or(TarballError::MissingSha512)?;
    download_verified(&dist.tarball, integrity)
}

/// Downloads `url` through the polite client, capped at
/// [`MAX_TARBALL_BYTES`].
pub fn download_tarball(url: &str) -> Result<Vec<u8>, TarballError> {
    download_tarball_with(&PoliteClient::default(), url, MAX_TARBALL_BYTES)
}

/// [`download_tarball`] with an explicit client and size limit.
pub fn download_tarball_with(
    client: &PoliteClient,
    url: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, TarballError> {
    let response = client.get(url, true)?;
    if let Some(length) = response.content_length() {
        if length > max_bytes {
            return Err(TarballError::TooLarge { limit: max_bytes });
        }
    }
    let mut body = Vec::new();
    response
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut body)?;
    if body.len() as u64 > max_bytes {
        return Err(TarballError::TooLarge { limit: max_bytes });
    }
    Ok(body)
}

/// Downloads `url` and checks the body against an SRI `integrity` string.
pub fn download_verified(url: &str, integrity: &str) -> Result<Vec<u8>, TarballError> {
    let bytes = download_tarball(url)?;
    verify_integrity(&bytes, integrity)?;
    Ok(bytes)
}

/// Verifies `bytes` against the SHA-512 digests in an SRI string.
///
/// SRI permits several space-separated tokens (`sha512-... sha256-...`);
/// only `sha512-` tokens are considered because npm publishes those. At
/// least one must match.
pub fn verify_integrity(bytes: &[u8], integrity: &str) -> Result<(), TarballError> {
    let mut saw_sha512 = false;
    let digest = Sha512::digest(bytes);
    for token in integrity.split_whitespace() {
        let Some((algorithm, encoded)) = token.split_once('-') else {
            continue;
        };
        if algorithm != "sha512" {
            continue;
        }
        saw_sha512 = true;
        let decoded = STANDARD
            .decode(encoded)
            .or_else(|_| STANDARD_NO_PAD.decode(encoded))
            .map_err(|_| TarballError::InvalidIntegrity(token.to_string()))?;
        if decoded.len() != digest.len() {
            return Err(TarballError::InvalidIntegrity(token.to_string()));
        }
        if constant_time_eq(&decoded, &digest) {
            return Ok(());
        }
    }
    if saw_sha512 {
        Err(TarballError::IntegrityMismatch)
    } else {
        Err(TarballError::MissingSha512)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Unpacks a gzipped npm tarball into memory with [`UnpackLimits::default`].
///
/// Returns regular files only, sorted by path. Directories are represented
/// implicitly by the paths of the files under them.
pub fn unpack_tarball(bytes: &[u8]) -> Result<Vec<PackageFile>, TarballError> {
    unpack_tarball_with_limits(bytes, &UnpackLimits::default())
}

/// [`unpack_tarball`] with explicit limits.
pub fn unpack_tarball_with_limits(
    bytes: &[u8],
    limits: &UnpackLimits,
) -> Result<Vec<PackageFile>, TarballError> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total: u64 = 0;

    for entry in archive.entries()? {
        let entry = entry?;

        let raw_path = entry.path_bytes();
        let path = normalize_path(&raw_path, limits.max_path_bytes)?;
        if path.is_empty() {
            continue;
        }

        let entry_type = entry.header().entry_type();
        match entry_type {
            EntryType::Directory => continue,
            EntryType::Regular | EntryType::Continuous => {}
            EntryType::Symlink | EntryType::Link => {
                return Err(TarballError::Link(path));
            }
            _ => return Err(TarballError::UnsupportedEntry(path)),
        }

        if files.len() >= limits.max_files {
            return Err(TarballError::TooManyFiles {
                limit: limits.max_files,
            });
        }

        if !seen.insert(path.clone()) {
            return Err(TarballError::DuplicatePath(path));
        }

        let size = entry.header().size()?;
        if size > limits.max_file_bytes {
            return Err(TarballError::TooLarge {
                limit: limits.max_file_bytes,
            });
        }
        total = total.checked_add(size).ok_or(TarballError::TooLarge {
            limit: limits.max_total_bytes,
        })?;
        if total > limits.max_total_bytes {
            return Err(TarballError::TooLarge {
                limit: limits.max_total_bytes,
            });
        }

        let capacity = usize::try_from(size.min(64 * 1024)).unwrap_or(0);
        let mut data = Vec::with_capacity(capacity);
        entry
            .take(size.saturating_add(1))
            .read_to_end(&mut data)
            .map_err(TarballError::from)?;
        if data.len() as u64 != size {
            return Err(TarballError::Archive(format!(
                "entry {path} is shorter than its declared {size} bytes"
            )));
        }
        files.push(PackageFile { path, data });
    }

    strip_package_prefix(&mut files);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Validates and normalises an archive path. Returns the path with `./`
/// components removed and empty components collapsed.
fn normalize_path(raw: &[u8], max_bytes: usize) -> Result<String, TarballError> {
    if raw.is_empty() {
        return Ok(String::new());
    }
    if raw.len() > max_bytes {
        return Err(TarballError::UnsafePath(
            String::from_utf8_lossy(raw).into_owned(),
        ));
    }
    if raw.contains(&0) {
        return Err(TarballError::UnsafePath("path contains NUL".to_string()));
    }
    let path = std::str::from_utf8(raw)
        .map_err(|_| TarballError::UnsafePath(String::from_utf8_lossy(raw).into_owned()))?;
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(TarballError::UnsafePath(path.to_string()));
    }
    if path.contains('\\') {
        return Err(TarballError::UnsafePath(path.to_string()));
    }

    let mut parts = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => continue,
            ".." => return Err(TarballError::UnsafePath(path.to_string())),
            // A Windows drive or alternate data stream would escape a
            // caller that later joins this onto a directory.
            component if component.contains(':') => {
                return Err(TarballError::UnsafePath(path.to_string()));
            }
            component => parts.push(component),
        }
    }
    Ok(parts.join("/"))
}

/// npm tarballs wrap everything in a single `package/` directory. Strip it
/// only when every file is under it, so an archive can't use the prefix to
/// hide a path that sits outside the package.
fn strip_package_prefix(files: &mut Vec<PackageFile>) {
    if files.is_empty() {
        return;
    }
    let prefixed = files
        .iter()
        .all(|file| file.path == "package" || file.path.starts_with("package/"));
    if !prefixed {
        return;
    }
    for file in files.iter_mut() {
        if let Some(rest) = file.path.strip_prefix("package/") {
            file.path = rest.to_string();
        }
    }
    files.retain(|file| !file.path.is_empty() && file.path != "package");
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::{Builder, Header};

    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = Builder::new(&mut encoder);
            for (path, data) in entries {
                let mut header = Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_entry_type(EntryType::Regular);
                header.set_cksum();
                let mut bytes = path.as_bytes().to_vec();
                if bytes.len() <= 100 {
                    bytes.resize(100, 0);
                    header.as_old_mut().name.copy_from_slice(&bytes);
                    header.set_cksum();
                    builder.append(&header, *data).unwrap();
                } else {
                    builder.append_data(&mut header, path, *data).unwrap();
                }
            }
            builder.finish().unwrap();
        }
        encoder.finish().unwrap()
    }

    fn append_raw(
        builder: &mut Builder<&mut GzEncoder<Vec<u8>>>,
        path: &str,
        entry_type: EntryType,
        size: u64,
        link: Option<&str>,
    ) {
        let mut header = Header::new_gnu();
        header.set_size(size);
        header.set_mode(0o644);
        header.set_entry_type(entry_type);
        if let Some(link) = link {
            header.set_link_name(link).unwrap();
        }
        let mut name = path.as_bytes().to_vec();
        name.resize(100, 0);
        header.as_old_mut().name.copy_from_slice(&name);
        header.set_cksum();
        builder.append(&header, std::io::empty()).unwrap();
    }

    fn tar_gz_with(f: impl FnOnce(&mut Builder<&mut GzEncoder<Vec<u8>>>)) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = Builder::new(&mut encoder);
            f(&mut builder);
            builder.finish().unwrap();
        }
        encoder.finish().unwrap()
    }

    fn sha512_sri(bytes: &[u8]) -> String {
        format!("sha512-{}", STANDARD.encode(Sha512::digest(bytes)))
    }

    #[test]
    fn unpack_returns_files_and_strips_the_package_prefix() {
        let archive = tar_gz(&[
            ("package/package.json", br#"{"name":"demo"}"#),
            ("package/index.js", b"module.exports = 1;\n"),
        ]);
        let files = unpack_tarball(&archive).unwrap();
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            vec!["index.js", "package.json"]
        );
        assert_eq!(files[0].text(), Some("module.exports = 1;\n"));
    }

    #[test]
    fn path_traversal_is_rejected() {
        let archive = tar_gz(&[("package/../../etc/passwd", b"owned")]);
        assert!(matches!(
            unpack_tarball(&archive),
            Err(TarballError::UnsafePath(path)) if path.contains("..")
        ));
    }

    #[test]
    fn absolute_paths_are_rejected() {
        let archive = tar_gz(&[("/etc/passwd", b"owned")]);
        assert!(matches!(
            unpack_tarball(&archive),
            Err(TarballError::UnsafePath(_))
        ));
    }

    #[test]
    fn backslash_and_drive_paths_are_rejected() {
        for path in ["package\\..\\evil", "C:/Windows/evil"] {
            let archive = tar_gz(&[(path, b"owned")]);
            assert!(
                matches!(unpack_tarball(&archive), Err(TarballError::UnsafePath(_))),
                "{path} was accepted"
            );
        }
    }

    #[test]
    fn symlinks_and_hard_links_are_rejected() {
        let symlink = tar_gz_with(|builder| {
            append_raw(
                builder,
                "package/evil",
                EntryType::Symlink,
                0,
                Some("/etc/passwd"),
            );
        });
        assert!(matches!(
            unpack_tarball(&symlink),
            Err(TarballError::Link(_))
        ));

        let hardlink = tar_gz_with(|builder| {
            append_raw(
                builder,
                "package/evil",
                EntryType::Link,
                0,
                Some("package/index.js"),
            );
        });
        assert!(matches!(
            unpack_tarball(&hardlink),
            Err(TarballError::Link(_))
        ));
    }

    #[test]
    fn size_bombs_are_rejected_before_data_is_read() {
        let limits = UnpackLimits {
            max_total_bytes: 128,
            max_file_bytes: 128,
            ..UnpackLimits::default()
        };
        let archive = tar_gz_with(|builder| {
            append_raw(builder, "package/big", EntryType::Regular, 1024, None);
        });
        assert!(matches!(
            unpack_tarball_with_limits(&archive, &limits),
            Err(TarballError::TooLarge { limit: 128 })
        ));
    }

    #[test]
    fn total_unpacked_size_is_capped_across_entries() {
        let limits = UnpackLimits {
            max_total_bytes: 10,
            max_file_bytes: 10,
            ..UnpackLimits::default()
        };
        let archive = tar_gz(&[("package/a", b"123456"), ("package/b", b"123456")]);
        assert!(matches!(
            unpack_tarball_with_limits(&archive, &limits),
            Err(TarballError::TooLarge { limit: 10 })
        ));
    }

    #[test]
    fn file_count_is_capped() {
        let limits = UnpackLimits {
            max_files: 2,
            ..UnpackLimits::default()
        };
        let archive = tar_gz(&[
            ("package/a", b"a"),
            ("package/b", b"b"),
            ("package/c", b"c"),
        ]);
        assert!(matches!(
            unpack_tarball_with_limits(&archive, &limits),
            Err(TarballError::TooManyFiles { limit: 2 })
        ));
    }

    #[test]
    fn duplicate_paths_are_rejected() {
        let archive = tar_gz(&[("package/a", b"a"), ("package/a", b"b")]);
        assert!(matches!(
            unpack_tarball(&archive),
            Err(TarballError::DuplicatePath(_))
        ));
    }

    #[test]
    fn non_utf8_and_overlong_paths_are_rejected() {
        let mut raw = b"package/".to_vec();
        raw.extend_from_slice(&[0xff, 0xfe]);
        let archive = tar_gz_with(|builder| {
            let mut header = Header::new_gnu();
            header.set_size(1);
            header.set_mode(0o644);
            header.set_entry_type(EntryType::Regular);
            let mut name = raw.clone();
            name.resize(100, 0);
            header.as_old_mut().name.copy_from_slice(&name);
            header.set_cksum();
            builder.append(&header, &b"x"[..]).unwrap();
        });
        assert!(matches!(
            unpack_tarball(&archive),
            Err(TarballError::UnsafePath(_))
        ));

        let long = format!("package/{}", "a".repeat(MAX_PATH_BYTES));
        let archive = tar_gz(&[(&long, b"x")]);
        assert!(matches!(
            unpack_tarball(&archive),
            Err(TarballError::UnsafePath(_))
        ));
    }

    #[test]
    fn verifies_a_sha512_sri_digest() {
        let bytes = b"the tarball body";
        let sri = sha512_sri(bytes);
        assert!(verify_integrity(bytes, &sri).is_ok());
        assert!(matches!(
            verify_integrity(b"tampered", &sri),
            Err(TarballError::IntegrityMismatch)
        ));
    }

    #[test]
    fn accepts_sha512_among_several_sri_tokens() {
        let bytes = b"body";
        let sri = format!("sha256-deadbeef {}", sha512_sri(bytes));
        assert!(verify_integrity(bytes, &sri).is_ok());
    }

    #[test]
    fn rejects_missing_or_malformed_sha512() {
        assert!(matches!(
            verify_integrity(b"body", "sha256-deadbeef"),
            Err(TarballError::MissingSha512)
        ));
        assert!(matches!(
            verify_integrity(b"body", "sha512-not!base64"),
            Err(TarballError::InvalidIntegrity(_))
        ));
        assert!(matches!(
            verify_integrity(b"body", ""),
            Err(TarballError::MissingSha512)
        ));
    }

    #[test]
    fn download_is_capped_and_verified() {
        let mut server = mockito::Server::new();
        let body = tar_gz(&[("package/index.js", b"ok")]);
        let mock = server
            .mock("GET", "/demo.tgz")
            .with_body(body.clone())
            .expect(2)
            .create();
        let client = PoliteClient::new(
            std::time::Duration::from_millis(1),
            "DepRadar test agent",
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        let url = format!("{}/demo.tgz", server.url());
        let downloaded = download_tarball_with(&client, &url, 1024).unwrap();
        assert_eq!(downloaded, body);
        verify_integrity(&downloaded, &sha512_sri(&body)).unwrap();

        assert!(matches!(
            download_tarball_with(&client, &url, 4),
            Err(TarballError::TooLarge { limit: 4 })
        ));
        mock.assert();
    }

    #[test]
    fn npm_dist_requires_integrity() {
        let mut server = mockito::Server::new();
        let body = tar_gz(&[("package/index.js", b"ok")]);
        let mock = server
            .mock("GET", "/demo.tgz")
            .with_body(body.clone())
            .create();
        let dist = NpmDist {
            tarball: format!("{}/demo.tgz", server.url()),
            integrity: Some(sha512_sri(&body)),
        };
        assert_eq!(download_npm_dist(&dist).unwrap(), body);
        mock.assert();

        let no_integrity = NpmDist {
            tarball: dist.tarball,
            integrity: None,
        };
        assert!(matches!(
            download_npm_dist(&no_integrity),
            Err(TarballError::MissingSha512)
        ));
    }

    #[test]
    fn corrupt_gzip_is_an_error_not_a_panic() {
        assert!(matches!(
            unpack_tarball(b"not a gzip stream"),
            Err(TarballError::Archive(_))
        ));
    }

    #[test]
    fn empty_archive_is_allowed() {
        let archive = tar_gz(&[]);
        assert_eq!(unpack_tarball(&archive).unwrap(), Vec::new());
    }
}
