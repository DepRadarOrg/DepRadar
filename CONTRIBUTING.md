# Contributing

Thanks for helping make DepRadar better. Start with an open issue labelled
`good first issue` or `help wanted`, comment that you're picking it up, and
open a PR that references it.

## Development Setup

Prerequisites: a stable Rust toolchain, the `wasm32v1-none` target (for the
contract), and Node.js 22+ (for `client/`).

```bash
rustup target add wasm32v1-none

# Off-chain toolkit + contract unit tests
cargo test --workspace --locked

# Lints — CI runs both and fails on any warning
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# Build the contract to wasm
cargo build -p deprader-router --target wasm32v1-none --release --locked

# Try the typosquat scanner
cargo run --bin scan -- expresss

# Testnet read client
cd client && npm ci && npm run typecheck && npm start

# Web dashboard (http://localhost:5173)
cd frontend && npm ci && npm run dev
npm run lint && npm run typecheck && npm test && npm run build
```

Frontend conventions (contract access only through `lib/contract.ts`,
loading/error/empty states everywhere, CSS tokens only) are in
[frontend/README.md](frontend/README.md).

Some tests in `src/cve/` call the live [OSV API](https://osv.dev); they skip
(rather than fail) when the network is unavailable.

## Pull Requests

- One issue per PR where possible, with `Closes #N` in the description.
- Keep CI green: fmt, clippy, tests, the wasm build, and the client
  typecheck all run on every PR.
- If you change `contracts/`, commit the regenerated files under
  `contracts/deprader_router/test_snapshots/`.
- Security-sensitive findings go through [SECURITY.md](SECURITY.md), not
  public issues.

## Closing Issues

No issue is closed without an entry in [PROGRESS.md](PROGRESS.md)
describing what was built, by whom, and which issue it closes. This is
called out explicitly in the Definition of Done on the issue templates
(`.github/ISSUE_TEMPLATE/`), but it applies to any issue, templated or not.

## Labels

Issues and pull requests use labels to categorize work by program, subsystem, ecosystem, and cross-cutting engineering concerns:

### Program & Onboarding
- `Stellar Wave`: Tasks eligible for bounties and tracking under the Drips Stellar Wave program.
- `good first issue`: Self-contained tasks with clear scope, recommended for newcomers.
- `help wanted`: Tasks needing community input or specific domain expertise.

### Subsystems & Modules
- `contract`: Soroban smart contract logic, tests, and storage in `contracts/deprader_router/`.
- `frontend`: Web dashboard UI, state management, and components in `frontend/`.
- `client`: Minimal testnet read client CLI in `client/`.
- `cli`: Core `depradar` command-line tool in `src/bin/`.
- `typosquat`: Package name similarity scoring, edit distance, and homoglyphs in `src/typosquat/`.
- `cve`: Known-vulnerability detection and OSV API client in `src/cve/`.
- `scanner`: Lockfile parsing and scan pipeline integration.
- `crawler`: Package registry crawlers and upstream update monitors.
- `behavioral`: Behavioral heuristics and package execution analysis.
- `wallet`: Wallet-connected transactions and signing interactions.
- `drips`: Payout streaming and Drips protocol distribution.
- `data`: Reference datasets in `data/popular-packages/`.

### Ecosystems
- `npm`: Node.js / npm package ecosystem.
- `pypi`: Python / PyPI package ecosystem.
- `crates`: Rust / crates.io package ecosystem.

### Engineering & Quality
- `ci`: CI workflows, automated checks, and release automation.
- `devex`: Tooling, developer configuration, and contributor workflow.
- `tooling`: Scripts and automation helpers.
- `testing`: Unit, integration, and contract tests, plus fixtures.
- `security`: Security hardening, threat models, and vulnerability fixes.
- `performance`: Execution speed, bundle size, gas/resource optimization.
- `design`: Visual design, theme design tokens, and UI layout.
- `accessibility`: WCAG conformance, keyboard navigation, and screen reader support.
- `documentation`: Guides, READMEs, architectural docs, and docstrings.
- `bug`: Defects breaking expected functionality.
- `enhancement`: New capabilities or improvements to existing functionality.

## Testnet Identities

Don't share or commit Stellar secret keys, including testnet ones. Generate
your own local identity instead:

```bash
stellar keys generate <name> --network testnet --fund
```

This creates a keypair under your local Stellar CLI config (see
`.gitignore` — `.stellar/`, `.soroban/`, and `**/identity/*.toml` are all
excluded from the repo) and funds it via friendbot. Use `<name>` wherever
these docs or scripts expect a `--source` or `--admin` identity.

The current `deprader_router` testnet deployment
(`CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`) uses a fresh
admin identity, replacing the one exposed by the original v1 deployment.
Testnet funds have no real value, but don't reuse identities across
testnet and mainnet — the contract supports `propose_admin` /
`accept_admin` precisely so the admin can be rotated without a redeploy.
