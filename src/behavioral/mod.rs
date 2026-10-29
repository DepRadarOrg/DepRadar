//! Behavioral analysis of unpacked packages.
//!
//! Where the typosquat and CVE modules reason about a package's *name* and
//! version, the detectors here reason about its *contents*: what runs when
//! it's installed ([`setup_py`]), what machine code it ships
//! ([`binaries`]), and what changed between two releases ([`diff`]).
//!
//! Behavioral findings are their own type ([`BehavioralFinding`]), not
//! [`Finding`](crate::model::Finding)s — they describe files inside one
//! package rather than a dependency in a lockfile. Each carries a
//! [`Severity`] so reports can rank them the same way as lockfile findings;
//! the level definitions live in `docs/severity.md`.

pub mod binaries;
pub mod diff;
pub mod network;
pub mod obfuscation;
pub mod scripts;
pub mod setup_py;
pub mod tarball;

use std::fmt;

use crate::cve::severity::Severity;

/// One suspicious thing a behavioral detector found inside a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehavioralFinding {
    /// Package-relative path of the file the finding is about, using `/`
    /// separators (e.g. `setup.py`, `vendor/helper.dll`).
    pub file: String,
    /// 1-based line number, when the detector works on text (e.g.
    /// [`setup_py`]); `None` for whole-file checks such as [`binaries`].
    pub line: Option<usize>,
    /// How serious the finding is; see `docs/severity.md`.
    pub severity: Severity,
    /// Human-readable description, e.g. `subprocess call`.
    pub message: String,
}

impl fmt::Display for BehavioralFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{line}: ", self.file)?,
            None => write!(f, "{}: ", self.file)?,
        }
        write!(f, "{}", self.message)
    }
}

/// Collects the package-relative path of every regular file under `root`,
/// sorted, using `/` as the separator on every platform. Directory symlinks
/// are not followed and non-file entries are skipped.
pub(crate) fn collect_files(root: &std::path::Path) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    collect_into(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_into(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_into(root, &path, out)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_display_includes_file_line_and_message() {
        let finding = BehavioralFinding {
            file: "setup.py".to_string(),
            line: Some(3),
            severity: Severity::High,
            message: "subprocess call".to_string(),
        };
        assert_eq!(finding.to_string(), "setup.py:3: subprocess call");

        let whole_file = BehavioralFinding {
            file: "bin/x".to_string(),
            line: None,
            severity: Severity::Low,
            message: "native binary".to_string(),
        };
        assert_eq!(whole_file.to_string(), "bin/x: native binary");
    }
}
