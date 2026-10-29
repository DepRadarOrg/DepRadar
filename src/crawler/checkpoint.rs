//! Persistent crawl-progress checkpoints.
//!
//! A [`CheckpointStore`] keeps a JSON file — `checkpoints.json` — that maps
//! crawler names to their last-seen cursor (e.g. a timestamp, page token, or
//! sequence number). Writes are atomic: the new state is serialized to a
//! sibling temp file and then renamed over the target, so a crash mid-write
//! never leaves a corrupt or half-written checkpoint file.
//!
//! # Example
//!
//! ```no_run
//! use deprader::crawler::checkpoint::CheckpointStore;
//!
//! let mut store = CheckpointStore::open("/var/lib/deprader/checkpoints.json").unwrap();
//! let cursor = store.get("npm").unwrap_or_default();
//! // … crawl from cursor …
//! store.set("npm", "2024-11-01T00:00:00Z").unwrap();
//! ```

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// In-memory map of crawler → last cursor, backed by a file on disk.
#[derive(Debug, Default)]
pub struct CheckpointStore {
    path: PathBuf,
    checkpoints: HashMap<String, String>,
}

/// The JSON shape stored on disk.
#[derive(Debug, Default, Serialize, Deserialize)]
struct OnDisk {
    #[serde(flatten)]
    checkpoints: HashMap<String, String>,
}

impl CheckpointStore {
    /// Opens (or creates) the checkpoint file at `path`.
    ///
    /// If the file does not exist the store starts empty — callers treat a
    /// missing key as the default starting cursor, so a fresh installation
    /// just works.
    ///
    /// Returns an error only if the file exists but cannot be read or parsed.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let checkpoints = match fs::read_to_string(&path) {
            Ok(contents) => {
                let on_disk: OnDisk = serde_json::from_str(&contents).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("malformed checkpoints file: {e}"),
                    )
                })?;
                on_disk.checkpoints
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => return Err(e),
        };

        Ok(Self { path, checkpoints })
    }

    /// Returns the last-seen cursor for `crawler`, or `None` if it has never
    /// been checkpointed.
    pub fn get(&self, crawler: &str) -> Option<&str> {
        self.checkpoints.get(crawler).map(String::as_str)
    }

    /// Persists a new `cursor` for `crawler`.
    ///
    /// The write is atomic: the serialized JSON is first written to a
    /// temporary file in the same directory as the checkpoint file, then
    /// renamed over it. A crash between the write and the rename leaves the
    /// original file untouched and the temporary file behind; it is cleaned
    /// up on the next successful write.
    pub fn set(&mut self, crawler: &str, cursor: impl Into<String>) -> io::Result<()> {
        self.checkpoints.insert(crawler.to_string(), cursor.into());
        self.flush()
    }

    /// Writes the current in-memory state to disk atomically.
    fn flush(&self) -> io::Result<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        // Write to a sibling temp file in the same directory so that the
        // rename (which is atomic on POSIX when src and dst are on the same
        // filesystem) is the only moment the destination is replaced.
        let tmp_path = self.path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&OnDisk {
            checkpoints: self.checkpoints.clone(),
        })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        fs::write(&tmp_path, json)?;
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// A round-trip: open → set → re-open → get.
    #[test]
    fn round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("checkpoints.json");

        let mut store = CheckpointStore::open(&path).unwrap();
        store.set("npm", "cursor-42").unwrap();
        store.set("pypi", "cursor-99").unwrap();

        // Re-open from disk.
        let store2 = CheckpointStore::open(&path).unwrap();
        assert_eq!(store2.get("npm"), Some("cursor-42"));
        assert_eq!(store2.get("pypi"), Some("cursor-99"));
    }

    /// A missing file returns `None` for every key — it is not an error.
    #[test]
    fn missing_file_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does_not_exist.json");

        let store = CheckpointStore::open(&path).unwrap();
        assert_eq!(store.get("npm"), None);
    }

    /// The temporary file must not outlive a successful write.
    /// Before the write the `.json.tmp` file should not exist; after it
    /// should also be gone (renamed away).
    #[test]
    fn temp_file_is_not_left_behind_after_successful_write() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("checkpoints.json");
        let tmp_path = path.with_extension("json.tmp");

        assert!(!tmp_path.exists(), "tmp file should not exist before any write");

        let mut store = CheckpointStore::open(&path).unwrap();
        store.set("npm", "cursor-1").unwrap();

        assert!(!tmp_path.exists(), "tmp file should be gone after a successful write");
        assert!(path.exists(), "checkpoint file should exist after write");
    }

    /// Overwriting a cursor updates the value.
    #[test]
    fn set_overwrites_existing_cursor() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("checkpoints.json");

        let mut store = CheckpointStore::open(&path).unwrap();
        store.set("npm", "cursor-1").unwrap();
        store.set("npm", "cursor-2").unwrap();

        let store2 = CheckpointStore::open(&path).unwrap();
        assert_eq!(store2.get("npm"), Some("cursor-2"));
    }

    /// `get` returns `None` for an unknown key even after other keys have
    /// been written.
    #[test]
    fn get_unknown_key_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("checkpoints.json");

        let mut store = CheckpointStore::open(&path).unwrap();
        store.set("npm", "cursor-1").unwrap();

        assert_eq!(store.get("pypi"), None);
    }
}
