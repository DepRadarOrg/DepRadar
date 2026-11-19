# Behavioral Analysis Modules (`src/behavioral/`)

While DepRadar's typosquat and CVE modules evaluate a package's *name* and reported vulnerabilities, the behavioral detectors inspect its *unpacked contents*. They identify risks that execute upon installation, communicate over the network, drop precompiled machine code, or hide malicious payloads behind obfuscation.

Behavioral findings are represented as [`BehavioralFinding`](mod.rs), containing the package-relative file path, an optional 1-based line number for source checks, a descriptive explanation, and a [`Severity`](../cve/severity.rs) rating consistent with [`docs/severity.md`](../../docs/severity.md).

---

## Detectors Overview

| Detector | Primary Focus | Evaluated Files | Default Severity |
| :--- | :--- | :--- | :--- |
| [`scripts`](scripts.rs) | npm lifecycle install scripts | `package.json` | **Medium** |
| [`network`](network.rs) | Shell pipes, network requests & credential theft | `package.json` scripts & invoked local files | **Medium / High** |
| [`setup_py`](setup_py.rs) | Python install-time command execution, obfuscation & network | `setup.py` | **Medium / High** |
| [`binaries`](binaries.rs) | Precompiled native binaries and shared objects | Entire package directory | **Low** (Informational) |
| [`obfuscation`](obfuscation.rs) | Encoded payloads, entropy, and dynamic evaluation | JavaScript (`.js`, `.mjs`, `.cjs`) | **Medium / High** |
| [`diff`](diff.rs) | Behavioral drift and added scripts across versions | Old vs. new unpacked trees | Inherits / **Medium** |
| [`tarball`](tarball.rs) | Safe archive extraction and SRI integrity checks | `.tgz` / `.tar.gz` packages | N/A (Ingestion boundary) |

---

## Detectors

### npm Lifecycle Scripts ([`scripts.rs`](scripts.rs))
The `scripts` detector parses `package.json` to detect declared install-time lifecycle hooks: `preinstall`, `install`, `postinstall`, and `prepare`. Because npm executes these hooks automatically with the privileges of the installing user, they represent the primary initial access vector for npm malware. The detector operates strictly declaratively without executing commands or invoking shells. Every declared lifecycle hook is reported as a **Medium** severity finding, prompting human review even if the command appears ordinary (such as native build wrappers).

### Network & Credential Access in Scripts ([`network.rs`](network.rs))
The `network` detector scans npm install scripts and the local files they reference for common payload delivery, beaconing, and secret exfiltration patterns. It flags pipe-to-shell patterns (e.g. `curl` or `wget` piped into `sh`/`bash`/`zsh` or evaluated inside subshells) and access to sensitive secrets (such as reading `process.env.NPM_TOKEN`, `AWS_SECRET_ACCESS_KEY`, `~/.ssh/id_rsa`, or `~/.npmrc`) as **High** severity findings. Direct HTTP/HTTPS requests targeting raw IPv4 addresses rather than domain hostnames are flagged as **Medium** severity because they bypass DNS controls and domain allowlists. Each finding captures a truncated line snippet (`MAX_SNIPPET_CHARS = 80`) so reviewers can inspect the payload without opening the package.

### Python `setup.py` Execution Calls ([`setup_py.rs`](setup_py.rs))
The `setup_py` detector inspects `setup.py` source text to catch install-time execution, dynamic evaluation, and network beaconing invoked when `pip` builds or installs a source distribution. It strips comments and flags command execution calls (`subprocess`, `os.system`, `exec`, `eval`) as **High** severity since they run arbitrary system code on install. Calls that deobfuscate embedded blobs (`base64.b64decode`) or make outbound network connections (`urllib`, `requests`) are flagged as **Medium** severity. Findings record the exact 1-based line number of each call.

### Precompiled Native Binaries ([`binaries.rs`](binaries.rs))
The `binaries` detector traverses an unpacked package to discover precompiled machine code that cannot be easily reviewed like Python or JavaScript source files. It checks for ELF, Windows PE (`MZ`), and macOS Mach-O magic byte headers, as well as native file extensions (`.exe`, `.dll`, `.so`, `.dylib`). Shipped binaries are emitted as a single consolidated **Low** severity finding per package. Because legitimate packages frequently distribute prebuilt native bindings, this detector acts as an informational prompt for human review rather than a blocking failure.

### Obfuscated JavaScript Heuristics ([`obfuscation.rs`](obfuscation.rs))
The `obfuscation` detector scores JavaScript files across four heuristic signals: long base64 string literals (200+ characters, weight 3), long hex string literals (200+ characters, weight 4), dynamic evaluation constructs outside comments (`eval` weight 2, `new Function` weight 3, `atob` weight 2), and high Shannon entropy ($\ge 4.8$ on strings of 128+ characters, weight 2 each, max 4). Files with lines exceeding 2,000 characters receive 1 point, ensuring standard minified bundles alone stay well below the reporting threshold (`OBFUSCATION_THRESHOLD = 4`). Files scoring 4–7 are flagged as **Medium** severity, while heavily obfuscated payloads scoring 8 or higher (`HIGH_SEVERITY_THRESHOLD = 8`) escalate to **High** severity.

### Version-to-Version Behavioral Diffing ([`diff.rs`](diff.rs))
The `diff` detector compares two unpacked package versions (the currently installed release vs. an incoming candidate) to identify behavioral drift between releases. It detects newly introduced install scripts (such as a patch update introducing a `setup.py` or npm `postinstall` script) rated at **Medium** severity (`NEW_SCRIPT_SEVERITY`), as well as any new suspicious `setup.py` calls (which retain their respective detector severity). Findings are tracked by type and target file rather than line offsets, avoiding spurious alerts when benign code edits merely shift line numbers.

### Sandboxed Archive Ingestion ([`tarball.rs`](tarball.rs))
While not emitting standalone findings directly, `tarball` provides the security-critical extraction sandbox through which all package contents enter behavioral analysis. It streams registry tarballs into memory under strict resource caps (`MAX_TARBALL_BYTES = 50 MB`, `MAX_UNPACKED_BYTES = 50 MB`, `MAX_FILES = 10,000`), validates Subresource Integrity (SHA-512) hashes against registry metadata, and rejects directory traversal (`..`), absolute paths, Windows drive paths, symlinks, and hardlinks. This guarantees that malformed or malicious archives cannot compromise the host during analysis.
