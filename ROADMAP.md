# DepRadar Roadmap: RepoGuard

> **One change a day for 30 days.** Each day is one small, reviewable pull
> request with tests. Every item is sized to be picked up as a single issue.

## Why

DepRadar already checks the packages a project *depends on*: lockfile
parsing, typosquat scoring, OSV vulnerability lookups, and behavioral
analysis of package tarballs. In 2026 the PolinRider malware campaign showed
that's only half the problem. It hid its payload *inside the repositories
themselves*, spread through thousands of open-source projects (including
ones in the Drips ecosystem), and evaded review by:

- appending obfuscated code to build configs (`postcss.config.js`,
  `tailwind.config.*`, …) after hundreds of columns of whitespace;
- forging "Merge pull request #N" commits so the change looked like a
  reviewed merge;
- planting auto-run hooks in `.vscode/tasks.json`, install scripts in
  `package.json`, and payloads disguised as font files.

None of that shows up in a lockfile. **RepoGuard** is a new `depradar
repo-scan` mode that inspects the repository itself: its files, its git
history, its branches and its open pull requests. Findings plug into
DepRadar's existing Bounty Router, so whoever catches malware first gets
paid on-chain.

## Principles

- **Reuse before writing.** `src/behavioral/` already scores obfuscated
  JavaScript (`obfuscation.rs`), install scripts (`scripts.rs`,
  `network.rs`) and native binaries (`binaries.rs`). RepoGuard points those
  detectors at repository files instead of package tarballs.
- **Low false positives.** A scanner people turn off is worthless. Every
  rule ships with positive *and* negative test fixtures from real projects.
- **No live malware in the repo.** Test fixtures are defanged: real
  structure, harmless payload.
- **Same outputs as today.** Text, JSON and SARIF via `src/report/`, and
  `depradar.toml` for config, so RepoGuard drops into existing CI.

## Phase 1: Scan the working tree (days 1–7)

| Day | Change | Done when |
|---|---|---|
| 1 | `depradar repo-scan [PATH]` subcommand: walks the tree (respecting `.gitignore` and `exclude` in `depradar.toml`) and emits findings through the existing report formats | Running it on DepRadar itself produces an empty, valid SARIF file |
| 2 | Indicator-of-compromise (IOC) rule pack in `data/ioc/` (known markers, file names and package names per campaign), starting with PolinRider | A fixture containing each listed marker is flagged with the campaign name |
| 3 | Whitespace-padding detector: very long lines, code after a long run of spaces, or code after `export default` / `module.exports` in `*.config.*` files | Catches a defanged copy of the Trellis `postcss.config.js` payload; no hits on the 20 most common real config files |
| 4 | Run `behavioral::obfuscation` over the repo's own JS/TS/MJS/CJS files, with thresholds tuned for source rather than minified tarballs | Flags packed or encoded loaders; ignores committed `dist/` bundles listed in config |
| 5 | Auto-run trap detector: `.vscode/tasks.json` with `runOn: folderOpen`, `task.allowAutomaticTasks`, `.devcontainer` lifecycle commands, and agent configs that pre-approve shell access (for example `.claude/settings.json` allowing `Bash`) | Each trap type has a fixture and a clear remediation message |
| 6 | Repo `package.json` lifecycle scripts (`preinstall`, `postinstall`, `prepare`, …) via `behavioral::scripts` and `behavioral::network` | `"preinstall": "node dist/setup.js"` is flagged; ordinary `prepare: husky` is low severity |
| 7 | Disguised files: fonts whose magic bytes aren't a font, `.llf` files, `dist/setup.js`, `temp_auto_push.bat`, a `config.bat` in `.gitignore`, and native binaries via `behavioral::binaries` | A JS file renamed to `fa-solid-500.woff2` is caught |

## Phase 2: Git forensics (days 8–14)

| Day | Change | Done when |
|---|---|---|
| 8 | `--history`: run every Phase 1 rule over every commit's diff, not just the current tree | Finds a payload that was added and later deleted |
| 9 | Forged-merge detector: single-parent commits titled "Merge pull request #N"; optionally confirm against the PR's real `merge_commit_sha` via the GitHub API | Reproduces the Trellis finding (`745961c6` vs the genuine `b31706d8`) on a synthetic repo |
| 10 | Evil-merge detector: content that appears only in a merge commit, matching neither parent | A merge commit that slips in an extra file is flagged |
| 11 | Signing report: unsigned commits on the default branch, and commits whose committer claims to be "GitHub" but carry no GitHub signature | Summary table in text output; per-commit entries in JSON |
| 12 | `--all-refs` and `--prs`: scan every branch and every open PR head (via the GitHub API) | One command reproduces the full Trellis sweep |
| 13 | Known-malicious packages: check lockfiles against a curated list plus OSV `MAL-` advisories | `tailwindcss-style-animate` and friends are flagged with their source |
| 14 | Baseline and suppressions: `depradar repo-scan --baseline` records accepted findings so CI only fails on new ones | Re-running with a baseline exits 0; a newly added payload exits non-zero |

## Phase 3: Put it where maintainers are (days 15–21)

| Day | Change | Done when |
|---|---|---|
| 15 | GitHub Action (`DepRadarOrg/repo-scan-action`) that runs `repo-scan` and uploads SARIF to the Security tab | Example workflow in `examples/` passes on this repo |
| 16 | PR mode: scan only what a PR changes, and leave one summary comment when it touches sensitive paths (configs, `.vscode/`, `package.json`, fonts or binaries) | A PR adding a padded config gets a comment pointing at the line |
| 17 | `--harden` suggestions: generates a CODEOWNERS block and a branch-ruleset checklist (signed commits, code-owner review, no force pushes) for the scanned repo | Output can be pasted straight into `.github/CODEOWNERS` |
| 18 | `depradar hook install`: a pre-commit hook that runs the fast Phase 1 rules on staged files | Committing a padded config is blocked locally, with a bypass flag |
| 19 | `depradar org-scan <org>`: sweep every repository in a GitHub org | Produces one combined report across all repos |
| 20 | Repo-scan report viewer in the dashboard (extends #142) | Upload a JSON report and browse findings by file and rule |
| 21 | Docs: an incident-response playbook (a step-by-step cleanup like the one Trellis went through, anonymised) and a "write your first rule" tutorial | Linked from the README |

## Phase 4: Beyond a scanner, an immune system for the ecosystem (days 22–30)

| Day | Change | Done when |
|---|---|---|
| 22 | Nightly ecosystem sweep: scan a published list of Drips Wave repositories, read-only and rate-limited | Results land in a private report for coordinated disclosure, never as public shaming |
| 23 | "Scanned by DepRadar" README badge served from the latest scan result | Badge shows clean, findings, or stale |
| 24 | Finding to bounty: a confirmed repo-scan finding can open a bounty on the Bounty Router (`create_bounty`) with the report hash as `proof_ref` | End-to-end on testnet |
| 25 | Responsible-disclosure flow: findings go privately to maintainers first, with an embargo before anything is public | Documented and enforced by the CLI's publish step |
| 26 | Community IOC feed: rules live in a separate signed repo that `repo-scan` updates from | `depradar repo-scan --update-rules` verifies the signature before use |
| 27 | Rule contribution kit: rule format docs, fixture template and a test harness, so each new campaign becomes a Wave issue | A new rule needs only a data file plus fixtures, no Rust |
| 28 | Performance: parallel walking and caching so a 50k-file monorepo scans in under 10 seconds | Benchmark added under `benches/` |
| 29 | Release: tag a version with RepoGuard, generate a changelog (#152), and publish binaries via the existing release pipeline | Release notes link this roadmap |
| 30 | Retrospective and next roadmap: false-positive review, contributor feedback, and Phase 5 planning | `ROADMAP.md` updated |

## How to contribute

Each day above becomes a GitHub issue labelled `roadmap` and `repo-scan`,
with the "Done when" column as its acceptance criteria. Pick one up the
usual way (see [CONTRIBUTING.md](CONTRIBUTING.md)) and add a
[PROGRESS.md](PROGRESS.md) entry when it lands. Issues build on each other
within a phase, so check the previous day has merged first.

Found a real infected repository while testing? **Don't open a public
issue.** Follow [SECURITY.md](SECURITY.md) so the maintainers can clean up
before attackers notice.
