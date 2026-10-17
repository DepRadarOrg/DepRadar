//! Manual CLI for the OSV client.
//!
//! Usage: `osv [--no-cache] <npm|pypi|crates|go|maven|rubygems|packagist> <name> <version>`
//! — prints the OSV advisory IDs for that exact version. Responses are cached
//! for 24h under the OS cache dir unless `--no-cache` is given.

use std::env;
use std::process::ExitCode;

use deprader::cve::cache::OsvCache;
use deprader::cve::osv_client::{Ecosystem, OsvClient};

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let no_cache = args.iter().any(|a| a == "--no-cache");
    args.retain(|a| a != "--no-cache");

    let [eco, name, version] = args.as_slice() else {
        eprintln!("usage: osv [--no-cache] <ecosystem> <name> <version>");
        return ExitCode::FAILURE;
    };
    let ecosystem = match eco.to_lowercase().as_str() {
        "npm" => Ecosystem::Npm,
        "pypi" => Ecosystem::PyPI,
        "crates" | "crates.io" => Ecosystem::CratesIo,
        "go" => Ecosystem::Go,
        "maven" => Ecosystem::Maven,
        "rubygems" => Ecosystem::RubyGems,
        "packagist" => Ecosystem::Packagist,
        other => {
            eprintln!("unknown ecosystem: {other}");
            return ExitCode::FAILURE;
        }
    };

    let mut client = OsvClient::new();
    if !no_cache {
        if let Some(cache) = OsvCache::default_location() {
            client = client.with_cache(cache);
        }
    }

    match client.query(name, ecosystem, version) {
        Ok(response) => {
            for vuln in response.vulns {
                println!("{}", vuln.id);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
