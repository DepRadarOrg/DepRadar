//! Deprecated: use the `depradar` binary instead.
//!
//! Usage: `scan <lockfile | package-name>`. This is the same as `depradar scan
//! <lockfile>` or `depradar name <package-name>`. It prints a deprecation
//! notice to stderr and otherwise behaves as before.

use std::env;
use std::path::Path;
use std::process::ExitCode;

use deprader::ignore::{IgnoreFile, IGNORE_FILE_NAME};
use deprader::pipeline::{scan_lockfile, ScanOptions};
use deprader::typosquat::scanner::score_package;

fn main() -> ExitCode {
    eprintln!(
        "warning: `scan` is deprecated; use `depradar name <package>` or \
         `depradar scan <path>` instead"
    );

    let target = match env::args().nth(1) {
        Some(target) => target,
        None => {
            eprintln!("usage: scan <lockfile | package-name>");
            return ExitCode::from(2);
        }
    };

    if !Path::new(&target).is_file() {
        println!("{}", score_package(&target));
        return ExitCode::SUCCESS;
    }

    let ignore_path = Path::new(&target)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(IGNORE_FILE_NAME);
    let ignore = if ignore_path.is_file() {
        match IgnoreFile::load(&ignore_path) {
            Ok(ignore) => Some(ignore),
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };

    let opts = ScanOptions {
        ignore: ignore.as_ref(),
        ..ScanOptions::default()
    };
    match scan_lockfile(&target, &opts) {
        Ok(report) => {
            println!("{report}");
            if report.has_findings() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}
