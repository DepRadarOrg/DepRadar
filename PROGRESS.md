# Progress Log

This file is the running record of concrete work landed in this repo: what
was built, who built it, and which issue (if any) it closes. It exists so
that "what's actually shipped" is answerable by reading one file, rather
than reconstructing it from git history or a scattered issue tracker.

Every issue's Definition of Done requires an entry here before the issue
is closed — see [CONTRIBUTING.md](CONTRIBUTING.md).

## Entries

### Devex — Replace `unwrap()` / `expect()` in `src/lockfile` non-test code — #444

- **What:** Audited all non-test code across `src/lockfile/` (`mod.rs`, `cargo.rs`, `npm.rs`, `pnpm.rs`, `pypi.rs`, `yarn.rs`). Confirmed zero `unwrap()` or `expect()` calls exist outside `#[cfg(test)]`, with all parsers properly returning `Result<_, ParseError>` and `Result<_, LockfileError>`. Documented the unreachable match arm invariant in `src/lockfile/mod.rs` for early-returned lockfile kinds.
- **Verified:** Automated grep confirms 0 `unwrap()`/`expect()` instances in non-test functions.
- **By:** genda-benjamin
- **Closes:** #444

### Scanner — Read dependencies from `pyproject.toml` — #327

- **What:** `pyproject.toml` is a recognised file (`LockfileKind::PyprojectToml`).
  `pypi::parse_pyproject_with_warnings` reads PEP 621
  `[project.dependencies]` and every group of
  `[project.optional-dependencies]` as direct dependencies, parsing each
  PEP 508 entry the same way as a `requirements.txt` line (extras and
  environment markers are ignored). Entries without an exact `==` / `===` pin
  are skipped with a warning, and a pin listed in several groups is reported
  once. `parse_lockfile` and `parse_file` dispatch to it, so `depradar scan`
  and directory scans pick up `pyproject.toml` files.
- **Tests:** `pyproject_keeps_pinned_entries_and_warns_about_the_rest` in
  `src/lockfile/pypi.rs` runs the new `tests/fixtures/pyproject.toml`
  (pinned, ranged and extras entries); `detect` covers the new file name. Not
  run yet.
- **By:** Muhammadcodes112
- **Closes:** #327

### Scanner — Detect `npm-shrinkwrap.json` — #328

- **What:** `lockfile::detect` maps `npm-shrinkwrap.json` to
  `LockfileKind::PackageLock`, since it uses the `package-lock.json` format,
  so it is recognised everywhere `detect` is used.
- **Tests:** a `npm-shrinkwrap.json` case in `detects_every_supported_file_name`
  (`src/lockfile/mod.rs`), and `scan_dir_scans_npm_shrinkwrap` in
  `tests/cli.rs`, which runs `depradar scan` over the new
  `tests/fixtures/npm-shrinkwrap/` fixture and expects its typosquat. Not run
  yet.
- **By:** cyfer-codes
- **Closes:** #328

### Scanner — Flag dependencies installed from git, URLs or local paths — #329

- **What:** `Dependency` gains a `source` field holding the git URL, tarball
  URL or `file:` path a package was installed from when that isn't the
  registry. The parsers fill it from npm's URL `version` or `file:` `resolved`,
  the Yarn v1 entry header (`pkg@git+https://…`, `pkg@file:…`), the pnpm
  `resolution` block (`repo`/`commit`, `tarball`, `directory`), and Cargo's
  `git+…` `source`. `scan_lockfile` reports each such package as a new
  `FindingKind::NonRegistrySource`, rated **low** in `severity_of`, with an
  issue-ref kind of `non-registry-source` and a matching SARIF rule.
  `docs/severity.md` and `docs/issue-ref.md` document the new kind.
- **Tests:** `git_and_file_dependencies_are_flagged_as_non_registry` in
  `tests/scan_lockfile.rs` scans the new `tests/fixtures/non-registry/yarn.lock`
  (one git and one `file:` dependency) and expects two findings; unit tests
  cover the pnpm and Cargo sources. The `npm-basic` fixture's `file:` entry
  is now flagged too, so the scan and SARIF tests that use it expect that
  finding. Not run yet.
- **By:** Muhammadcodes112
- **Closes:** #329

### Scanner — Report each package once across a monorepo scan — #330

- **What:** `DirReport::findings()` in `src/pipeline.rs` groups identical
  findings (same package, version, ecosystem and kind) from every lockfile
  into one `DirFinding` that lists the lockfiles it came from. The text output
  of a directory scan now prints that grouped list once, e.g.
  `requests@2.19.0 [direct]: known vulnerability … (in services/api/requirements.txt, services/worker/requirements.txt)`,
  and each `== lockfile ==` section keeps only its scan line, suppressions,
  warnings and summary. Per-lockfile `Report::findings` and the JSON output
  are unchanged.
- **Tests:** `scan_dir_reports_a_shared_finding_once_with_every_lockfile` in
  `tests/scan_dir.rs`: two lockfiles with the same vulnerable package give one
  finding with two paths. Not run yet.
- **By:** Muhammadcodes112
- **Closes:** #330

### Scanner — Report unpinned dependencies — #331

- **What:** `Summary` gains an `unpinned` count of packages with no exact
  version (such as `requests>=2`), which are never sent for a CVE lookup.
  `Report::unpinned()` lists their names. The text output (`Report`'s
  `Display` and `report::text`) adds an `Unpinned (N, ...)` section and
  `N unpinned` to the summary line; the JSON report adds `summary.unpinned`
  and an `unpinned` name list.
- **Tests:** `counts_and_lists_unpinned_requirements` in
  `tests/scan_lockfile.rs` and `scan_reports_unpinned_requirements_in_text_and_json`
  in `tests/cli.rs`, over the new `tests/fixtures/pypi-unpinned/requirements.txt`
  fixture (two unpinned lines). Not run yet.
- **By:** cyfer-codes
- **Closes:** #331

### Scanner — Dependency-confusion check for private package names — #332

- **What:** `depradar.toml` accepts `private_packages = [...]`
  (`ProjectConfig::private_packages`). The new `registry::confusion` module's
  `find_public_names` queries the public npm registry for each name, encoding
  scoped names the same way as `registry::npm`, and returns the names that
  exist publicly. A `404` means unclaimed; any other failure is returned as an
  error instead of being read as "not found".
- **Tests:** mock-registry tests in `src/registry/confusion.rs` (a private name
  that exists publicly is flagged, a scoped unclaimed name is not, and a `500`
  is an error), plus a `private_packages` parse test in `src/config.rs`. Not
  run yet.
- **By:** cyfer-codes
- **Closes:** #332

### Scanner — Scan only lockfiles changed since a git ref — #333

- **What:** `depradar scan <dir> --changed-since <git-ref>` scans only the
  lockfiles that `git diff --name-only <git-ref>` lists, so CI on a large
  monorepo checks just the lockfiles a PR touched. The library side is
  `pipeline::scan_dir_changed_since` and `pipeline::changed_files`;
  unchanged lockfiles are skipped before they are parsed or sent to OSV. An
  unknown ref is an error rather than an empty scan.
- **Tests:** `scan_dir_changed_since_scans_only_changed_lockfiles` in
  `tests/scan_dir.rs` builds a temp git repo with two lockfiles, changes one,
  and checks that only that one is scanned. Not run yet.
- **By:** Muhammadcodes112
- **Closes:** #333

### Typosquat — RubyGems popular-package dataset — #334

- **What:** `data/popular-packages/rubygems-top1000.json` holds the 1000
  most-downloaded gems, ranked by rubygems.org total download counts via the
  BestGems ranking (rubygems.org's own stats page stops at 100).
  `scripts/update-datasets.py rubygems` regenerates it, and
  `typosquat::dataset` loads it alongside the npm, PyPI and crates.io lists,
  so RubyGems names are scored and ranked like the others. The dataset
  README documents the source.
- **Tests:** the dataset validation test now covers the RubyGems file (unique,
  trimmed, lowercase, valid gem names); the loader test checks `rails` and
  `nokogiri`; `rubygems_typosquat_is_flagged` in `src/typosquat/scanner.rs`
  flags `nokogri` as one edit from `nokogiri`. Not run yet.
- **By:** cyfer-codes
- **Closes:** #334

### Docs — Contract error codes reference — #449

- **What:** Adds [`docs/contract-errors.md`](docs/contract-errors.md) mapping every Soroban contract error code from `contracts/deprader_router/src/lib.rs` to its enum variant, triggering methods, failure conditions, and remediation guidance.
- **Verified:** Every error code, variant name, and condition cross-referenced against `contracts/deprader_router/src/lib.rs`.
- **By:** genda-benjamin
- **Closes:** #449

### Docs — Module README for `src/behavioral/` — #454

- **What:** Adds [`src/behavioral/README.md`](src/behavioral/README.md) documenting each behavioral detector (`scripts`, `network`, `setup_py`, `binaries`, `obfuscation`, `diff`, `tarball`), detailing its analyzed files, heuristic weights/signals, and severity levels consistent with `docs/severity.md`.
- **Verified:** Every detector description, heuristic weight, constant, and link was verified against `src/behavioral/*.rs` and `docs/severity.md`.
- **By:** genda-benjamin
- **Closes:** #454

### scanner — composer.lock parser — #323

- **What:** Adds `parse_composer_lock` in [`composer.rs`](src/lockfile/composer.rs),
  registered in `lockfile::detect` for `composer.lock` and dispatched from
  `parse_file` and `parse_lockfile` with ecosystem `Packagist`. Entries in
  `packages` and `packages-dev` are returned with their pinned versions, the
  latter with `dev: true`; `direct` is `false` because the format doesn't
  record it. Malformed JSON fails with `ParseError::Json`.
- **Tests:** Unit tests over the new `tests/fixtures/composer-basic/composer.lock`
  fixture (names, versions, `direct`/`dev` flags) and invalid input. Not run
  in this change.
- **By:** cyber-punk112
- **Closes:** #323
- **Status:** In review.

### scanner — go.sum parser — #324

- **What:** Adds `parse_go_sum` in [`go.rs`](src/lockfile/go.rs), registered
  in `lockfile::detect` for `go.sum` and dispatched from `parse_file` and
  `parse_lockfile` with ecosystem `Go`. Each `module version hash` line with a
  plain version becomes a dependency (each pair once); `version/go.mod` lines,
  which only checksum a module's `go.mod`, are skipped. `direct` and `dev` are
  `false` because the format doesn't record them. Lines with any other shape
  fail with `ParseError::Malformed`.
- **Tests:** Unit tests over the new `tests/fixtures/go-basic/go.sum` fixture
  (names, versions, `direct`/`dev` flags) and malformed lines. Not run in this
  change.
- **By:** cyber-punk112
- **Closes:** #324
- **Status:** In review.

### scanner — Yarn Berry (v2+) yarn.lock parser — #326

- **What:** `parse_yarn_lock` in [`yarn.rs`](src/lockfile/yarn.rs) now
  recognises a Berry lockfile by its top-level `__metadata` key and parses it
  as YAML. Each entry's `resolution` (`name@npm:1.2.3`) gives the package name
  and version, including scoped names; `workspace:` entries and other
  protocols (`patch:`, `link:`, ...) are skipped. v1 lockfiles are parsed as
  before, and both go through `yarn.lock` detection.
- **Tests:** Unit tests over the new `tests/fixtures/yarn-berry/yarn.lock`
  fixture, and a `parse_file` test over the v1 and Berry fixtures. Not run in
  this change.
- **By:** cyber-punk112
- **Closes:** #326
- **Status:** In review.

### scanner — gradle.lockfile parser — #325

- **What:** Adds `parse_gradle_lockfile` in [`gradle.rs`](src/lockfile/gradle.rs),
  registered in `lockfile::detect` for `gradle.lockfile` and dispatched from
  `parse_file` and `parse_lockfile` with ecosystem `Maven`. Each
  `group:artifact:version=configurations` line becomes a `group:artifact`
  dependency; comments and the `empty=` line are skipped. `dev` is set when
  every configuration is a test one; `direct` is `false` because the format
  doesn't record it. Lines with any other shape fail with the new
  `ParseError::Malformed`.
- **Tests:** Unit tests over the new `tests/fixtures/gradle-basic/gradle.lockfile`
  fixture (names, versions, `direct`/`dev` flags) and malformed lines. Not run
  in this change.
- **By:** cyber-punk112
- **Closes:** #325
- **Status:** In review.

### scanner — Gemfile.lock parser — #322

- **What:** Adds `parse_gemfile_lock` in [`ruby.rs`](src/lockfile/ruby.rs)
  and registers `Gemfile.lock` in `lockfile::detect`, `parse_file` and
  `parse_lockfile` (ecosystem `RubyGems`). Gems are read from the `specs:`
  of the `GEM`, `GIT` and `PATH` sections (`name (version)`), with platform
  suffixes such as `-x86_64-linux` dropped and each `(name, version)` pair
  returned once. A gem is `direct` when it is listed under `DEPENDENCIES`;
  `dev` is always `false` because the lockfile doesn't record groups.
  Malformed input returns the new `ParseError::Invalid`.
- **Tests:** Unit tests and a `tests/lockfile.rs` fixture test over the new
  `tests/fixtures/Gemfile.lock` (names, versions, direct/transitive flags,
  malformed input). Not run in this change.
- **By:** elmuby
- **Closes:** #322
- **Status:** In review.

### scanner — pdm.lock parser — #321

- **What:** Adds `parse_pdm_lock` in [`pypi.rs`](src/lockfile/pypi.rs) and
  registers `pdm.lock` in `lockfile::detect`, `parse_file` and
  `parse_lockfile` (ecosystem `PyPI`). Every `[[package]]` entry is read
  with PEP 503 name normalization. `dev` is `true` when `groups` doesn't
  include `default`. `pdm.lock` has no project entry, so a package is
  `direct` when no other locked package lists it in `dependencies`.
  Malformed input returns a `ParseError`.
- **Tests:** Unit tests and a `tests/lockfile.rs` fixture test over the new
  `tests/fixtures/pdm.lock` (names, versions, direct/transitive flags,
  malformed input). Not run in this change.
- **By:** elmuby
- **Closes:** #321
- **Status:** In review.

### scanner — uv.lock parser — #320

- **What:** Adds `parse_uv_lock` in [`pypi.rs`](src/lockfile/pypi.rs) and
  registers `uv.lock` in `lockfile::detect`, `parse_file` and
  `parse_lockfile` (ecosystem `PyPI`). Every `[[package]]` entry is read
  with PEP 503 name normalization, except the project itself and workspace
  members (`virtual` / `editable` sources). A package is `direct` when a
  project package lists it in `dependencies`, `optional-dependencies` or
  `dev-dependencies`, and `dev` when it is listed only in
  `dev-dependencies`. Malformed input returns a `ParseError`.
- **Tests:** Unit tests and a `tests/lockfile.rs` fixture test over the new
  `tests/fixtures/uv.lock` (names, versions, direct/transitive flags,
  malformed input). Not run in this change.
- **By:** elmuby
- **Closes:** #320
- **Status:** In review.

### scanner — bun.lock parser — #319

- **What:** Adds `parse_bun_lock` in [`bun.rs`](src/lockfile/bun.rs) and
  registers `bun.lock` in `lockfile::detect`, `parse_file` and
  `parse_lockfile` (ecosystem `Npm`). Trailing commas are stripped before
  the file is read as JSON. Each `packages` entry's `name@version` is
  returned; workspace, git and file entries are skipped. A package is
  `direct` when its key is its bare name and the root workspace declares
  it, and `dev` when the root lists it only in `devDependencies`.
  Malformed input returns a `ParseError`.
- **Tests:** Unit tests and a `tests/lockfile.rs` fixture test over the new
  `tests/fixtures/bun.lock` (names, versions, direct/transitive flags,
  malformed input). Not run in this change.
- **By:** elmuby
- **Closes:** #319
- **Status:** In review.
- **Status:** In review.

### Frontend — Manual light/dark theme toggle — #135

- **What:** A System / Light / Dark toggle in the header
  (`frontend/src/components/ThemeToggle.tsx`). It sets `data-theme` on `<html>`,
  and `lib/theme.ts` persists the choice in `localStorage` inside a try/catch.
  `main.tsx` applies the stored choice before the first render. `index.css` adds
  a light palette that mirrors the dark tokens, under
  `:root[data-theme="light"]` and `:root:not([data-theme="dark"])` for
  `prefers-color-scheme: light`. Hard-coded colours that didn't work on light
  backgrounds were moved to tokens. The frontend README is updated, since it said
  the UI was dark-only.
- **Tests:** not written or run yet.
- **By:** Druw-agon
- **Closes:** #135

### Frontend — Accessibility pass — #134

- **What:** Static review of `frontend/src/` against the issue. The leaderboard
  table gets a screen-reader `<caption>` (new `.sr-only` utility in
  `index.css`) and `scope="col"` on its headers. The existing global
  `:focus-visible` outline and the global `prefers-reduced-motion` rule already
  cover links, buttons and CSS animations, and badges already carry text labels,
  so they are unchanged.
- **Not done:** the axe before/after report requires running the app in a
  browser. It has not been produced, so the serious/critical findings are not
  yet confirmed.
- **By:** Druw-agon
- **Refs:** #134 (partial: the axe report is still outstanding, so the issue stays open)

### Frontend — Copy-to-clipboard on addresses — #133

- **What:** `AddressLink` in `frontend/src/components/ui.tsx` takes an optional
  `copy` prop that adds a `CopyButton` (`aria-label="Copy address"`) beside the
  link. The bounty detail page enables it for the creator and token addresses.
  `CopyButton` now accepts an optional `label`.
- **Tests:** new `AddressLink` cases in `components/ui.test.tsx` with a mocked
  `navigator.clipboard`. Not run yet.
- **By:** Druw-agon
- **Closes:** #133

### Frontend — Read token symbol and decimals from the token contract — #132

- **What:** `getTokenInfo()` in `frontend/src/lib/contract.ts` queries `symbol()`
  and `decimals()` on a bounty's token contract, cached per token; failed
  lookups are not cached. A new `useTokenInfo` hook
  (`frontend/src/lib/useTokenInfo.ts`) feeds the values into `formatAmount` and
  `tokenSymbol` on the bounty card and detail page. Native XLM skips the lookup
  and keeps 7 decimals; on error the previous behaviour is used.
- **Tests:** `lib/tokenInfo.test.ts` (mocked RPC, including a 6-decimal token)
  and new cases in `lib/format.test.ts`. Not run yet.
- **By:** Druw-agon
- **Closes:** #132

### CI — Deploy the dashboard to GitHub Pages — #139

- **What:** Adds [`.github/workflows/pages.yml`](.github/workflows/pages.yml),
  which builds `frontend/` on pushes to `main` (and on manual dispatch) and
  deploys it with `actions/deploy-pages`. The build sets `VITE_SITE_URL` to
  the Pages URL so the social-preview tags get an absolute image URL. The
  live URL is added to the README's dashboard section.
- **Verified:** Not yet — the workflow has not been run on a fork or on this
  PR, and the live page has not been loaded.
- **By:** Farez Abel
- **Closes:** #139

### Frontend — Address profile page — #138

- **What:** Adds `/address/:id` (`frontend/src/pages/Address.tsx`), listing
  the bounties an account has funded and the claims it has submitted. It
  scans all bounties with the contract getters, since there is no claimant
  index yet. `AddressLink` now opens this page, with a small external-link
  icon next to it for the explorer. Contract addresses (such as tokens) keep
  linking straight to the explorer, since they have no profile.
- **Verified:** Not yet — the component test (`Address.test.tsx`, with the
  contract layer mocked) was written but not run.
- **By:** Farez Abel
- **Closes:** #138

### Frontend — Live deadline countdown and review-window indicator — #137

- **What:** Open bounties on the detail page show a live countdown
  (`3d 4h left`) that refreshes every minute (`frontend/src/lib/useNow.ts`).
  After the deadline, the page shows "Under review — creator can reclaim in
  X", based on the 7-day `REVIEW_WINDOW_SECS` from the contract. Duration
  and window helpers live in `frontend/src/lib/format.ts`, with boundary
  tests at the deadline and the reclaim time.
- **Verified:** Not yet — tests and build were not run for this change.
- **By:** Farez Abel
- **Closes:** #137

### Frontend — Code-split the Stellar SDK out of the initial bundle — #136

- **What:** `frontend/src/App.tsx` now loads every page with `React.lazy`
  behind a `Suspense` boundary, so `@stellar/stellar-sdk` (pulled in by
  `lib/contract.ts`) is no longer part of the main chunk. The shell and
  navigation render immediately while a page's chunk loads.
- **Verified:** Not yet — build sizes before/after are to be reported from
  `vite build` on the PR.
- **By:** Farez Abel
- **Closes:** #136

### Frontend — Network switcher (testnet / mainnet config) — #131

- **What:** `frontend/src/config.ts` defines testnet and mainnet presets (RPC
  URL, passphrase, router contract, native XLM contract, explorer network).
  The top bar gets a network picker, and the choice is saved in
  `localStorage` (access wrapped in try/catch). Switching reloads the page,
  because the RPC client and contract are created at startup. Mainnet is
  hidden until `VITE_MAINNET_CONTRACT_ID` is set.
- **Verified:** Not verified. Tests, lint and build were skipped for this
  change at the author's request.
- **Closes:** #131
- **Status:** In review.

### Frontend — Live activity feed from contract events — #130

- **What:** `frontend/src/lib/events.ts` pulls the router's recent contract
  events with `rpc.Server.getEvents` (about one day of ledgers) and decodes
  each one into a line such as "Claim #1 accepted on bounty #1 — 5 XLM".
  `frontend/src/components/Activity.tsx` shows the newest 30 on the board,
  polls every 15 seconds, and links each line to its transaction.
- **Verified:** Not verified. Tests, lint and build were skipped for this
  change at the author's request.
- **Closes:** #130
- **Status:** In review.

### Frontend — Cancel button for eligible creators — #129

- **What:** `frontend/src/lib/cancel.ts` computes when `cancel_bounty` accepts
  a creator: the deadline has passed with no pending claims, or the 7-day
  review window has passed. It also blocks cancellation while the contract is
  paused. On the detail page, the connected creator sees a Cancel & refund
  button when eligible, or the date it becomes available otherwise.
- **Verified:** Not verified. Tests, lint and build were skipped for this
  change at the author's request.
- **Closes:** #129
- **Status:** In review.

### Frontend — Admin review controls (reject / release) — #128

- **What:** On a bounty's detail page, a Freighter-connected wallet that
  equals `get_admin()` sees Reject and Release buttons on each pending claim.
  Release asks for confirmation first, showing the amount and recipient.
  Actions are signed in the wallet and submitted through
  `frontend/src/lib/contract.ts` (`invokeSigned`), and the page refreshes
  after each one. The transaction hash is shown with a link to the explorer.
- **Verified:** Not verified. Tests, lint and build were skipped for this
  change at the author's request.
- **Closes:** #128
- **Status:** In review.

### Frontend — Submit-claim form on the bounty page — #127

- **What:** On open bounties, the bounty page shows a "Submit claim" form with
  a proof reference. The form is disabled with an explanation when the wallet
  is not connected, is on the wrong network, is the bounty's creator, the
  deadline has passed, or the wallet already has a claim (`get_claim_id`).
  Submitting builds, signs and submits `submit_claim` as the connected wallet,
  then reloads the bounty and its claims. Shared write path: `frontend/src/lib/tx.ts`.
- **Verified:** Not run; no tests, lint, typecheck or build were executed for
  this change. No testnet transaction hash is recorded.
- **By:** olivia-brien
- **Closes:** #127

### Frontend — Create-bounty form — #126

- **What:** Adds `/new`, a form for a finding reference, a reward in XLM, and
  a deadline date (at most one year out). Submitting builds and simulates
  `create_bounty` as the connected Freighter wallet, signs it, submits it,
  waits for confirmation, then opens the new bounty. Validation in
  `frontend/src/lib/newBounty.ts` mirrors the contract: amount > 0, reference
  1–256 bytes, deadline within one year. The shared write path is
  `frontend/src/lib/tx.ts`. The reward is paid in native XLM.
- **Verified:** Not run; no tests, lint, typecheck or build were executed for
  this change. No testnet transaction hash is recorded.
- **By:** olivia-brien
- **Closes:** #126

### Frontend — Connect a Freighter wallet — #125

- **What:** Adds `@stellar/freighter-api` and a `WalletProvider` / `useWallet()`
  context in `frontend/src/wallet/`. The top bar has a Connect button that shows
  the short address and network once connected, and warns when the wallet's
  network differs from `NETWORK`. An already-approved site reconnects on load;
  if the extension is missing the button links to Freighter instead.
- **Verified:** Not run; no tests, lint, typecheck or build were executed for
  this change.
- **By:** olivia-brien
- **Closes:** #125

### Frontend — Paginate the bounty list — #124

- **What:** The dashboard now loads 20 bounties at a time, newest first, with
  a "Load more" button. `listBountiesPage()` in `frontend/src/lib/contract.ts`
  pages over bounty IDs until the contract exposes a paginated getter. The
  status filter, chip counts, hero stats and radar apply to the bounties
  loaded so far.
- **Verified:** Not run; no tests, lint, typecheck or build were executed for
  this change.
- **By:** olivia-brien
- **Closes:** #124

### Frontend — Sort the bounty board — #123

- **What:** Adds a sort select next to the filter chips on the bounty board
  (`frontend/src/pages/BountyList.tsx`) with four options: Newest, Highest
  reward, Closing soon and Most claims. The choice is stored in the `sort` URL
  param; the default (Newest) leaves the URL clean. Changing a status chip now
  keeps the sort param. Sorting lives in `frontend/src/lib/bountySort.ts`,
  which compares rewards as `bigint`. Adds unit tests in
  `frontend/src/lib/bountySort.test.ts`, including amounts beyond
  `Number.MAX_SAFE_INTEGER` and beyond 64 bits.
- **Verified:** Not yet run. The unit tests and the lint, typecheck, test and
  build steps have not been executed. `frontend/src/lib/contract.ts` still
  imports from the missing `@deprader/contract-types` package, so the page
  cannot build until that is resolved.
- **By:** <github-username>
- **Closes:** #123

### Frontend — Search bounties by finding text — #122

- **What:** Adds a search input to the bounty board in
  `frontend/src/pages/BountyList.tsx`. It filters by a case-insensitive
  substring of `issueRef`, combined with the status filter chips. The input
  is debounced by 150ms (`frontend/src/lib/useDebouncedValue.ts`). When nothing
  matches it shows "No matches" with a clear button. Matching lives in
  `frontend/src/lib/bountySearch.ts`. Adds a component test in
  `frontend/src/pages/BountyList.test.tsx` covering match, no match and clear.
- **Verified:** Not yet run. The component test and the lint, typecheck,
  test and build steps have not been executed. `frontend/src/lib/contract.ts`
  still imports from the missing `@deprader/contract-types` package, so the
  page cannot build until that is resolved.
- **By:** <github-username>
- **Closes:** #122

### Docs — Document the contract's events — #121

- **What:** Adds [`docs/events.md`](docs/events.md) with one table per event
  for `created`, `claimed`, `rejected`, `released`, `cancelled`, `admin_set`
  and `admin_prp`: topics, data tuple and position, and what each value means.
  The layouts were taken from the `publish` calls in
  `contracts/deprader_router/src/lib.rs`. The page also lists the remaining
  events by name. README's Documentation section links to the new page.
- **Verified:** Not yet complete. The table layouts were checked against the
  source only. The "example decoded from testnet" sections are placeholders
  marked TODO, because they need a real testnet event. Links in the new page
  have not been checked.
- **By:** <github-username>
- **Closes:** #121

### Contract — Resource budget regression test — #120

- **What:** Adds `test_resource_budget_for_bounty_lifecycle` to
  `contracts/deprader_router/src/test.rs`. It resets
  `env.cost_estimate().budget()` before `create_bounty`, `submit_claim` and
  `release`, prints the measured CPU instructions and memory bytes for each,
  and asserts they are under the ceilings. The ceilings are the Soroban
  per-transaction network limits (100,000,000 CPU instructions and 40 MiB),
  documented in a comment next to the constants.
- **Verified:** Not yet run. The test was written without executing
  `cargo test`, `cargo fmt`, `cargo clippy` or the wasm build. Measured
  numbers, headroom and snapshot updates are still outstanding.
- **By:** <github-username>
- **Closes:** #120

### CI — Guard against conflict markers and pasted code fences — #282

- **What:** Adds `scripts/check-source.sh`, a small Bash script that greps
  every tracked `.rs`, `.ts`, and `.tsx` file for Git conflict markers
  (`<<<<<<<`, `=======`, `>>>>>>>`) and Markdown code-fence delimiters
  (` ``` `). The script exits non-zero if any pattern is found and prints the
  offending filenames. It is added as the **first step** of the `lint` job in
  `.github/workflows/ci.yml`, before the Rust toolchain is set up, so garbled
  files are caught before the compiler runs.
- **Verified:** The script passes on the current clean tree; introducing a
  conflict marker into any `.rs` or `.ts` file causes the script to exit 1.
- **By:** joshfatoye0011-bit
- **Closes:** #282

### Frontend — Add missing key props in Layout.tsx — #279

- **What:** Two places in `frontend/src/components/Layout.tsx` required `key`
  props to avoid React reconciliation warnings.
  1. The `<code>` element inside the `Trans` component's `components` array in
     the footer — React-i18next indexes the array and React requires a `key`
     on each element. Added `key="code"` and an explanatory comment so the
     requirement is not silently dropped in future edits.
  2. The nav links in the topbar were static repeated JSX without keys.
     Refactored to a `NAV_LINKS` constant array mapped with `key` props on
     each rendered `<Link>` / `<a>`, which also makes the nav easier to extend.
- **Verified:** No "missing key" console warnings; `npm run typecheck` passes;
  visual output is unchanged.
- **By:** joshfatoye0011-bit
- **Closes:** #279

### CLI — Regenerate --help snapshot to include explain subcommand — #281

- **What:** `tests/snapshots/depradar-help.txt` was missing the `explain`
  subcommand, which is already implemented in `src/bin/depradar.rs` (added in
  #180). The snapshot test `help_text_matches_snapshot` therefore failed once
  the crate compiled. The snapshot is regenerated by running
  `cargo run --bin depradar -- --help` and capturing the output. The `explain`
  entry now appears between `scan` and `watch` in the commands list, and the
  `depradar explain GHSA-xxxx-xxxx-xxxx` example line is present in the
  `after_help` examples block.
- **Verified:** Snapshot matches the `Explain` variant's doc comment and the
  `EXAMPLES` constant in `src/bin/depradar.rs`. `cargo test --test cli` now
  passes.
- **By:** joshfatoye0011-bit
- **Closes:** #281

### Security — Document and suppress cargo audit advisory failures — #280

- **What:** The weekly Dependency Audit workflow was failing on two open
  advisories: `RUSTSEC-2026-0387` (`crypto-bigint 0.5.5`, fixed in `0.7.6`)
  and a `quinn-proto 0.11.17` ACK-delay panic advisory. Because neither crate
  can be updated in one step without migrating the whole dependency tree,
  both are added as dated, documented `[advisories.ignore]` entries in
  `deny.toml` with mandatory revisit dates of **2026-12-15** and links to the
  upstream tracking issues. The entries explain why the fix is blocked and
  what the resolution step is (`cargo update -p <crate>` once a patched
  version is available).
- **Verified:** `deny.toml` syntax is valid; entries carry explicit reasons
  and revisit dates per the project's audit policy.
- **By:** joshfatoye0011-bit
- **Closes:** #280

### Docs — Architecture guide with sequence diagrams — #155

- **What:** Adds [`docs/architecture.md`](docs/architecture.md) with Mermaid
  sequence diagrams for: scan → finding (lockfile through pipeline to
  report), create bounty → claim → review → release (complete bounty
  lifecycle), cancel bounty (refund path), and dashboard reads (RPC
  simulation flow). Describes each module's responsibility in one paragraph:
  Crawler (discovers packages from registries), Scanner (typosquat, CVE,
  behavioral, license checks), Bounty Router (locks funds, records claims,
  pays out), Dashboard (read-only React view), CLI (scan command) and Client
  (Node wrapper for contract reads). The README now links to it under a new
  Documentation section.
- **Verified:** All internal links checked; Mermaid syntax validated; every
  contract method and module referenced exists in the codebase.
- **Closes:** #155

### Docs — Threat model for the bounty router — #156

- **What:** Adds [`docs/threat-model.md`](docs/threat-model.md) documenting
  actors (funder, researcher, admin), assets (locked funds, claim priority,
  contract state), trust assumptions (admin honesty, key security, network
  correctness, token legitimacy), and each known attack with its mitigation
  or open status: claim squatting (✅ multiple claims per bounty), admin
  compromise (⚠️ two-step rotation but current admin can release
  immediately), malicious token (⚠️ token allowlist not yet implemented,
  #121/#122), griefing via claim spam (✅ capped at 50 claims per bounty),
  storage expiry (✅ TTL extended on every write), creator rug-pull (✅
  deadline + review window enforced), double-release (✅ status check
  prevents), claim proof forgery (⚠️ admin responsibility, no on-chain
  validation). Links related issues and test coverage for each threat. The
  README Documentation section now includes it.
- **Verified:** All contract method references and test names checked; issue
  links exist; security assumptions match SECURITY.md.
- **Closes:** #156

### Docs — Publish Rust API docs — #157

- **What:** Adds `.github/workflows/docs.yml` that builds `cargo doc
  --no-deps --workspace` and the dashboard, then publishes both to GitHub
  Pages under `/api/` (Rust docs) and `/` (dashboard). Turns on
  `#![warn(missing_docs)]` in `src/lib.rs`. The README Documentation section
  links to the published API docs at
  `https://depradarorg.github.io/DepRadar/api/`.
- **Verified:** Workflow syntax valid; README link added; `#![warn(missing_docs)]`
  present in `src/lib.rs`.
- **Closes:** #157

### Docs — Tutorial: "Report your first typosquat" — #158

- **What:** Adds [`docs/tutorial.md`](docs/tutorial.md) with a guided path
  from scanning a lockfile (`depradar scan package-lock.json` on a typosquat
  like `expresss`), interpreting the result (distance, confidence, severity),
  reporting to the registry (npm malware form), funding a bounty on testnet
  (`stellar contract invoke ... create_bounty` with native XLM), submitting a
  claim (`submit_claim` with a gist proof reference), admin review and release
  (simulated with `release`), verifying the payout (balance check, Stellar
  Expert), and viewing on the dashboard. Uses real commands with the live
  testnet contract ID, token address, and `stellar-cli` invocations. Includes
  a troubleshooting section and commands reference table. The README
  Documentation section now links to it.
- **Verified:** All commands follow the contract's actual interface; testnet
  contract ID and token address match README and DEPLOYMENT.md; every link
  resolves.
- **Closes:** #158

### Frontend — Leaderboard of top researchers — #399

- **What:** Adds `/leaderboard` page ranking researchers by total paid rewards from released bounties and count of bounties won. Implements `rankResearchers` in `frontend/src/lib/insights.ts` with unit tests covering payout ordering, tie-breaking, and filtering of unreleased bounties/unaccepted claims. Adds navigation link in topbar, i18n scaffolding fixes, and routing in `App.tsx`.
- **Verified:** `npm run lint`, `npm run typecheck`, `npm test`, and `npm run build` all pass 100% green in `frontend/`.
- **By:** genda-benjamin
- **Closes:** #399
- **Status:** In review.

### Docs — FAQ — #<this issue>

- **What:** Adds [`docs/FAQ.md`](docs/FAQ.md) with at least ten common
  questions answered from the README, the contract and
  [`SECURITY.md`](SECURITY.md): whether the deployment is mainnet or
  testnet, who decides bounty claims, what counts as a finding, how
  bounties are funded, released and refunded, and how to report a
  vulnerability. The README now links to it.
- **Verified:** every command and link in the new docs was checked against
  the repo; the README link resolves to `docs/FAQ.md`.
- **By:** CodePrincess360
- **Closes:** #<this issue>
- **Status:** In review.


### Typosquat — Go and Packagist popular-package datasets — #335, #336

- **What:** Adds `data/popular-packages/go-top1000.json` (the 1000 most
  imported Go modules, from deps.dev / pkg.go.dev import counts) and
  `data/popular-packages/packagist-top1000.json` (the 1000 most-installed
  Composer packages from packagist.org's popular list).
  `scripts/update-datasets.py` gains `go` and `packagist` sources that
  regenerate both files in the existing format, and `typosquat::dataset`
  loads them for the `Go` and `Packagist` ecosystems, so names in those
  ecosystems are scored and ranked like the others. The dataset README
  documents both sources.
- **Tests:** the dataset validation test now covers both files (unique,
  trimmed, valid module/package names); the loader test checks known
  entries (`github.com/gin-gonic/gin`, `symfony/console`); a known Go
  module typosquat and a known Packagist typosquat are flagged in
  `src/typosquat/scanner.rs`. Not run yet.
- **By:** cyfer-codes
- **Closes:** #335, #336

### Typosquat — separator-swap signal (`-`, `_`, `.`) — #339

- **What:** Adds a signal for names that match a popular package once
  separators are normalised, so `date_fns` (for `date-fns`) and
  `lodash.merge` (for `lodash-merge`) are caught even when they are more
  than one cheap edit away. PyPI is skipped, since PEP 503 already treats
  `-`, `_` and `.` as the same name. The signal is reported on
  `ScanResult` and contributes to the confidence score.
- **Tests:** `-`↔`_` and `.`↔`-` swaps on npm and crates.io are flagged;
  the same swap on PyPI is not. Not run yet.
- **By:** cyfer-codes
- **Closes:** #339

### Typosquat — bitsquatting signal — #341

- **What:** Adds a bitsquatting signal: for each character of a candidate
  name, a single bit flip is applied and the result is looked up in the
  dataset, so names one bit away from a popular package (`lodasj` for
  `lodash`) are flagged. The signal is reported on `ScanResult` and
  contributes to the confidence score.
- **Tests:** three bit-flipped examples are flagged and three negatives
  (the real package, an unrelated name, a two-bit-flip name) are not. Not
  run yet.
- **By:** cyfer-codes
- **Closes:** #341

### Dependency licenses and sources — `cargo deny` — #163

- **What:** Adds [`deny.toml`](deny.toml) allowing MIT, Apache-2.0, BSD,
  ISC and Unicode licenses (with narrow per-crate exceptions for
  `option-ext` (MPL-2.0) and `webpki-roots` (CDLA-Permissive-2.0)), denying
  wildcard versions, unknown registries and unknown git sources, and
  reporting duplicate versions. A `cargo deny` job in the Dependency Audit
  workflow runs `cargo deny check licenses bans sources`.
- **Verified:** `cargo deny check licenses bans sources` passes locally
  (`bans ok, licenses ok, sources ok`).
- **By:** CodePrincess360
- **Closes:** #163
- **Status:** In review.

### Scanner detection on the bounty page — #166

- **What:** New [`crates/typosquat-wasm`](crates/typosquat-wasm) compiles
  the typosquat scorer to WebAssembly with `wasm-bindgen`, mounting
  `src/typosquat` directly so the root crate's network dependencies stay
  out of the wasm build. Its `scan(name)` returns the confidence and the
  signals that fire. The bounty page parses the package from `issue_ref`
  (or uses the structured `package` field) and shows those signals; the
  module is built with `npm run build:wasm` and the panel degrades
  gracefully when it isn't present. CI builds the crate for
  `wasm32-unknown-unknown`.
- **Tests:** Rust integration tests for the detection mapping and JSON
  output; a `ScannerDetection` component test with the wasm module mocked;
  unit tests for the `issue_ref` package parser.
- **By:** CodePrincess360
- **Closes:** #166
- **Status:** In review.

### Error boundary and friendly RPC errors — #167

- **What:** A top-level React `ErrorBoundary` replaces a blank page with a
  message and a Reload button when rendering fails. `ErrorState` now maps
  common RPC failures — network down, rate limited, contract not found —
  to plain-language messages via `lib/errors.ts`, falling back to the raw
  message for anything else.
- **Tests:** Boundary render/reload tests and a test per mapped error.
- **By:** CodePrincess360
- **Closes:** #167
- **Status:** In review.

### Open Graph and social preview tags — #169

- **What:** `frontend/index.html` gains `og:*` and Twitter
  `summary_large_image` card tags with a 1200×630 `og-image.png` derived
  from `assets/banner.svg`. `VITE_SITE_URL` makes the image URL absolute
  for link-preview crawlers.
- **By:** CodePrincess360
- **Closes:** #169
- **Status:** In review.

### CLI — `depradar explain <advisory-id>` — #180

- **What:** Adds `depradar explain <advisory-id>` to
  [`src/bin/depradar.rs`](src/bin/depradar.rs). It fetches the advisory from
  OSV (`GET /v1/vulns/{id}`) and prints its summary, severity, affected
  range, fixed version and references, so a `GHSA-…` seen in a report can be
  inspected without leaving the terminal.
- **Tests:** An `assert_cmd` integration test in [`tests/cli.rs`](tests/cli.rs)
  with the OSV call mocked, checking the printed advisory details.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #180
- **Status:** In review.

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

### Web dashboard — empty-state call to action for researchers — #182

- **What:** `frontend/src/pages/BountyList.tsx` no longer stops at "No
  bounties" when nothing is open. The empty state now explains how bounties
  work (find a vulnerability, open an issue, get paid on release) and links
  to the tutorial, `CONTRIBUTING.md` and the repo's GitHub issues so a
  researcher has a next step.
- **Verified:** `npm run lint`, `npm run typecheck`, `npm run test` and
  `npm run build` pass in `frontend/`; a screenshot of the empty state is in
  the PR.
- **By:** ALLEN-AYODEJI
- **Closes:** #182
- **Status:** In review.

### Read client — `--contract` / `--rpc` flags and env fallbacks — #183

- **What:** `client/src/index.ts` no longer hard-codes the contract ID and
  RPC URL. Both are configurable via `--contract` / `--rpc` flags, falling
  back to `DEPRADAR_CONTRACT_ID` / `DEPRADAR_RPC_URL`, then to the previous
  defaults. `client/README.md` documents the flags, the env vars and the
  precedence order.
- **Verified:** `npm run typecheck` passes in `client/`; the client still
  reads the live testnet contract with the defaults and with each override.
- **By:** ALLEN-AYODEJI
- **Closes:** #183
- **Status:** In review.

### CI — cache-friendly, faster workflow — #184

- **What:** `.github/workflows/ci.yml` now uses `Swatinem/rust-cache` with
  per-job cache keys, passes `--locked` to every Cargo invocation, and adds
  a `concurrency` group that cancels superseded PR runs. Before/after job
  timings are recorded in the PR description.
- **Verified:** the workflow runs green on the PR.
- **By:** ALLEN-AYODEJI
- **Closes:** #184
- **Status:** In review.

### Docs — issue and PR labels guide — #185

- **What:** Adds a "Labels" section to `CONTRIBUTING.md` listing every label
  the repo uses — the area labels (`typosquat`, `contract`, `frontend`, …),
  `good first issue` and `Stellar Wave` — and when each is applied.
- **Verified:** every command and link in the new section was checked.
- **By:** ALLEN-AYODEJI
- **Closes:** #185
- **Status:** In review.

### Integration test — scan → bounty on a local contract — #161

- **What:** Adds [`tests/scan_to_bounty.rs`](tests/scan_to_bounty.rs). It
  scores `expresss` with `score_package`, turns the result into a
  typosquat `Finding`, builds the bounty's `issue_ref` with
  `Finding::issue_ref`, then uses soroban testutils to create the bounty on a
  locally registered router, parse the stored `issue_ref` back, submit a
  claim and release it to the claimant. The root crate gains
  `deprader-router` (with `testutils`) and `soroban-sdk` (with `testutils`)
  as dev-dependencies, so the test runs in `cargo test --workspace`.
- **By:** MamaLove360
- **Closes:** #161
- **Status:** In review.

### Standard `issue_ref` format for findings — #162

- **What:** Documents `<ecosystem>:<package>[@<version>]#<kind>[:<detail>]`
  in [`docs/issue-ref.md`](docs/issue-ref.md). Adds `IssueRef`,
  `IssueKind`, `format_issue_ref`, `parse_issue_ref` and
  `Finding::issue_ref` to [`src/model.rs`](src/model.rs), and mirrors them as
  `formatIssueRef` / `parseIssueRef` in
  [`frontend/src/lib/format.ts`](frontend/src/lib/format.ts). Free-form refs
  parse to "no match" so existing bounties still render as text.
- **Tests:** Round-trip and rejection tests in both Rust and Vitest,
  covering npm scopes, Maven coordinates and unpinned versions.
- **By:** MamaLove360
- **Closes:** #162
- **Status:** In review.

### Security — CycloneDX SBOMs on every release — #164

- **What:** [`.github/workflows/release.yml`](.github/workflows/release.yml)
  gains an `sbom` job that generates CycloneDX JSON SBOMs with
  `cargo cyclonedx` (CLI crate and router contract) and
  `@cyclonedx/cyclonedx-npm` (frontend production dependencies). The
  `publish` job waits for it and attaches the SBOMs to the GitHub Release
  alongside the binaries. The README lists the SBOM files.
- **By:** MamaLove360
- **Closes:** #164
- **Status:** In review.

### Shared contract types between `client/` and `frontend/` — #145

- **What:** Adds `packages/contract-types/`, a small TypeScript package
  exporting the `Bounty` and `Claim` types plus `parseBounty` and
  `parseClaim`, so the two apps decode the router's structs with one
  implementation instead of two copies that can drift. Both `client/` and
  `frontend/` consume it through npm workspaces (a relative import in the
  client's tsconfig paths), and their local parsing code is deleted.
- **Verified:** `npm run typecheck` passes in `client/`; `npm run lint`,
  `npm run typecheck`, `npm test` and `npm run build` pass in `frontend/`.
  The parsing tests moved with the code into the shared package.
- **Closes:** #145

### Docs — Drips integration design — #146

- **What:** Adds [`docs/drips-integration.md`](docs/drips-integration.md):
  what Drips offers on Stellar, three candidate designs (the claimant
  receives directly and then splits; the router sends a share to a
  project's Drips list; Drips streams fund the bounty pool), their
  trade-offs, a recommendation, and the contract changes each design would
  need.
- **Verified:** every command and link in the new doc was checked against
  the repo and the Drips documentation.
- **Closes:** #146

### GitHub ↔ Stellar identity verification for claimants — #147

- **What:** Adds a verification flow in which a claimant posts a message
  signed with their Stellar key in a gist or PR comment, and a verifier
  checks the signature against the claimed address. Verified links are
  stored off-chain as JSON in the repo, and the trust model is documented
  in `docs/`.
- **Verified:** signature-verification tests pass; `cargo fmt`,
  `cargo clippy -- -D warnings` and `cargo test --workspace` pass; all
  commands and links in the new docs were checked.
- **Closes:** #147

### Contract — Drips share on release — #148

- **What:** Following the design doc, `create_bounty` takes an optional
  `drips_share_bps` and destination address. On `release`, that share of
  the bounty is paid to the destination and the rest to the claimant; the
  share is capped and emitted in the `released` event.
- **Verified:** tests cover a 0% share, a typical share and the cap;
  `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test --workspace`
  pass; contract test snapshots are updated and the wasm build succeeds.
- **Closes:** #148

### CI — scan DepRadar's own lockfiles — #165

- **What:** [`.github/workflows/ci.yml`](.github/workflows/ci.yml) gains a
  `self-scan` job that builds the `depradar` CLI and runs
  `depradar --fail-on critical scan` on `Cargo.lock`,
  `client/package-lock.json` and `frontend/package-lock.json`.
- **By:** MamaLove360
- **Closes:** #165
- **Status:** In review.

### CI — code coverage for Rust and frontend — #150

- **What:** [`.github/workflows/ci.yml`](.github/workflows/ci.yml) gains a
  `coverage` job that runs `cargo llvm-cov --workspace --lcov --output-path
  lcov.info` and uploads `lcov.info` as an artifact, and the `frontend` job
  now runs `vitest --coverage` (via `npm run test:coverage`) and uploads the
  `frontend/coverage/` directory as an artifact. `frontend/package.json`
  gains the `test:coverage` script and the `@vitest/coverage-v8` dev
  dependency. The README shows a coverage badge.
- **Verified:** the workflow runs green on the PR and the coverage numbers
  appear in the PR's CI run; both artifacts are downloadable from the run.
- **Closes:** #150
- **Status:** In review.

### DevEx — `justfile` with common tasks — #151

- **What:** Adds a [`justfile`](justfile) with `just check` (fmt, clippy,
  tests), `just wasm` (release wasm build of the router), `just fe`
  (frontend dev server), `just fe-check` (frontend lint, typecheck, tests
  and build) and `just scan <name>` (typosquat scan of a package name).
  `CONTRIBUTING.md` documents `just --list` and the recipes.
- **Verified:** every recipe runs successfully locally (Linux); all
  commands and links in the new docs were checked.
- **Closes:** #151
- **Status:** In review.

### CI — changelog and tagged releases with git-cliff — #152

- **What:** Adds [`cliff.toml`](cliff.toml) configuring git-cliff for
  conventional commits, a [`CHANGELOG.md`](CHANGELOG.md) generated from the
  full existing history, and [`.github/workflows/release.yml`](.github/workflows/release.yml)
  gains a release job that runs on `v*` tags and creates a GitHub Release
  whose body is the changelog section for that tag.
- **Verified:** the generated changelog covers all existing history and the
  workflow runs green on the PR; all commands and links in the new docs were
  checked.
- **Closes:** #152
- **Status:** In review.

### Contract — fix build: `DataKey::Reviewer` and ecosystem length check — #277

- **What:** `contracts/deprader_router/src/lib.rs` gains the
  `DataKey::Reviewer(Address)` storage key plus admin functions to grant and
  revoke the reviewer role, and stores `ecosystem` as a `String` so
  `check_metadata` can validate its length (a `Symbol` has no `.len()`).
  `contracts/deprader_router/src/test.rs` covers granting a reviewer and
  rejecting an over-long ecosystem.
- **Verified:** `cargo build -p deprader-router --target wasm32v1-none
  --release` succeeds; `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass; contract snapshots updated.
- **Closes:** #277
- **Status:** In review.

### Build fix — remove pasted chat text from `src/pipeline.rs` — #270

- **What:** Deletes the assistant chat text ("Looking at this task, I need
  to…") and the stray ```` ```rust ```` / ```` ``` ```` fences that preceded
  the `//!` module doc comment in [`src/pipeline.rs`](src/pipeline.rs), so
  the file parses again. The rest of the file was checked against the merged
  PRs and still contains the deprecation and license checks as intended.
- **Verified:** `cargo build` gets past `src/pipeline.rs` with no parse
  errors; `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #270
- **Status:** In review.

### Build fix — restore the `match` in `run_scan` — #271

- **What:** Restores the `match` over the scan result in
  [`src/bin/depradar.rs`](src/bin/depradar.rs) that a bad merge had left as a
  dangling `Err(message) =>` arm after the exit-code `if/else`. `Ok` prints
  the report and `Err` prints `error: …` and exits 2.
- **Tests:** `tests/cli.rs` covers `depradar scan` on a missing file,
  asserting the `error: …` output and exit code 2.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #271
- **Status:** In review.

### Build fix — remove conflict marker in `src/registry/npm.rs` — #272

- **What:** Deletes the bare `=======` left by an unresolved merge conflict
  in [`src/registry/npm.rs`](src/registry/npm.rs), keeping both sides: the
  `deprecated` / `yanked` fields and the `NpmDocument` type.
- **Verified:** the file compiles and its existing tests pass; `cargo fmt`,
  `cargo clippy -- -D warnings` and `cargo test --workspace` pass.
- **Closes:** #272
- **Status:** In review.

### Formatting — `cargo fmt --all` on contract and typosquat modules — #278

- **What:** Applies `cargo fmt --all` to
  `contracts/deprader_router/src/lib.rs`, `test.rs`,
  [`src/typosquat/dataset.rs`](src/typosquat/dataset.rs) and
  [`src/typosquat/distance.rs`](src/typosquat/distance.rs). Formatting only —
  no behavior changes.
- **Verified:** `cargo fmt --all -- --check` passes; `cargo fmt`,
  `cargo clippy -- -D warnings` and `cargo test --workspace` pass.
- **Closes:** #278
- **Status:** In review.

### Frontend — render `proof_ref` and `issue_ref` as links — #140

- **What:** Adds `refToUrl(ref)` to
  [`frontend/src/lib/format.ts`](frontend/src/lib/format.ts): `https://`
  URLs are returned as-is, `owner/repo#N` refs become
  `https://github.com/owner/repo/issues/N`, and anything else (including
  `javascript:` and other non-`https` schemes) returns `null` so it stays
  plain text. The bounty detail page and the bounty list use it to render
  refs as links that open in a new tab with `rel="noreferrer"`.
- **Verified:** Unit tests cover `https://` URLs, GitHub refs, and
  `javascript:` URLs (which must not become links). `npm run lint`,
  `npm run typecheck`, `npm test` and `npm run build` pass in `frontend/`.
- **By:** ALLEN-AYODEJI
- **Closes:** #140
- **Status:** In review.

### Frontend — Playwright smoke test against testnet — #141

- **What:** Adds Playwright under [`frontend/e2e/`](frontend/e2e/) with one
  smoke test: it opens `/`, waits for at least one bounty row, opens it, and
  checks that its claims render. A separate, non-blocking `e2e` job in
  [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs it against the
  live testnet contract (`continue-on-error: true`), since testnet can be
  flaky.
- **Verified:** The e2e job passes on the PR; `npm run lint`,
  `npm run typecheck`, `npm test` and `npm run build` pass in `frontend/`.
- **By:** ALLEN-AYODEJI
- **Closes:** #141
- **Status:** In review.

### Frontend — scan-report viewer — #142

- **What:** Adds `/report`, a page where a `depradar --format json` file can
  be dropped or uploaded and rendered entirely client-side: the summary plus
  findings grouped by severity. The JSON shape is validated and a clear
  error is shown for wrong files. Implemented in
  [`frontend/src/pages/Report.tsx`](frontend/src/pages/Report.tsx).
- **Verified:** Component tests cover a valid file and an invalid one.
  `npm run lint`, `npm run typecheck`, `npm test` and `npm run build` pass
  in `frontend/`.
- **By:** ALLEN-AYODEJI
- **Closes:** #142
- **Status:** In review.

### Client — `--json` and `--all` flags — #143

- **What:** [`client/src/index.ts`](client/src/index.ts) gains `--all`,
  which prints every bounty using the contract's `bounty_count` getter, and
  `--json`, which prints a single JSON document instead of the
  human-readable output. [`client/README.md`](client/README.md) documents
  both flags with output examples.
- **Verified:** `npm run typecheck` passes in `client/`; output examples for
  both flags are in the PR.
- **By:** ALLEN-AYODEJI
- **Closes:** #143
- **Status:** In review.

### Fix — `src/license.rs` imports a non-existent `crate::finding` module — #273

- **What:** `src/license.rs` imported `Finding`, `FindingKind` and
  `Severity` from `crate::finding`, which doesn't exist. The imports now
  point at the real modules — `Finding` / `FindingKind` from
  [`src/model.rs`](src/model.rs) and `Severity` from
  [`src/cve/severity.rs`](src/cve/severity.rs) — and the license checks
  build findings with the existing types.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass, including the `license` module tests.
- **Closes:** #273
- **Status:** In review.

### Fix — `src/registry/crates.rs` imports `PoliteClient` from the wrong module — #274

- **What:** `src/registry/crates.rs` imported `PoliteClient` from
  `crate::registry`, but it is defined in [`src/net.rs`](src/net.rs). The
  import is now `crate::net::PoliteClient`, and the follow-on type errors in
  the file are fixed.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass, including the `registry::crates` tests.
- **Closes:** #274
- **Status:** In review.

### Fix — add the missing `flate2` dependency — #275

- **What:** [`src/behavioral/tarball.rs`](src/behavioral/tarball.rs) uses
  `flate2::read::GzDecoder`, but `flate2` wasn't in `Cargo.toml`, so the
  build failed. `flate2` is now a `[dependencies]` entry and `Cargo.lock`
  is updated.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass, including the `behavioral::tarball` tests;
  `cargo deny check licenses bans sources` still passes.
- **Closes:** #275
- **Status:** In review.

### Fix — `depradar explain` uses a non-existent `deprader::osv` module — #276

- **What:** `run_explain`, `explain_text` and `explain_json` in
  [`src/bin/depradar.rs`](src/bin/depradar.rs) referenced
  `deprader::osv::Vuln`, which doesn't exist. They now use the real types
  from [`src/cve/osv_client.rs`](src/cve/osv_client.rs) (re-exported via
  [`src/cve/mod.rs`](src/cve/mod.rs)) — `OsvVulnerability` and its nested
  structs — so `depradar explain GHSA-…` prints the summary, severity and
  fixed version in both text and JSON.
- **Tests:** An `assert_cmd` integration test in [`tests/cli.rs`](tests/cli.rs)
  runs `depradar explain` against a mock OSV server and checks the printed
  summary, severity and fixed version in text and JSON output.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #276
- **Status:** In review.

### Read client — unit tests for argument parsing and output — #144

- **What:** Adds Vitest to `client/` and extracts the CLI's argument
  parsing and output formatting out of `client/src/index.ts` into testable
  functions. `parseArgs` is covered for the defaults (`--bounty-id` and
  `--claim-id` both `1`, plus the default contract and RPC URL) and for
  invalid IDs (non-numeric and negative values are rejected). The formatter
  is covered for the found and not-found paths of both `get_bounty` and
  `get_claim`. `npm test` runs the suite and is wired into the existing
  `client` CI job.
- **Verified:** `npm test` and `npm run typecheck` pass in `client/`; the
  workflow runs green on the PR.
- **By:** ALLEN-AYODEJI
- **Closes:** #144
- **Status:** In review.

### Drips — fund the bounty pool from a Drips stream — #149

- **What:** Prototypes a pool address in `contracts/` that receives Drips
  funding and lets the admin open bounties from the pool balance, with the
  limitations and next steps written up in
  [`docs/drips-integration.md`](docs/drips-integration.md).
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass; contract test snapshots are updated and the
  wasm build succeeds. A testnet demo with transaction links is recorded in
  the PR, and every command and link in the new docs was checked.
- **By:** ALLEN-AYODEJI
- **Closes:** #149
- **Status:** In review.

### CI — CodeQL analysis for TypeScript — #153

- **What:** Adds [`.github/workflows/codeql.yml`](.github/workflows/codeql.yml)
  running CodeQL's `javascript-typescript` analysis over `client/` and
  `frontend/` on push, on pull requests and on a weekly schedule.
- **Verified:** the workflow runs green on the PR.
- **By:** ALLEN-AYODEJI
- **Closes:** #153
- **Status:** In review.

### Dev container for one-click setup — #154

- **What:** Adds [`.devcontainer/devcontainer.json`](.devcontainer/devcontainer.json)
  with Rust stable plus the `wasm32v1-none` target, Node 22 and the Stellar
  CLI. Its `postCreateCommand` runs `cargo fetch` and `npm ci` in
  `frontend/` and `client/`, and `CONTRIBUTING.md` documents the setup.
- **Verified:** opening the repo in GitHub Codespaces lets you run
  `cargo test` and `npm test`; every command and link in the new docs was
  checked.
- **By:** ALLEN-AYODEJI
- **Closes:** #154
- **Status:** In review.

### Typosquat — grow the npm dataset to the top 5,000 packages — #338

- **What:** `data/popular-packages/npm-top1000.json` is regenerated as the
  top 5,000 npm packages by downloads (the file keeps its historical name so
  the loader and validation tests are unchanged), and
  `scripts/update-datasets.py` now fetches 5,000 npm names instead of 1,000.
  Mid-popularity squats that previously fell outside the reference list are
  now scored.
- **Benchmarks:** `benches/typosquat.rs` `score_package` numbers before/after
  are recorded in the PR description; no benchmark regressed by more than
  20% (the BK-tree lookup keeps scoring sub-linear in dataset size).
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass, including the dataset validation test over
  the enlarged file.
- **Closes:** #338
- **Status:** In review.

### Typosquat — detect ecosystem affixes (`py-`, `-js`, `node-`, `-rs`) — #340

- **What:** [`src/typosquat/combosquat.rs`](src/typosquat/combosquat.rs)
  strips common per-ecosystem affixes (`py-`/`-py` for PyPI, `node-`/`-js`
  for npm, `-rs`/`rs-` for crates.io) and compares the stripped name to the
  dataset, so names like `requests-py`, `pyrequests` or `node-lodash` are
  caught. A dedicated `affix_match` signal is reported on `ScanResult` and
  contributes to the confidence score.
- **Tests:** at least three affixes per ecosystem are covered, plus
  allowlisted real packages (e.g. `node-fetch`, `pyyaml`) that must not be
  flagged.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #340
- **Status:** In review.

### Typosquat — fail CI when a popular-package dataset is stale — #344

- **What:** Each file in `data/popular-packages/` now carries a
  `generated_at` date, and [`src/typosquat/dataset.rs`](src/typosquat/dataset.rs)
  gains a test that fails when a dataset's `generated_at` is older than 180
  days. `scripts/update-datasets.py` writes the date on every regeneration.
- **Tests:** the staleness test fails on an old date and passes on today's.
- **Verified:** the workflow runs green on the PR; `cargo fmt`,
  `cargo clippy -- -D warnings` and `cargo test --workspace` pass.
- **Closes:** #344
- **Status:** In review.

### CVE — treat OSV `MAL-` advisories as critical malicious-package findings — #345

- **What:** [`src/cve/`](src/cve/) detects OSV advisory IDs beginning with
  `MAL-` and maps them to `Severity::Critical` with a "malicious package"
  label, so known malware is reported distinctly from ordinary
  vulnerabilities. [`src/report/`](src/report/) renders the label in the text
  and JSON output.
- **Tests:** a mock-server test feeds a `MAL-` hit and asserts it renders as
  critical with the malware label.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #345
- **Status:** In review.

### CVE — Flag vulnerabilities in CISA's Known Exploited Vulnerabilities list — #346

- **What:** Adds `src/cve/kev.rs`, which downloads CISA's KEV JSON feed
  (`https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json`)
  and caches it on disk next to the OSV cache, refreshing it once the cached
  copy is older than a day. The feed's `cveID` values are collected into a
  set, and each finding's CVE aliases are checked against it; a match sets
  `known_exploited: true` on the finding. The flag is shown in the text
  report, in the JSON report (`known_exploited`) and in the SARIF result
  properties.
- **Tests:** A small KEV fixture (two CVEs) is served by a mock server, and
  a test asserts that the finding whose alias is in the feed is marked
  `known_exploited` while a finding with an unrelated CVE is not. A JSON
  output test checks the field appears in the report.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #346
- **Status:** In review.

### CVE — Add EPSS exploit-probability scores to findings — #347

- **What:** Adds `src/cve/epss.rs`, which queries the FIRST EPSS API
  (`https://api.first.org/data/v1/epss?cve=…`) for a finding's CVE aliases
  and stores the highest returned score on the finding as `epss_score`.
  Responses are cached with the OSV cache, keyed by the CVE ID, so repeated
  scans don't re-query the API. The score is printed in the text report,
  added to the JSON report (`epss_score`) and included in the SARIF result
  properties.
- **Tests:** A mock-server test serves a fixed EPSS response and asserts the
  score appears in the JSON output for the matching finding.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #347
- **Status:** In review.

### CLI — `depradar cache info` and `depradar cache clear` — #349

- **What:** Adds a `cache` subcommand to [`src/bin/depradar.rs`](src/bin/depradar.rs).
  `depradar cache info` prints the OSV cache directory path, the number of
  cached entries and their total size; `depradar cache clear` deletes the
  cache directory. Both use the path helpers in
  [`src/cve/cache.rs`](src/cve/cache.rs), and the cache directory can be
  overridden (e.g. for tests) so the real OS cache is never touched.
- **Tests:** `assert_cmd` CLI tests in [`tests/cli.rs`](tests/cli.rs) run
  both commands against a temporary cache directory: `cache info` reports
  the path and entry count, and `cache clear` removes the directory.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #349
- **Status:** In review.

### CVE — Suggest the smallest non-breaking upgrade — #351

- **What:** [`src/cve/remediation.rs`](src/cve/remediation.rs) now prefers the
  lowest fixed version within the installed major version when an advisory
  lists several fixed versions, so a `lodash@4.17.4` finding suggests
  `4.17.21` rather than `5.0.0`. When a fix exists in a later major, the
  major-version upgrade is reported as an alternative alongside the
  in-major fix. [`src/report/`](src/report/) renders the primary suggestion
  and the alternative in the text and JSON output.
- **Tests:** A remediation test uses an advisory fixed in two majors and
  asserts the in-major fix is chosen with the major bump listed as the
  alternative; a report test checks both appear in the output.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #351
- **Status:** In review.

### CVE — Show advisory publish date and age — #352

- **What:** [`src/cve/osv_client.rs`](src/cve/osv_client.rs) reads the
  `published` timestamp from the OSV response into the vulnerability model.
  [`src/report/text.rs`](src/report/text.rs) prints "published N days ago"
  for each finding, and the JSON report includes the raw `published` date.
- **Tests:** A snapshot test covers a report whose finding carries a
  `published` date, including the new text line and the JSON field.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #352
- **Status:** In review.

### Behavioral — `depradar scan --deep` runs behavioral checks on each dependency — #353

- **What:** Adds a `--deep` flag to `depradar scan`
  ([`src/bin/depradar.rs`](src/bin/depradar.rs)). With it,
  [`src/pipeline.rs`](src/pipeline.rs) downloads each npm dependency's
  tarball and runs the detectors in [`src/behavioral/`](src/behavioral/), so
  install scripts and obfuscation in dependencies are reported. Downloads go
  through the existing polite HTTP client and are limited to a small number
  of concurrent fetches.
- **Tests:** A mock-registry test serves a dependency whose tarball contains
  a malicious `postinstall` script and asserts `scan --deep` reports it.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #353
- **Status:** In review.

### CVE — PEP 440 version comparison for PyPI fixed versions — #350

- **What:** Adds PEP 440 ordering to [`src/cve/remediation.rs`](src/cve/remediation.rs)
  so PyPI fixed-version suggestions compare versions correctly instead of as
  semver. The comparison handles epochs (`1!2.0`), pre-releases
  (`2.0rc1`, `2.0a1`, `2.0b2`), post-releases (`1.0.post1`) and dev releases,
  and normalises the common spellings (`rc`, `c`, `pre`, `preview`, `-`/`_`
  separators). PyPI remediation uses this ordering; other ecosystems keep
  semver.
- **Tests:** Unit tests cover pre-release ordering (`2.0rc1 < 2.0`), post-release
  ordering (`1.0 < 1.0.post1`) and epoch ordering (`1!1.0 > 2.0`), plus the
  fixed-version suggestion picking the lowest fixed version above the
  installed one.
- **Verified:** `cargo fmt`, `cargo clippy -- -D warnings` and
  `cargo test --workspace` pass.
- **Closes:** #350
- **Status:** In review.
- **Status:** In review.

- #310 — Scan checked-out repositories without executing their contents and encrypt nightly ecosystem reports for the disclosure team (@willy-de7)
- #311 — Render and serve clean, findings, or stale README badges from repository scan reports (@willy-de7)
- #312 — Open testnet bounties from confirmed findings and persist the SHA-256 report proof on the router (@willy-de7)
- #313 — Record private notification and enforce the disclosure embargo for report and bounty publication (@willy-de7)
- #314 — Fetch signed community indicators and verify the exact rule bytes before scanning (@willy-de7)

### Typosquat — Maven Central popular-package dataset — #337

- **What:** Adds `data/popular-packages/maven-top1000.json`, the 1000
  most-depended-on Maven Central artifacts ranked by deps.dev dependent
  counts, written as `group:artifact` coordinates in rank order.
  `scripts/update-datasets.py` gains a `maven` target that rebuilds the file
  from deps.dev, and `typosquat::dataset` loads it for the Maven ecosystem
  alongside the npm, PyPI, crates.io and RubyGems lists, so Maven
  coordinates are scored and ranked like the others. The dataset README
  documents the source and snapshot date.
- **Tests:** the dataset validation test now covers the Maven file (unique,
  trimmed, lowercase, valid `group:artifact` coordinates); the loader test
  checks `org.springframework:spring-core` and `com.google.guava:guava`;
  `maven_typosquat_is_flagged` in `src/typosquat/scanner.rs` flags
  `com.google.guava:guav` as one edit from `com.google.guava:guava`.
- **By:** cyfer-codes
- **Closes:** #337
- **Status:** In review.

### Typosquat — project-level allowlist in `depradar.toml` — #342

- **What:** `depradar.toml` accepts `typosquat.allow = ["name", ...]`
  (`ProjectConfig::typosquat_allow`). At scan time the scanner merges it
  with the built-in `data/allowlist.json` entries, so a project can silence
  a known-good near-collision without editing DepRadar itself. Matching is
  case-insensitive and applies to the package name as written in the
  lockfile.
- **Tests:** a config parse test in `src/config.rs` for the new
  `typosquat.allow` key, and a scanner test in `src/typosquat/scanner.rs`
  showing an allowed name produces no typosquat finding while the same name
  without the allowlist entry is still flagged.
- **By:** cyfer-codes
- **Closes:** #342
- **Status:** In review.

### Typosquat — top-N matches and confidence from `typosquat-wasm` — #343

- **What:** `crates/typosquat-wasm` now returns
  `matches: [{name, distance, confidence}]` from its wasm API instead of a
  single best match, built on `nearest_matches` and the existing confidence
  score, so the dashboard can show alternatives and a score.
  `frontend/src/lib/typosquat.ts` gains the matching `TyposquatMatch` type
  and the updated result shape.
- **Tests:** `crates/typosquat-wasm/tests/detect.rs` covers the new result
  shape, including a name with several near matches and a clean name with an
  empty `matches` array.
- **By:** cyfer-codes
- **Closes:** #343
- **Status:** In review.

### CVE — offline mode using a downloaded OSV database — #348

- **What:** `depradar db update` downloads the per-ecosystem OSV zip dumps
  and indexes them locally, and `depradar scan --offline-db` makes `scan`
  query that local index instead of osv.dev, so air-gapped CI can run
  without network access. The `src/cve/` module gains the local index and
  the offline lookup path; the online client is unchanged.
- **Tests:** a fixture scan run against the local DB produces the same
  findings as the mocked OSV API, in `tests/cli.rs`.
- **By:** cyfer-codes
- **Closes:** #348
- **Status:** In review.

### Docs — Glossary of domain terms — #450

- **What:** `docs/glossary.md` defines the vocabulary the code and findings use
  without explaining: typosquat, combosquat, homoglyph, Damerau-Levenshtein
  distance, ecosystem-scoped scoring, `lockfile_ast`, `finding_kind`,
  `suppression_rule`, `proof_ref`, `review_window` and the PROGRESS.md
  requirement. Every entry carries a link to the file and symbol that
  implements it, so the glossary doubles as a map into `src/`. Linked from the
  README Documentation section.
- **Tests:** Not applicable — documentation only. All internal links and
  referenced paths were checked against the tree (`src/typosquat/scanner.rs`,
  `src/typosquat/combosquat.rs`, `src/bin/depradar.rs`,
  `src/bin/depradar/repo_commands.rs`, `src/pipeline.rs`,
  `docs/issue-23-damerau-levenshtein.md`,
  `docs/issue-24-ecosystem-scoped-scoring.md`,
  `docs/issue-25-pypi-name-normalization.md` all resolve).
- **By:** Qaz1853508
- **Closes:** #450
- **Status:** In review.
