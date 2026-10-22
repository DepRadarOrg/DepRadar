# DepRadar

![DepRadar](./assets/banner.svg)

**A decentralized security scanner for open-source dependency graphs — funded bounties, not empty advisories.**

[![CI](https://github.com/DepRadarOrg/DepRadar/actions/workflows/ci.yml/badge.svg)](https://github.com/DepRadarOrg/DepRadar/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Network](https://img.shields.io/badge/network-Stellar%20Testnet-blue)](https://stellar.expert/explorer/testnet/contract/CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ)

## Mission

DepRadar crawls open-source package registries, scans dependency graphs for
known vulnerabilities, maintainer-account takeovers, and typosquatting, and
pays out bounties through [Drips](https://www.drips.network/) to whoever
catches the issue first — verifiably, on-chain, without a foundation or a
platform sitting between the finder and the payout.

## Architecture

```mermaid
flowchart LR
    Crawler[Crawler] -->|discovers packages / versions| Scanner[Scanner<br/>CVE match · typosquat · behavioral]
    Scanner -->|finding: create_bounty| Router[Bounty Router<br/>Soroban contract]
    Router -->|release: payout| Drips[Drips]
    Router -.get_bounty / get_claim.-> Dashboard[Dashboard]

    classDef live    fill:#134e4a,stroke:#2dd4bf,color:#e6fffa;
    classDef planned fill:#1e293b,stroke:#475569,color:#94a3b8,stroke-dasharray:4;
    class Router,Scanner,Dashboard,Crawler live;
    class Drips planned;
```

**Solid teal** = live / substantially complete.  **Dashed** = planned — see
[Status & Roadmap](#status--roadmap).

- **Crawler** uses a shared checkpoint/event contract. The npm crawler reads
  the registry changes feed for new packages and versions; additional
  registries and scanner wiring are still in progress.
- **Scanner** evaluates what the crawler surfaces against three detection
  classes: known-CVE matching, typosquat similarity, and behavioral
  analysis (install scripts, obfuscation, anomalous permission requests).
  The typosquat subsystem is substantially complete — popular-package
  reference dataset, Levenshtein edit-distance scorer, homoglyph signal, and
  a combined scoring API that ties all three signals into a single
  `score_package()` call (`src/typosquat/`). CVE matching has its first
  module too: an OSV API client (`src/cve/osv_client.rs`) that queries
  [osv.dev](https://osv.dev) for a pinned package version. What remains is
  the crawler wiring that feeds real package names into these scorers.
- **Bounty Router** is the Soroban smart contract in this repo
  (`contracts/deprader_router/`). It's the trust boundary: it locks funds
  against a finding, records who claimed it, and pays out — independent of
  which crawler or scanner module produced the finding.
- **Drips** handles the actual payout distribution once a bounty is
  released.
- **Dashboard** (`frontend/`) is a read-only React app over the contract's
  public state (`bounty_count` / `get_bounty` / `get_claim`). It lists every
  bounty and its claims, holds no authority, and needs no wallet. Run it
  with `cd frontend && npm ci && npm run dev`.

## Scanning a lockfile (CLI)

The `depradar` binary scans a lockfile for typosquats and known
vulnerabilities:

```sh
cargo run --bin depradar -- path/to/package-lock.json
```

Useful options (run `depradar --help` for the full list):

| Option | Effect |
|---|---|
| `--format text\|json` | Output format; `json` writes a single machine-readable document |
| `--fail-on low\|medium\|high\|critical\|none` | Severity at which the run exits 1 (default `high`; overrides `depradar.toml`) |
| `--exclude <GLOB>` | Skip lockfiles matching the pattern (may be repeated; overrides `depradar.toml`) |
| `--ecosystem <ECO>` | Restrict to one ecosystem (may be repeated; overrides `depradar.toml`) |
| `--offline` | Skip the OSV lookup (typosquat checks only) |

Findings accepted in a `.depradarignore` next to the lockfile are reported
as suppressed and never fail the run.

### Install a prebuilt CLI

Download the archive for Linux x86_64, macOS Apple Silicon (aarch64), or
Windows x86_64 from the [latest GitHub release](https://github.com/DepRadarOrg/DepRadar/releases/latest).
Each archive has a matching `.sha256` file. Verify it before extracting:

```sh
# Linux
sha256sum -c depradar-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf depradar-x86_64-unknown-linux-gnu.tar.gz

# macOS
shasum -a 256 -c depradar-aarch64-apple-darwin.tar.gz.sha256
tar -xzf depradar-aarch64-apple-darwin.tar.gz
```

On Windows, compare the archive hash in PowerShell:

```powershell
$actual = (Get-FileHash -Algorithm SHA256 .\depradar-x86_64-pc-windows-msvc.zip).Hash.ToLower()
$expected = ((Get-Content .\depradar-x86_64-pc-windows-msvc.zip.sha256).Trim() -split '\s+')[0]
if ($actual -ne $expected) { throw 'SHA-256 mismatch' }
Write-Output 'SHA-256 verified'
```

The SHA-256 values must match. Extract the archive and place `depradar` (or
`depradar.exe`) on your `PATH`. The release does not require a Rust toolchain.
Builds report their version and source commit with `depradar --version`.

Every release also carries CycloneDX software bills of materials:
`depradar-rust.cdx.json` (the CLI crate), `deprader-router-rust.cdx.json`
(the bounty router contract) and `depradar-frontend.cdx.json` (the
dashboard's production npm dependencies).

### Project config file (`depradar.toml`)

Teams that run the same flags on every CI run can put them in a
`depradar.toml` at the root of their repository. When the file is present,
its values are used as defaults. **CLI flags always override the file**, and
the file always overrides the built-in defaults.

```toml
# depradar.toml

# Severity level at which the run exits with code 1.
# Accepted values: "low", "medium", "high", "critical", "none"
# Default: "high"
fail_on = "high"

# Output format for the scan command.
# Accepted values: "text", "json"
# Default: "text"
format = "text"

# Glob patterns for lockfile paths to skip.
# Patterns are matched against the full path of each lockfile found.
# `*` matches any sequence of non-separator characters; `**` matches
# any sequence including path separators.
# Default: [] (nothing excluded)
exclude = ["vendor/**", "third_party/**", "node_modules/**"]

# Restrict scanning to these ecosystems only.
# Accepted values: "npm", "pypi", "cargo", "go", "maven", "rubygems", "packagist"
# Default: [] (all ecosystems scanned)
ecosystems = ["npm", "pypi"]
```

All keys are optional — an empty `depradar.toml` is valid and just uses
built-in defaults. Unknown keys are rejected with a parse error.

| Key | Type | Accepted values | Default |
|---|---|---|---|
| `fail_on` | string | `low`, `medium`, `high`, `critical`, `none` | `high` |
| `format` | string | `text`, `json` | `text` |
| `exclude` | array of strings | glob patterns | `[]` |
| `ecosystems` | array of strings | `npm`, `pypi`, `cargo`, `go`, `maven`, `rubygems`, `packagist` | `[]` (all) |

### Exit codes

| Code | Meaning |
|---|---|
| `0` | No finding at or above the `--fail-on` level |
| `1` | At least one finding at or above the `--fail-on` level |
| `2` | Usage error, or the scan couldn't complete (unreadable or malformed lockfile or `.depradarignore`, failed OSV lookup) |

To use DepRadar as a CI gate and show its findings in GitHub's Security
tab:

```yaml
- name: DepRadar
  run: cargo run --release --bin depradar -- --format sarif --output depradar.sarif --fail-on high package-lock.json
- name: Upload SARIF
  if: always()
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: depradar.sarif
```

The upload job needs the `security-events: write` permission.

### Shell completions

Generate completion scripts for your shell with `depradar completions <bash|zsh|fish|powershell>`:

- **Bash**
  ```sh
  mkdir -p ~/.local/share/bash-completion/completions
  depradar completions bash > ~/.local/share/bash-completion/completions/depradar
  ```
- **Zsh**
  ```sh
  mkdir -p ~/.zfunc
  depradar completions zsh > ~/.zfunc/_depradar
  # Ensure ~/.zfunc is on your fpath before compinit in ~/.zshrc:
  # fpath=(~/.zfunc $fpath)
  # autoload -Uz compinit && compinit
  ```
- **Fish**
  ```sh
  mkdir -p ~/.config/fish/completions
  depradar completions fish > ~/.config/fish/completions/depradar.fish
  ```
- **PowerShell**
  ```powershell
  depradar completions powershell >> $PROFILE
  ```

## Live on Testnet

The router contract is deployed and has completed a full bounty lifecycle
on Stellar testnet:

**Contract ID:** [`CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`](https://stellar.expert/explorer/testnet/contract/CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ)

To deploy the contract yourself — build the wasm, deploy it, run
`initialize --admin`, and print the new contract ID — run
`scripts/deploy.sh`. The full procedure (identities, verifying the deploy
with the read client, and updating the contract ID in this README,
`client/` and `frontend/`) is documented in [DEPLOYMENT.md](DEPLOYMENT.md).

A one-time `initialize(admin)` call at deploy time sets the admin who
reviews claims. Bounty IDs are assigned by the contract, and every state
change emits an event (`created`, `claimed`, `rejected`, `released`,
`cancelled`, `admin_set`) so indexers and dashboards can follow along
without polling.

1. **`create_bounty(creator, issue_ref, amount, token, deadline) -> bounty_id`**
   — locks `amount` of `token` from `creator` against the finding described
   by `issue_ref`, open for claims until `deadline`. Funds move into the
   contract here.
2. **`submit_claim(bounty_id, claimant, proof_ref) -> claim_id`** — records a
   claim with a reference to the proof (a PR, advisory or writeup). Moves no
   funds. Any number of researchers can claim the same bounty (one claim
   per address, capped at 50), so a junk claim submitted first can't block
   the real finder.
3. **`reject_claim(bounty_id, claim_id)`** — admin-only. Marks a bad claim
   rejected; the bounty stays open for the others.
4. **`release(bounty_id, claim_id)`** — admin-only, once per bounty. Pays the
   locked funds to the chosen claimant and closes the bounty. Still works
   after the deadline, so in-time claims can be paid during review.
5. **`cancel_bounty(caller, bounty_id)`** — refunds the creator. The admin
   can cancel at any time; the creator can cancel once the deadline has
   passed with no pending claims, or once a 7-day review window after the
   deadline has also passed. Funds can never be locked forever, and a
   creator can't pull them out from under a claimant awaiting review.

The admin can be rotated with a two-step `propose_admin` / `accept_admin`
handshake. `get_bounty`, `get_claim`, `get_claim_id`, `bounty_count` and
`get_admin` expose the full state at every step, so anyone can verify a
bounty without trusting a dashboard or API — the
[testnet read client](client/) does exactly that.

The lifecycle above was run end-to-end against the live contract,
including the case the original single-claim design couldn't handle: a
junk claim lands first, the admin rejects it, and the real finder is paid
([release tx](https://stellar.expert/explorer/testnet/tx/69538e183dab1799c92be7231e562fcbca47bd32e0247a44f0cddfbeaee84cb0)).

## Why This Matters

Software supply chain compromise isn't a hypothetical: the `event-stream`
backdoor, the `ua-parser-js` account takeover, and the `xz-utils`
long-con backdoor all reached production systems by exploiting the same
weak point — a compromised or abandoned maintainer identity slipping a
malicious change past a dependency graph nobody was actively watching.
Typosquatting attacks on npm and PyPI follow the same pattern at higher
volume and lower sophistication, betting that an install-time typo goes
unnoticed.

The common failure isn't a lack of detection tooling — it's that
detection has no funded, verifiable path to action. A researcher who spots
a malicious package today has no standard way to be compensated for the
find, and no public record proving they found it first. DepRadar is built
to close that gap: pay the bounty on-chain, the moment the claim is
verified, with the full trail auditable by anyone.

## Status & Roadmap

### Done

| What | PR |
|---|---|
| Bounty Router contract (`create_bounty` / `submit_claim` / `release`), unit-tested and deployed to Stellar testnet | _(predates issue tracker)_ |
| Full `create_bounty` → `submit_claim` → `release` lifecycle run and verified against the live testnet contract | _(predates issue tracker)_ |
| Repository health — MIT license, SECURITY.md, Code of Conduct, PR/issue templates, CODEOWNERS, Dependabot, CI split into fmt/clippy/test/wasm/client jobs, weekly `cargo audit` + `npm audit` | [`41903c3`](https://github.com/DepRadarOrg/DepRadar/commit/41903c3) |
| Read-only web dashboard — bounty list and bounty detail with claims, live from testnet (`frontend/`) | _(see PROGRESS.md)_ |
| Hardened Bounty Router v2 — multiple claims per bounty, `reject_claim`, deadlines + `cancel_bounty` refunds, contract-assigned IDs, events, storage TTL management, two-step admin rotation; redeployed and verified on testnet | [`79326f2`](https://github.com/DepRadarOrg/DepRadar/commit/79326f2) |
| CLI release archives for Linux, macOS and Windows with SHA-256 checksums; `depradar --version` includes the source commit | [#87](https://github.com/DepRadarOrg/DepRadar/issues/87), [#88](https://github.com/DepRadarOrg/DepRadar/issues/88) |
| Shared crawler checkpoint/event contract and npm registry changes-feed crawler | [#89](https://github.com/DepRadarOrg/DepRadar/issues/89), [#90](https://github.com/DepRadarOrg/DepRadar/issues/90) |
| Popular-package reference dataset — npm and PyPI top-1000 curated lists (`data/popular-packages/`) and dataset loader (`src/typosquat/dataset.rs`) | [#1](https://github.com/DepRadarOrg/DepRadar/pull/1) |
| Levenshtein edit-distance scorer and `nearest_match` for typosquat detection (`src/typosquat/distance.rs`) | [#2](https://github.com/DepRadarOrg/DepRadar/pull/2) |
| Homoglyph (lookalike-character) signal — Unicode confusable normalization and `homoglyph_impersonation` (`src/typosquat/homoglyph.rs`) | [#7](https://github.com/DepRadarOrg/DepRadar/pull/7) |
| Testnet read client — CLI wrapper around `get_bounty` / `get_claim` against the live contract (`client/`) | [#9](https://github.com/DepRadarOrg/DepRadar/pull/9) |
| OSV API client — `query_osv(package, ecosystem, version)` against [osv.dev](https://osv.dev) (`src/cve/osv_client.rs`) | [#10](https://github.com/DepRadarOrg/DepRadar/pull/10) |
| Combined typosquat scoring API — `score_package()` merges Levenshtein + homoglyph signals; `cargo run --bin scan` CLI (`src/typosquat/scanner.rs`) | [#11](https://github.com/DepRadarOrg/DepRadar/pull/11) |
| Project config file — `depradar.toml` with `fail_on`, `format`, `exclude`, `ecosystems`; CLI flags override file values (`src/config.rs`) | [#85](https://github.com/DepRadarOrg/DepRadar/issues/85) |
| Deployment script (`scripts/deploy.sh`) and [DEPLOYMENT.md](DEPLOYMENT.md) — build, deploy, `initialize --admin`, print contract ID | [#119](https://github.com/DepRadarOrg/DepRadar/issues/119) |

### Open

- Additional registry crawlers (PyPI, crates.io, …) and scanner wiring
- Full scanner pipeline wiring (crawler → scanner → `create_bounty`)
- Behavioral analysis (install scripts, obfuscation, anomalous permission requests)
- Dashboard features: filters, search, live event feed, wallet-connected actions
- Drips payout integration
- **RepoGuard** (`depradar repo-scan`): detect malware hidden inside repositories, their git history and open PRs. See the day-by-day plan in [ROADMAP.md](ROADMAP.md)

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to pick up a module, and
[open issues](https://github.com/DepRadarOrg/DepRadar/issues) for what's
currently up for grabs. [PROGRESS.md](PROGRESS.md) tracks what's actually
landed, module by module.

## Contributors

Thanks to everyone who has shipped code into this repo:

[![Contributors](https://contrib.rocks/image?repo=DepRadarOrg/DepRadar)](https://github.com/DepRadarOrg/DepRadar/graphs/contributors)

| Contributor | What they shipped |
|---|---|
| [ALLEN-AYODEJI](https://github.com/ALLEN-AYODEJI) | Bounty Router contract, typosquat dataset, edit-distance scorer, homoglyph signal, testnet read client |
| [Emeraldson001](https://github.com/Emeraldson001) | Combined typosquat scoring API and `scan` CLI binary ([#11](https://github.com/DepRadarOrg/DepRadar/pull/11)) |
| [MamaLove360](https://github.com/MamaLove360) | OSV API client for CVE matching ([#10](https://github.com/DepRadarOrg/DepRadar/pull/10)) |

Want to add your name to this list? Pick an open issue and read
[CONTRIBUTING.md](CONTRIBUTING.md) first.

## Security

Found a vulnerability in DepRadar itself? Please report it privately — see
[SECURITY.md](SECURITY.md). Participation in this project is governed by the
[Code of Conduct](CODE_OF_CONDUCT.md).

## License

DepRadar is released under the [MIT License](LICENSE).
