//! Manual CLI for the combined typosquat scorer.
//!
//! Usage: `scan <package-name>` — scores the given name against the bundled
//! popular-package dataset with both the edit-distance and homoglyph signals
//! and prints the result.
//!
//! Pass `--json` before the name to print the result as JSON.

use std::env;
use std::process::ExitCode;

use deprader::typosquat::scanner::score_package;
fn main() -> ExitCode {
    let args = env::args().skip(1);
    let mut json_output = false;
    let mut name: Option<String> = None;

    for arg in args {
        if arg == "--json" {
            json_output = true;
        } else if name.is_none() {
            name = Some(arg);
        } else {
            eprintln!("usage: scan [--json] <package-name>");
            return ExitCode::FAILURE;
        }
    }

    let name = match name {
        Some(name) => name,
        None => {
            eprintln!("usage: scan [--json] <package-name>");
            return ExitCode::FAILURE;
        }
    };

    let result = score_package(&name);
    if json_output {
        match serde_json::to_string_pretty(&result) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("failed to render JSON: {err}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        println!("{}", result);
    }
    ExitCode::SUCCESS
}
