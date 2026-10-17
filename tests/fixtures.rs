//! Housekeeping for the real-world lockfile corpus in `tests/fixtures/`
//! (`npm/`, `pypi/`, `cargo/`): every file is attributed in the README and
//! stays small.

use std::fs;
use std::path::{Path, PathBuf};

/// Directories holding lockfiles copied from open-source projects.
const REAL_WORLD_DIRS: &[&str] = &["npm", "pypi", "cargo"];

/// Fixture files may not grow past this.
const MAX_FIXTURE_BYTES: u64 = 200 * 1024;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// `dir/file` for every file in the real-world directories, sorted.
fn real_world_fixtures() -> Vec<String> {
    let mut files = Vec::new();
    for dir in REAL_WORLD_DIRS {
        for entry in fs::read_dir(fixtures_dir().join(dir)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                files.push(format!("{dir}/{}", entry.file_name().to_str().unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn there_is_a_real_lockfile_for_every_supported_format() {
    let files = real_world_fixtures();
    for expected in [
        "npm/package-lock.json",
        "npm/package.json",
        "pypi/requirements-dev.txt",
        "pypi/Pipfile.lock",
        "cargo/Cargo.lock",
    ] {
        assert!(files.iter().any(|f| f == expected), "missing {expected}");
    }
}

#[test]
fn every_real_world_fixture_is_attributed_in_the_readme() {
    let readme = fs::read_to_string(fixtures_dir().join("README.md")).unwrap();
    for file in real_world_fixtures() {
        assert!(
            readme.contains(&format!("[`{file}`]({file})")),
            "tests/fixtures/README.md has no source/license row for {file}"
        );
    }
}

#[test]
fn every_real_world_fixture_is_under_200_kb() {
    for file in real_world_fixtures() {
        let size = fs::metadata(fixtures_dir().join(&file)).unwrap().len();
        assert!(size <= MAX_FIXTURE_BYTES, "{file} is {size} bytes");
    }
}
