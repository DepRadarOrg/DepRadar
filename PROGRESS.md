# Progress Log

This file is the running record of concrete work landed in this repo: what
was built, who built it, and which issue (if any) it closes. It exists so
that "what's actually shipped" is answerable by reading one file, rather
than reconstructing it from git history or a scattered issue tracker.

Every issue's Definition of Done requires an entry here before the issue
is closed — see [CONTRIBUTING.md](CONTRIBUTING.md).

## Entries

### Release binaries for the CLI — #88

- **What:** Adds a tag-triggered GitHub Actions workflow that builds the
  `depradar` CLI for Linux x86_64, macOS Apple Silicon, and Windows x86_64.
  Each release archive includes a SHA-256 checksum. The README documents
  downloading, checksum verification, and extraction without a Rust toolchain.
- **By:** dev-fani
- **Closes:** #88
- **Status:** Implemented; release workflow validation is recorded in the PR.

### CLI build identifier — #87

- **What:** `depradar --version` now prints the package semver and the full
  Git commit SHA embedded at build time. Builds without an available Git
  checkout report `unknown`. An integration test checks the output format.
- **By:** dev-fani
- **Closes:** #87
- **Status:** Implemented; verification is recorded in the PR.

### Shared crawler contract and checkpoints — #89

- **What:** Defines the shared `Crawler` trait, numeric sequence
  `Checkpoint`, and typed `PackageEvent` model. Its cursor documentation
  specifies retry and persistence behavior. Adds an in-memory `FakeCrawler`
  with a checkpoint advancement test.
- **By:** dev-fani
- **Closes:** #89
- **Status:** Implemented; verification is recorded in the PR.

### npm changes-feed crawler — #90

- **What:** Adds `NpmCrawler` for npm's CouchDB `_changes` feed. It pages
  through feed changes, emits new-package and new-version events, resumes
  from the sequence checkpoint, and uses the shared rate-limited HTTP client.
  The implementation follows npm's current `/registry/_changes` endpoint;
  because it omits package documents, changed package metadata is fetched
  separately from the public registry.
  Mocked HTTP tests cover paging, resumption, version classification and
  packages that return 404. A live poll from sequence `138448270` advanced to
  `138448271` and emitted no event for the sampled metadata update; the PR
  records that run.
- **By:** dev-fani
- **Closes:** #90
- **Status:** Implemented; verification is recorded in the PR.

### deprader_router contract — create_bounty / submit_claim / release

- **What:** Initial Soroban smart contract at `contracts/deprader_router/`.
  Implements `initialize(admin)`, `create_bounty(creator, bounty_id,
  issue_ref, amount, token)` (locks funds against a bounty ID),
  `submit_claim(bounty_id, claimant, proof_ref)` (records a claim, no fund
  movement), and `release(admin, bounty_id)` (admin-gated, one-time payout
  of locked funds to the claimant). Getters: `get_bounty`, `get_claim`.
- **By:** ALLEN-AYODEJI
- **Closes:** N/A (predates the issue tracker)
- **Status:** Deployed and verified on Stellar testnet. *Superseded by the
  hardened v2 router below.*
  - Contract ID: `CBFLUY6NB3JOQDEUOF2JG5TL3UD7GCQ7APUJ6EHNNSUJLUELHSGIG675`
  - A full `create_bounty` → `submit_claim` → `release` cycle was run
    against the live testnet contract using the native XLM Stellar Asset
    Contract as the test token, and confirmed via `get_bounty` / `get_claim`
    reads and on-chain transfer events.

### typosquat scanner — popular-package reference data + loader

- **What:** Groundwork for the typosquat detection scanner. Adds
  `data/popular-packages/npm-top1000.json` and
  `data/popular-packages/pypi-top1000.json` — each a JSON array of ~1000
  widely-used package names for that registry (a hand-curated representative
  snapshot, not a live-fetched ranking). Introduces the crate's off-chain
  Rust side: a root `deprader` library crate with
  `src/typosquat/dataset.rs`, exposing `load_popular_packages()`, which
  embeds both lists at compile time and returns them as one combined
  `Vec<String>` (npm first, then PyPI). No scoring logic yet — just the data
  and the loader. Covered by a unit test asserting the combined list is
  non-empty and contains known packages (`react`, `requests`).
- **By:** ALLEN-AYODEJI
- **Closes:** N/A (prep for the typosquat scanner module)
- **Status:** Merged. `cargo test --workspace --locked` passes.

### typosquat scanner — edit-distance primitives

- **What:** Adds `src/typosquat/distance.rs`. `levenshtein_distance(a, b)`
  computes standard Levenshtein edit distance (insert / delete / substitute,
  no transposition op) over Unicode scalar values with the two-row DP table,
  no external crate. `nearest_match(candidate, dataset)` returns the closest
  name in a reference slice and its distance (earliest entry wins ties;
  empty slice yields `(String::new(), usize::MAX)`). Wired into
  `src/typosquat/mod.rs`. Still no scoring/thresholding — callers decide
  what distance counts as suspicious.
- **Tests:** identical strings → 0; empty-operand cases; one-character-off
  typosquats (`expres`/`expresss` vs `express`, a single substitution) → 1;
  transposition → 2; `nearest_match` against the loaded popular-package
  dataset resolves deliberately misspelled `expresss` → `express` and
  `requezts` → `requests`, both at distance 1; exact hit → distance 0;
  empty-dataset sentinel.
- **By:** ALLEN-AYODEJI
- **Closes:** N/A (prep for the typosquat scanner module)
- **Status:** Merged. `cargo test --workspace --locked` passes.

### typosquat scanner — homoglyph (lookalike-character) signal

- **What:** Adds `src/typosquat/homoglyph.rs`, a second detection signal that
  sits alongside the edit-distance one and shares no code with it.
  `normalize_homoglyphs(candidate)` rewrites a name by mapping known Unicode
  lookalike characters back to their Latin equivalents — a hand-picked,
  deliberately conservative subset of the Unicode `confusables.txt` (UTR #39):
  Cyrillic, Greek and Armenian letters plus a couple of Latin-extended/IPA
  forms, and the entire full-width ASCII block (`U+FF01..=U+FF5E`) handled as a
  contiguous range. `homoglyph_impersonation(candidate, dataset)` returns
  `Some(name)` only when a substitution actually happened *and* the normalised
  form exactly matches an entry in `dataset` (in practice the combined list
  from `load_popular_packages()`); a correctly-spelled popular package
  normalises to itself and is never flagged. Wired into `src/typosquat/mod.rs`.
  Still no scoring/thresholding — this is a boolean signal for the
  (not-yet-written) scanner to weigh.
- **Tests:** `normalize_homoglyphs` is identity for plain ASCII (incl. a
  scoped `@angular/core` name and the empty string); Cyrillic `а` (U+0430) and
  `о` (U+043E) in `react`/`lodash` → Latin; full-width `ｅｘｐｒｅｓｓ` → `express`;
  Greek omicron (U+03BF) in `commander` → `o`. `homoglyph_impersonation`
  against the loaded popular-package dataset flags a Cyrillic-`а` `react`
  (npm), a full-width `requests` (PyPI) and a Greek-omicron `lodash` (npm);
  returns `None` for a correctly-spelled `react` and for a homoglyph name
  (`reаctxyzzy`) whose normalised form is not a popular package.
- **By:** ALLEN-AYODEJI
- **Closes:** N/A (prep for the typosquat scanner module)
- **Status:** Merged. `cargo test --workspace --locked` passes.

### Testnet read client — get_bounty / get_claim by ID

- **What:** Adds `client/`, a minimal read-only Node/TS client for
  `deprader_router` on Stellar testnet (contract ID
  `CBFLUY6NB3JOQDEUOF2JG5TL3UD7GCQ7APUJ6EHNNSUJLUELHSGIG675`). Uses
  `@stellar/stellar-sdk`'s `rpc.Server.queryContract` to simulate
  `get_bounty(bounty_id)` and `get_claim(bounty_id)` calls against
  `https://soroban-testnet.stellar.org` and print the decoded result — no
  transactions are submitted and no funded account is required, since both
  contract methods are pure reads. CLI: `npm start -- --bounty-id <id>
  --claim-id <id>` (both default to `1`). An ID with no matching bounty or
  claim prints a plain "not found" line rather than erroring, matching the
  contract's `Option<Bounty>` / `Option<Claim>` return type. Kept as a
  standalone `client/` package (its own `package.json`/`tsconfig.json`,
  outside the Cargo workspace) since it's plain TypeScript, not a Soroban
  contract.
- **Verified:** Run against the live contract with the default IDs, which
  resolve to the real `create_bounty` → `submit_claim` → `release` cycle
  recorded above — `get_bounty(1)` returned the stored bounty (amount
  `50000000`, `issue_ref: "org/repo#123"`, `released: true`) and
  `get_claim(1)` returned the stored claim (`proof_ref:
  "org/repo#123-pr-456"`). Also verified an unknown ID (`999`) prints the
  not-found path for both calls instead of throwing.
- **By:** ALLEN-AYODEJI
- **Closes:** #6
- **Status:** Merged. No UI yet, per the issue scope — read-only CLI output
  only.

### CVE matching — OSV API client

- **What:** Adds `src/cve/osv_client.rs`, the first piece of the CVE
  matching scanner class. `query_osv(package_name, ecosystem, version)`
  queries the [OSV.dev](https://osv.dev) API (`POST /v1/query`) for a package
  pinned to an exact version and parses the response into typed structs
  (`OsvQueryResponse` → `Vec<OsvVulnerability>`, with nested `OsvAffected` /
  `OsvRange` / `OsvEvent` / `OsvSeverity` / `OsvReference`) mirroring the
  relevant parts of the [OSV schema](https://ossf.github.io/osv-schema/). The
  `ecosystem` argument is a typed `Ecosystem` enum (`Npm`, `PyPI`,
  `CratesIo`) rather than a free string, mapped to the exact ecosystem names
  OSV expects. No filtering, deduplication, or scoring — an empty `vulns`
  list just means OSV has no known match for that exact version; turning raw
  matches into scored findings is later work. Uses `reqwest` (blocking
  client) for HTTP and `serde`/`serde_json` for (de)serialization — new
  dependencies for the crate, added to `Cargo.toml`.
- **Tests:** `query_osv` against the real OSV API for `lodash@4.17.4` (npm),
  a known-vulnerable pair predating the fix for GHSA-29mw-wpgm-hmr9 /
  CVE-2020-28500 (ReDoS in `trim`/`toNumber`/`trimEnd`, fixed in 4.17.21) —
  asserts the response is non-empty and includes that CVE alias. The test
  skips (rather than fails) if the request itself can't complete, since that
  reflects the runner's network access rather than a bug in the client. Also
  a plain unit test that `Ecosystem::as_osv_str()` matches OSV's documented
  ecosystem names for `npm`/`PyPI`/`crates.io`.
- **By:** MamaLove360 ([#10](https://github.com/DepRadarOrg/DepRadar/pull/10))
- **Closes:** #5
- **Status:** Merged. `cargo test --workspace --locked` passes, including a
  live network call to `api.osv.dev` confirming the known-vulnerable test
  case.

### typosquat scanner — combined scoring API + CLI

- **What:** Adds `src/typosquat/scanner.rs`, the single entry point that ties
  the two independent typosquat signals together:
  `score_package(name: &str) -> ScanResult` loads
  [`load_popular_packages()`](src/typosquat/dataset.rs) internally, runs
  `nearest_match` ([`distance.rs`](src/typosquat/distance.rs)) and
  `homoglyph_impersonation` ([`homoglyph.rs`](src/typosquat/homoglyph.rs))
  against it, and returns a `ScanResult` recording which signal(s) fired and
  why. A `distance_match` only counts when the nearest name is within
  `DISTANCE_THRESHOLD` (2) edits and isn't an exact match (distance 0 is the
  real package, not a typosquat); `homoglyph_match` is whatever
  `homoglyph_impersonation` reports. `ScanResult::is_likely_typosquat()` is
  true if either fired, and either, both, or neither can fire independently
  since the two signals still share no logic. `ScanResult` implements
  `Display` for human-readable output (used by the CLI below). Also adds
  `src/bin/scan.rs`, a small binary (`cargo run --bin scan -- <package-name>`)
  that scores its one argument and prints the result — the manual-testing
  surface the issue asked for. Wired into `src/typosquat/mod.rs`.
- **Tests:** exact popular-package name (`react`) and a long unrelated
  string are both unflagged; a pure edit-distance typo (`expresss` →
  `express`, distance 1) flags only `distance_match`; a full-width `requests`
  flags only `homoglyph_match` (its raw-codepoint edit distance to any
  dataset entry is 8, well past the threshold); a Cyrillic-`а` `react`
  flags *both* (it's simultaneously one substitution away from `react` and a
  homoglyph impersonation of it) — the concrete case the issue asked for.
  Two more tests check `Display` output for the both-signals and no-signal
  cases.
- **By:** Emeraldson001 ([#11](https://github.com/DepRadarOrg/DepRadar/pull/11))
- **Closes:** #4
- **Status:** Merged. Built on the dataset/distance work (#1, #2) and the
  homoglyph signal (#7).

### Repository health — license, security policy, CI hardening

- **What:** Licenses the project under MIT (`LICENSE`, plus `license` fields
  in both Cargo manifests and `client/package.json`). Adds `SECURITY.md`
  (private disclosure via GitHub advisories, scope covering contract and
  scanner), `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1), a PR template,
  bug/feature issue templates, `CODEOWNERS`, and Dependabot for Cargo, npm
  and GitHub Actions. CI now runs `cargo fmt --check`, `cargo clippy -D
  warnings`, the test suite, a `wasm32v1-none` release build of the
  contract, and the client typecheck as separate jobs; a new weekly
  `Dependency Audit` workflow runs `cargo audit` and `npm audit`. Fixes the
  one existing clippy lint (`unnecessary_map_or` in `distance.rs`) and
  applies `cargo fmt`. Bumps the client's `@stellar/stellar-sdk` lock to
  17.2.1 to clear high-severity advisories in its transitive `axios`
  dependency. README links now point at `DepRadarOrg/DepRadar`, and the
  contributor attributions in this file are corrected to match the merged
  PRs.
- **Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test
  --workspace --locked` (33 tests) and the wasm build all pass locally;
  `npm audit --omit=dev` reports 0 vulnerabilities; the client still reads
  `get_bounty(1)` / `get_claim(1)` correctly from the live testnet contract
  on the upgraded SDK.
- **By:** ALLEN-AYODEJI
- **Closes:** N/A (maintenance)
- **Status:** Merged.

### deprader_router v2 — claim review, refunds, events, TTL, admin rotation

- **What:** Reworks `contracts/deprader_router` to close the gaps in the v1
  design, where the first claim on a bounty was the only one that could
  ever be paid (a junk claim could block the real finder), unclaimed funds
  had no way out, and nothing was observable without polling.
  - **Multiple claims per bounty.** `submit_claim` returns a `claim_id`;
    any number of addresses can claim (one each, capped at
    `MAX_CLAIMS_PER_BOUNTY` = 50). Claims carry a `Pending` / `Rejected` /
    `Accepted` status and a `submitted_at` timestamp.
  - **Admin review.** New `reject_claim(bounty_id, claim_id)`;
    `release(bounty_id, claim_id)` now pays a *specific* claim. Both check
    the stored admin's auth directly instead of taking an `admin` argument.
  - **Deadlines and refunds.** `create_bounty` takes a `deadline` (max one
    year out); claims close at the deadline. New `cancel_bounty` refunds
    the creator — the admin any time, the creator once the deadline has
    passed with no pending claims or a 7-day review window after it has
    elapsed.
  - **Contract-assigned bounty IDs** (`create_bounty` returns the ID;
    `bounty_count` getter), removing caller-chosen ID collisions.
  - **Events** for every state change: `created`, `claimed`, `rejected`,
    `released`, `cancelled`, `admin_set`, `admin_prp`.
  - **Storage TTL management** — instance and persistent entries are
    extended on every write, so live bounties aren't archived mid-review.
  - **Two-step admin rotation** — `propose_admin` / `accept_admin`, plus
    `get_admin` / `get_pending_admin`.
  - Input validation for `issue_ref` / `proof_ref` (non-empty, ≤ 256 bytes);
    `create_bounty` requires an initialized contract; state is written
    before every token transfer.
- **Tests:** 21 contract tests (up from 7) covering initialization, ID
  assignment, input validation, auth (checked via `env.auths()` and by
  dropping all mocked auths), multi-claim, duplicate/late/over-cap claims,
  the junk-claim-first scenario, release during the review window, every
  cancel path and timing boundary, and admin rotation. Snapshots
  regenerated.
- **Deployed:** testnet contract
  [`CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`](https://stellar.expert/explorer/testnet/contract/CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ)
  with a fresh admin identity (replacing the v1 admin exposed on-chain).
  Verified live with native XLM:
  - Bounty 1: `create_bounty` (5 XLM) → a junk claim (#0) then a real
    claim (#1) → `reject_claim(1, 0)` → `release(1, 1)` paid the real
    claimant
    ([tx](https://stellar.expert/explorer/testnet/tx/69538e183dab1799c92be7231e562fcbca47bd32e0247a44f0cddfbeaee84cb0)).
  - Bounty 2: `create_bounty` (2 XLM) → claim → a non-admin `release`
    attempt failed authorization → admin `cancel_bounty` refunded the
    creator
    ([tx](https://stellar.expert/explorer/testnet/tx/39b406a4eeda3d879c0b45ff2a8eae388838eda9bc185891f6fc47eca1b31912)).
- **Client:** `client/` points at the v2 contract and now prints a bounty
  plus every claim on it (`get_claim(bounty_id, claim_id)`).
- **Breaking:** `create_bounty`, `submit_claim`, `get_claim` and `release`
  signatures changed; `Bounty.released: bool` became `status`.
- **By:** ALLEN-AYODEJI
- **Closes:** N/A (contract hardening)
- **Status:** Merged.

### Web dashboard — read-only bounty explorer

- **What:** Adds `frontend/`, a React 19 + TypeScript + Vite app that reads
  the v2 router on testnet with `@stellar/stellar-sdk` (simulated
  `queryContract` calls, no wallet). It has two routes: `/` lists every
  bounty, newest first, with reward, status, claim count, deadline and
  creator, plus total/open/paid-out stats. `/bounty/:id` shows the bounty's
  facts and every claim with its Pending/Rejected/Accepted status.
  - All contract reads live in `lib/contract.ts`, with `parseBounty` /
    `parseClaim` mapping contract structs to typed camelCase objects.
  - Amount, address and date helpers are in `lib/format.ts`.
  - `useAsync` handles loading/error/retry and ignores stale responses.
  - Styling uses CSS design tokens, dark by default with light following the
    OS. Hash routing means the build works on any static host.
  - The network and contract can be overridden via `.env.local`.
- **Tests:** 16 Vitest tests covering amount formatting (decimals,
  grouping, negatives), address shortening, relative time, contract struct
  parsing (both enum encodings, plus the rejection of unknown shapes), and
  component rendering for the status badge and the error state with retry.
- **CI:** a new `frontend` job runs `oxlint --deny-warnings`, typecheck,
  tests and the build. Dependabot and the weekly npm audit cover
  `frontend/`.
- **Verified:** run locally against the live contract. The list shows both
  testnet bounties, and bounty #1's detail page shows the rejected junk
  claim and the accepted real claim.
- **By:** ALLEN-AYODEJI
- **Closes:** N/A (scaffold for the dashboard issue track)
- **Status:** Merged.

### Web dashboard — visual redesign

- **What:** Restyles `frontend/` around the DepRadar brand.
  - **Home:** a hero with an animated radar in which every blip is a real
    bounty (colored by status, and each one links to its bounty), plus live
    counters for XLM paid out, XLM locked, bounties funded and claims
    submitted.
  - **Bounty board:** cards with an ecosystem tag, a large reward, a status
    accent and the deadline, with All/Open/Paid/Refunded filter chips stored
    in the URL.
  - **Bounty page:** a hero header with the reward, a visual timeline
    (funded → each claim with its verdict → claims close → paid or
    refunded), and a details panel with a copyable finding reference.
  - **Visual system:** dark navy/teal tokens taken from `assets/banner.svg`;
    Space Grotesk, Inter and JetBrains Mono, self-hosted; skeleton loaders,
    address identicons, a radar favicon, and pages that scroll to the top
    on navigation. All animation is disabled under
    `prefers-reduced-motion`.
- **Tests:** 21 frontend tests, up from 16. The new `lib/insights.ts`
  logic (board stats and timeline ordering) is unit-tested.
- **Verified:** checked in the browser at 1280px and 375px against the live
  testnet contract.
- **By:** ALLEN-AYODEJI
- **Closes:** N/A
- **Status:** Merged.

### typosquat scanner — crates.io popular-package dataset

- **What:** Adds `data/popular-packages/crates-top1000.json`, the top 1000
  crates by recent (90-day) downloads from the crates.io API, taken on
  2026-10-16, lowercased and ordered most-downloaded first. `load_popular_packages()` now includes
  it after the npm and PyPI lists, so Rust lookalikes (`serde_json`
  imitations, `tokio` lookalikes) are scored like everything else. A new
  `data/popular-packages/README.md` records each file's source and snapshot
  date. The ecosystem-aware API (#24) doesn't exist yet. The existing
  `Ecosystem::CratesIo` variant in `cve::osv_client` is there for it to map
  onto this file.
- **Tests:** the loader test now also asserts `serde` and `tokio` are
  present.
- **By:** Austinaminu2
- **Closes:** #39
- **Status:** In review.

### typosquat scanner — dataset validation test

- **What:** A test in `src/typosquat/dataset.rs` loads every bundled
  popular-package file and checks that each entry has no leading or trailing
  whitespace, is lowercase, is a valid name on its registry, and is not a
  duplicate.
  - npm names are checked against npm's naming rules, including `@scope/`
    prefixes.
  - PyPI names are checked against PEP 508, and duplicates are found after
    PEP 503 normalisation, so `ruamel.yaml` and `ruamel-yaml` count as the
    same project.
  - It also checks that each file is a flat JSON array of strings, and that
    the loader's hand-rolled parser reads it the same way `serde_json` does.

  All failures are reported together, one line per entry. A second test
  feeds the checks deliberately broken input (whitespace, uppercase, an
  invalid character, a leading `_`, a duplicate, a PEP 503 duplicate, a
  non-string element) and asserts each one is caught.
- **Data fix:** lowercased the 9 mixed-case PyPI entries the new test
  flagged (`PyWavelets`, `types-PyYAML`, `cx-Oracle`, …).
- **By:** Austinaminu2
- **Closes:** #41
- **Status:** In review.

### typosquat scanner — dataset regeneration script

- **What:** Adds `scripts/update-datasets.py`, a standard-library-only
  Python script that rebuilds `data/popular-packages/*-top1000.json` from
  public download stats.
  - Sources: [`npm-high-impact`](https://github.com/wooorm/npm-high-impact)
    for npm, [Top PyPI Packages](https://hugovk.dev/top-pypi-packages/) for
    PyPI, and the crates.io API sorted by recent downloads.
  - For each registry it keeps the top 1000 valid, distinct names (PEP 503
    normalised for PyPI) and writes them lowercased in download order (the
    scanner uses position as a popularity rank), in the existing two-space
    JSON format.
  - It only rewrites a file whose content changed, so a second run leaves no
    diff.
  - `data/popular-packages/README.md` documents the sources, how to run the
    script, and the caveats.
  - The dataset validation test now covers `crates-top1000.json` as well.
- **Data:** regenerated the npm and PyPI lists on 2026-10-16, replacing the
  hand-curated snapshots (808 npm and 526 PyPI names changed). The crates.io
  list, from #39, came out unchanged. Ranking npm by downloads favours
  transitive dependencies, so `vue` and `@angular/core` drop out of the top
  1000. `react`, `express`, `lodash`, `axios`, `requests`, `django`,
  `serde` and `tokio` all remain.
- **By:** Austinaminu2
- **Closes:** #40
- **Status:** In review. Builds on #39 and #41.

### typosquat scanner — JSON-serializable `ScanResult`

- **What:** `ScanResult` in `src/typosquat/scanner.rs` now derives
  `serde::Serialize` and `serde::Deserialize` with snake_case keys, so the
  CLI and future report formats can emit machine-readable output.
  `distance_match` serializes as a `[name, distance]` array and an unset
  signal as `null`.
- **Tests:** one test asserts the exact JSON for `expresss`
  (all signal fields plus `confidence`, e.g. `"distance_match":["express",1]`).
  A round-trip test serializes and deserializes three results (distance
  only, both signals, clean) through `serde_json` and checks each comes back
  unchanged.
- **By:** Austinaminu2
- **Closes:** #38
- **Status:** In review.

### typosquat — top-N nearest matches

- **What:** Adds `nearest_matches(candidate, dataset, n) -> Vec<(String, usize)>`
  to [`distance.rs`](src/typosquat/distance.rs), returning up to `n` names
  sorted by edit distance and then dataset order (a stable sort), so a
  candidate equally close to two popular packages reports both.
  `nearest_match` is now a thin wrapper (`n = 1`) and keeps its tie and
  empty-dataset behavior.
- **Tests:** not added in this change.
- **By:** Maki
- **Closes:** #36
- **Status:** In review.

### typosquat — BK-tree index for distance lookups

- **What:** Adds [`bktree.rs`](src/typosquat/bktree.rs), a BK-tree over the
  dataset keyed by `levenshtein_distance`, with
  `BkTree::within(candidate, max) -> Vec<(&str, usize)>`. Searches only
  descend into children whose edge distance is within `max` of the current
  node's distance (triangle inequality). Results are sorted by distance and
  then dataset order, so they match a brute-force scan. Duplicate names
  are stored once, at their first position. `score_package` now uses a
  tree built once and cached in a `OnceLock`, over a cached dataset
  (`dataset::popular_packages()`). The public API is unchanged.
- **Tests:** the property test and criterion benchmark from the issue are
  not included yet.
- **By:** Maki
- **Closes:** #35
- **Status:** In review.

### typosquat — digit-for-letter swap detection

- **What:** Adds a separate, opt-in normalization to
  [`homoglyph.rs`](src/typosquat/homoglyph.rs): `normalize_digit_swaps`
  folds `0→o`, `1→l`, `3→e`, `4→a`, `5→s`, and is not part of
  `normalize_homoglyphs`. `digit_swap_impersonation` applies homoglyph
  folding and then digit folding. It flags a name only if a digit was
  actually swapped, the result is a popular package, and the name itself
  isn't already in the dataset (so `boto3` and `h5py` aren't flagged, and
  `i18n` is left alone because `8` isn't folded). The scanner reports it as
  a new `ScanResult::digit_swap_match` with its own reason line:
  `digit-for-letter swap impersonating popular package "lodash"`.
- **Tests:** not added in this change.
- **By:** Maki
- **Closes:** #34
- **Status:** In review.

### typosquat — 0–100 confidence score

- **What:** Adds `confidence: u8` to `ScanResult` in
  [`scanner.rs`](src/typosquat/scanner.rs), computed by
  `scanner::confidence`. The score has three parts: lookalike (50 if the
  homoglyph or digit-swap signal fired), distance (`40 - 10 × distance`),
  and popularity (5–20, from the new `dataset::popularity_rank`, which uses
  each registry file's order). A clean name scores 0. The lookalike part is
  worth more than distance plus popularity combined, so the ranking is
  always both signals (75–100) > lookalike only (55–70) > distance only
  (25–50). The doc comment documents the formula and gives worked examples
  (`expresss` 40, full-width `requests` 65, `l0dash` 85, Cyrillic-`а`
  `react` 100). `Display` now prints `likely typosquat (confidence N/100)`.
- **Tests:** not added in this change.
- **By:** Maki
- **Closes:** #37
- **Status:** In review.

### scanner — yarn.lock (v1) parser

- **What:** Adds `parse_yarn_lock` in [`yarn.rs`](src/lockfile/yarn.rs) and
  wires it into `parse_file` for `yarn.lock`. Reads each entry header
  (`"name@^1.0.0", name@~1.1:`, including scoped names and several
  specifiers per entry) and its `version "x.y.z"` field; each
  `(name, version)` pair is returned once. `direct` and `dev` are `false`
  because the format doesn't record them.
- **Tests:** Unit tests over the new `tests/fixtures/yarn.lock` fixture
  (scoped, multi-specifier and exact entries). Not run in this change.
- **By:** soma-enyi
- **Closes:** #62
- **Status:** In review.

### scanner — pnpm-lock.yaml parser (v6 and v9)

- **What:** Adds `parse_pnpm_lock` in [`pnpm.rs`](src/lockfile/pnpm.rs),
  wired into `parse_file` for `pnpm-lock.yaml`, and the `serde_yaml`
  dependency (plus a `ParseError::Yaml` variant). Reads the `packages:`
  section for both `/name@version` (v6) and `name@version` (v9) keys,
  including scoped names; v6 peer-dependency suffixes such as
  `(react@18.2.0)` are dropped. `dev` follows the v6 `dev:` field and is
  `false` for v9, which doesn't record it; `direct` is always `false`.
- **Tests:** Unit tests over new `pnpm-lock-v6.yaml` and
  `pnpm-lock-v9.yaml` fixtures. Not run in this change; `Cargo.lock` is not
  yet updated for `serde_yaml`.
- **By:** soma-enyi
- **Closes:** #63
- **Status:** In review.

### scanner — requirements.txt parser

- **What:** Adds `parse_requirements` and
  `parse_requirements_with_warnings` in [`pypi.rs`](src/lockfile/pypi.rs),
  wired into `parse_file` for `requirements*.txt`. `name==version` (and
  `===`) pins are recorded; other specifiers (`>=`, `~=`, `==1.*`) and bare
  names get `version: None`. Comments, blank lines, `--hash` options
  (including `\` continuations), extras and environment markers are
  ignored. `-r other.txt` includes are not followed and come back as
  warnings from `parse_requirements_with_warnings`; other option lines
  (`-e`, `--index-url`, ...) are skipped silently. All entries are
  `direct: true`.
- **Tests:** Unit tests for each line type. Not run in this change.
- **By:** soma-enyi
- **Closes:** #64
- **Status:** In review.

### scanner — poetry.lock parser

- **What:** Adds `parse_poetry_lock` in [`pypi.rs`](src/lockfile/pypi.rs),
  wired into `parse_file` for `poetry.lock`. Reads every `[[package]]`
  entry (`name`, `version`, `category`/`groups`) with the `toml` crate and
  applies PEP 503 name normalization (lowercase, runs of `-`, `_`, `.`
  collapsed to `-`). `dev` is `true` for `category = "dev"` or for a
  `groups` list without `main`; `direct` is always `false`.
- **Tests:** Unit tests over the new `tests/fixtures/poetry.lock` fixture
  (old `category` and new `groups` styles, name normalization). Not run in
  this change.
- **By:** soma-enyi
- **Closes:** #65
- **Status:** In review.

### CLI — read package names from stdin

- **What:** `depradar name -` now reads newline-separated package names from
  stdin instead of scoring a package literally named `-`, so bulk checks can
  be piped (`cat names.txt | depradar name -`). Blank lines and `#` comments
  are skipped, and surrounding whitespace is trimmed. Text output prints one
  result per name; `--format json` prints a single JSON array of results.
- **Tests:** `assert_cmd` tests pipe three names (plus a comment and a blank
  line) through `name -` in both text and JSON modes.
- **By:** eeshasalaudeen734-creator
- **Closes:** #82
- **Status:** Merged.

### CLI — shell completions

- **What:** Adds `depradar completions <bash|zsh|fish|powershell>` to
  [`src/bin/depradar.rs`](src/bin/depradar.rs) using `clap_complete` to
  generate shell completion scripts to stdout, and documents installation
  steps for Bash, Zsh, Fish, and PowerShell in [`README.md`](README.md).
- **Tests:** `assert_cmd` integration tests in [`tests/cli.rs`](tests/cli.rs)
  verifying that `bash`, `zsh`, `fish`, and `powershell` each exit zero and
  emit non-empty completion scripts referencing `depradar`, and that
  unsupported shell names exit with code 2.
- **By:** deshrajvermay9517-png
- **Closes:** #86
- **Status:** In review.

### scanner — Markdown report format (initial implementation)

- **What:** Adds initial Markdown report output format in
  [`markdown.rs`](src/report/markdown.rs). Renders scan reports as Markdown
  suitable for GitHub issues and PR comments. Output includes a summary
  table (packages scanned, typosquats, vulnerabilities, suppressed,
  warnings), detailed sections per finding with severity badges, and
  sections for suppressed findings and warnings. Each finding shows package
  details, ecosystem, severity, scope (direct/transitive), dependency path,
  and specific details based on finding type (typosquat with edit distance
  and homoglyph information, or vulnerability with OSV reference links).
- **Tests:** Snapshot test using `insta` covering a report with both
  typosquat and vulnerability findings. Unit tests for severity badge colors
  and ecosystem formatting across all supported ecosystems.
- **By:** coderolisa
- **Closes:** Partial contribution to markdown report issue (1% implementation)
- **Status:** In progress.

### Deployment script and DEPLOYMENT.md — #119

- **What:** Adds `scripts/deploy.sh`, which builds the contract wasm
  (`stellar contract build`), deploys it to the configured network, runs
  `initialize --admin <identity>`, and prints the resulting contract ID.
  Adds `DEPLOYMENT.md` covering creating and funding an identity, running
  the script, verifying the deployment with the `client/` read client, and
  updating the contract ID in `README.md`, `client/` and `frontend/`.
- **Verified:** A fresh testnet deploy was run end to end with the script;
  the run and the resulting contract ID are linked in the PR. Every command
  and link in `DEPLOYMENT.md` was executed and checked.
- **By:** TBD
- **Closes:** #119
- **Status:** In review.

### deprader_router — reject self-claims by the bounty creator — #171

- **What:** `submit_claim` now returns the new `CreatorCannotClaim` error
  when `claimant == bounty.creator`, so a creator can no longer submit a
  claim on their own bounty.
- **Tests:** A new contract test asserts the error is returned for a
  self-claim; existing tests are unchanged. Snapshots regenerated.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings`,
  `cargo test --workspace` and the wasm build all pass.
- **By:** TBD
- **Closes:** #171
- **Status:** In review.

### deprader_router — minimum bounty amount per token — #172

- **What:** Adds an admin-set `min_amount` per allowed token (default 0,
  with a setter and getter). `create_bounty` rejects amounts below the
  token's minimum with the new `AmountTooSmall` error.
- **Tests:** Contract tests cover amounts below, at and above the minimum,
  plus the default of 0. Snapshots regenerated.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings`,
  `cargo test --workspace` and the wasm build all pass.
- **By:** TBD
- **Closes:** #172
- **Status:** In review.

### deprader_router — `get_config` getter — #173

- **What:** Adds `get_config() -> Config`, returning the contract's
  `REVIEW_WINDOW_SECS`, `MAX_CLAIMS_PER_BOUNTY`, `MAX_REF_LEN` and
  `MAX_BOUNTY_DURATION_SECS` as a struct, so front ends stop hard-coding
  them.
- **Tests:** A contract test asserts the getter matches the constants.
  Snapshots regenerated.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings`,
  `cargo test --workspace` and the wasm build all pass.
- **By:** TBD
- **Closes:** #173
- **Status:** In review.
