//! Native binaries shipped inside a package.
//!
//! A prebuilt executable in an npm or PyPI package can't be code-reviewed
//! the way JavaScript or Python can, and dropped binaries are a common
//! payload carrier. [`scan_binaries`] walks an unpacked package tree and
//! reports every file that either carries an ELF, PE or Mach-O magic
//! header or has a native-library extension (`.exe`, `.dll`, `.so`,
//! `.dylib`).
//!
//! The whole scan is one low-severity, informational
//! [`BehavioralFinding`] (`severity_low` in `docs/severity.md`) listing
//! the files — the point is to put the list in front of a human, not to
//! claim malice. Real projects do ship legitimate native wheels and
//! addons, so this is a review prompt, not a blocker.

use std::fs;
use std::io::Read;
use std::path::Path;

use crate::cve::severity::Severity;

use super::{collect_files, BehavioralFinding};

/// The single, low-severity finding [`scan_binaries`] emits.
pub const SEVERITY: Severity = Severity::Low;

/// Executable/object formats recognised by magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFormat {
    /// `\x7fELF` — Linux/Unix executables and shared objects.
    Elf,
    /// `MZ` — Windows PE executables and DLLs.
    Pe,
    /// `0xFEEDFACF`/`0xFEEDFACE` (and byte-swapped) — macOS binaries.
    MachO,
}

impl BinaryFormat {
    fn from_magic(header: &[u8]) -> Option<Self> {
        if header.starts_with(&[0x7f, b'E', b'L', b'F']) {
            Some(BinaryFormat::Elf)
        } else if header.starts_with(b"MZ") {
            Some(BinaryFormat::Pe)
        } else if header.len() >= 4 {
            let magic = [header[0], header[1], header[2], header[3]];
            // MH_MAGIC / MH_CIGAM / MH_MAGIC_64 / MH_CIGAM_64.
            const MACHO: [[u8; 4]; 4] = [
                [0xfe, 0xed, 0xfa, 0xce],
                [0xce, 0xfa, 0xed, 0xfe],
                [0xfe, 0xed, 0xfa, 0xcf],
                [0xcf, 0xfa, 0xed, 0xfe],
            ];
            if MACHO.contains(&magic) {
                Some(BinaryFormat::MachO)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Extension-based detection for `.exe`/`.dll`/`.so`/`.dylib`, including
/// versioned shared objects (`libfoo.so.1.2`).
fn has_native_extension(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    const EXTS: &[&str] = &[".exe", ".dll", ".so", ".dylib"];
    EXTS.iter()
        .any(|ext| name.ends_with(ext) || name.contains(&format!("{ext}.")))
}

/// One file flagged as a native binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBinary {
    /// Package-relative path (`/`-separated).
    pub file: String,
    /// Format from magic bytes; `None` when only the extension matched.
    pub format: Option<BinaryFormat>,
}

/// Scans the unpacked package at `root`, returning every native binary
/// found, sorted by path. Files are sniffed (first 8 bytes) before their
/// extension is consulted, so a `.so` that is actually text is still
/// reported but an extension-less ELF is too.
pub fn scan_binaries(root: &Path) -> std::io::Result<Vec<NativeBinary>> {
    let mut binaries = Vec::new();
    for file in collect_files(root)? {
        let path = root.join(&file);
        let format = sniff(&path)?;
        match format {
            Some(format) => binaries.push(NativeBinary {
                file,
                format: Some(format),
            }),
            None => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if has_native_extension(&name) {
                    binaries.push(NativeBinary { file, format: None });
                }
            }
        }
    }
    Ok(binaries)
}

/// [`scan_binaries`] wrapped as the informational finding reports consume.
pub fn scan_binaries_finding(root: &Path) -> std::io::Result<Option<BehavioralFinding>> {
    let binaries = scan_binaries(root)?;
    if binaries.is_empty() {
        return Ok(None);
    }
    let mut list = Vec::new();
    for binary in &binaries {
        match binary.format {
            Some(format) => list.push(format!("{} ({format})", binary.file)),
            None => list.push(binary.file.clone()),
        }
    }
    Ok(Some(BehavioralFinding {
        file: String::new(),
        line: None,
        severity: SEVERITY,
        message: format!("ships native binaries: {}", list.join(", ")),
    }))
}

/// Reads the first bytes of `path` and matches them against the known
/// magic numbers. I/O errors propagate; short files just don't match.
fn sniff(path: &Path) -> std::io::Result<Option<BinaryFormat>> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 8];
    let read = file.read(&mut header)?;
    Ok(BinaryFormat::from_magic(&header[..read]))
}

impl std::fmt::Display for BinaryFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            BinaryFormat::Elf => "ELF",
            BinaryFormat::Pe => "PE",
            BinaryFormat::MachO => "Mach-O",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn recognises_magic_headers() {
        let elf = [0x7f, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00];
        assert_eq!(BinaryFormat::from_magic(&elf), Some(BinaryFormat::Elf));

        let pe = b"MZ\x90\x00\x03\x00\x00\x00";
        assert_eq!(BinaryFormat::from_magic(pe), Some(BinaryFormat::Pe));

        // 64-bit little-endian Mach-O, the common macOS layout.
        let macho64le = [0xcf, 0xfa, 0xed, 0xfe, 0x07, 0x00, 0x00, 0x01];
        assert_eq!(
            BinaryFormat::from_magic(&macho64le),
            Some(BinaryFormat::MachO)
        );
        // 32-bit big-endian Mach-O.
        let macho32be = [0xfe, 0xed, 0xfa, 0xce, 0x00, 0x00, 0x00, 0x01];
        assert_eq!(
            BinaryFormat::from_magic(&macho32be),
            Some(BinaryFormat::MachO)
        );

        assert_eq!(BinaryFormat::from_magic(b"#!/bin/sh\n"), None);
        assert_eq!(BinaryFormat::from_magic(b""), None);
        assert_eq!(BinaryFormat::from_magic(b"M"), None);
    }

    #[test]
    fn extension_matching_covers_versioned_shared_objects() {
        assert!(has_native_extension("payload.exe"));
        assert!(has_native_extension("payload.EXE"));
        assert!(has_native_extension("helper.dll"));
        assert!(has_native_extension("native.so"));
        assert!(has_native_extension("libfoo.so.1"));
        assert!(has_native_extension("libfoo.so.1.2.3"));
        assert!(has_native_extension("addon.dylib"));
        assert!(!has_native_extension("setup.py"));
        assert!(!has_native_extension("README.md"));
        assert!(!has_native_extension("something.soapy"));
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn scan_reports_magic_and_extension_hits_across_the_tree() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(&root.join("index.js"), b"module.exports = {};\n");
        write(&root.join("README.md"), b"# hello\n");
        write(&root.join("bin/dropper"), b"\x7fELF\x02\x01\x01\x00padding");
        write(
            &root.join("vendor/helper.dll"),
            b"MZ\x90\x00 not really a PE",
        );
        write(&root.join("native.dylib"), b"not a real mach-o but .dylib");
        write(&root.join("lib.so.1"), b"versioned shared object by name");
        // An extension-less Mach-O hiding deeper in the tree.
        write(
            &root.join("vendor/nested/mod"),
            b"\xfe\xed\xfa\xcf\x07\x00\x00\x01",
        );

        let binaries = scan_binaries(root).unwrap();
        let files: Vec<&str> = binaries.iter().map(|b| b.file.as_str()).collect();
        assert_eq!(
            files,
            vec![
                "bin/dropper",
                "lib.so.1",
                "native.dylib",
                "vendor/helper.dll",
                "vendor/nested/mod",
            ]
        );
        assert_eq!(binaries[0].format, Some(BinaryFormat::Elf));
        assert_eq!(binaries[1].format, None, "extension-only hit");
        assert_eq!(binaries[2].format, None, "extension-only hit");
        assert_eq!(binaries[3].format, Some(BinaryFormat::Pe));
        assert_eq!(binaries[4].format, Some(BinaryFormat::MachO));
    }

    #[test]
    fn clean_text_package_yields_nothing() {
        let dir = tempdir().unwrap();
        write(&dir.path().join("package.json"), b"{}\n");
        write(&dir.path().join("src/index.js"), b"export default 1;\n");
        assert_eq!(scan_binaries(dir.path()).unwrap(), Vec::new());
    }

    #[test]
    fn informational_finding_lists_every_binary_at_low_severity() {
        let dir = tempdir().unwrap();
        write(&dir.path().join("a.exe"), b"MZ");
        write(&dir.path().join("b"), b"\x7fELF\x02");
        let finding = scan_binaries_finding(dir.path()).unwrap().unwrap();
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.line, None);
        assert!(
            finding.message.contains("a.exe (PE)"),
            "{}",
            finding.message
        );
        assert!(finding.message.contains("b (ELF)"), "{}", finding.message);
    }

    #[test]
    fn no_finding_for_a_binary_free_package() {
        let dir = tempdir().unwrap();
        write(
            &dir.path().join("setup.py"),
            b"from setuptools import setup\n",
        );
        assert!(scan_binaries_finding(dir.path()).unwrap().is_none());
    }
}
