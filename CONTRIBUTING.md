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

### Benchmarks and fuzzing

```bash
# Criterion benchmarks for the typosquat signals (benches/typosquat.rs)
cargo bench
cargo bench --no-run   # compile only; this is what CI runs

# Fuzz the typosquat scanner (needs nightly and `cargo install cargo-fuzz`)
cd fuzz
cargo +nightly fuzz run normalize_homoglyphs -- -max_total_time=600
cargo +nightly fuzz run score_package -- -max_total_time=600
```

Any crash input that `cargo fuzz` saves under `fuzz/artifacts/` should be
added as a regression test next to the code it broke.

Some tests in `src/cve/` call the live [OSV API](https://osv.dev); they skip
(rather than fail) when the network is unavailable.

## Labels

Every issue and PR carries at least one **area label** naming the part of
the repo it touches, plus any **status labels** that apply. Use this list
when triaging or when you're unsure where a change belongs.

### Area labels

| Label | Applies to | When it's used |
| --- | --- | --- |
| `typosquat` | `src/`, `benches/typosquat.rs`, `fuzz/` | Changes to the typosquat scanner: normalization, homoglyph handling, scoring signals, or the `scan` binary. |
| `contract` | `contracts/` | Changes to the `deprader_router` Soroban contract, its tests, or the committed `test_snapshots/`. |
| `frontend` | `frontend/` | Changes to the web dashboard (Vite app, components, CSS tokens). |
| `client` | `client/` | Changes to the testnet read client. |
| `cve` | `src/cve/` | Changes to the OSV-backed vulnerability lookup and its tests. |
| `docs` | `*.md`, `docs/` | Documentation-only changes: guides, the severity model, PROGRESS entries. |
| `ci` | `.github/`, build config | Workflow, lint, or build-pipeline changes. |

If a PR spans several areas, apply each relevant area label; the first one
listed should be the primary area.

### Status labels

| Label | When it's used |
| --- | --- |
| `good first issue` | Small, self-contained issues with clear acceptance criteria and no prior codebase knowledge required — a good entry point for new contributors. |
| `help wanted` | Issues the maintainers have scoped but aren't actively working on; outside contributions are explicitly welcome. |
| `Stellar Wave` | Issues included in a Stellar Wave contribution round. Comment on the issue to be assigned before starting, and reference the issue with `Closes #N` in your PR. |

Comment on an issue before starting work so it can be assigned to you, and
make sure your PR references the issue it closes (see
[Pull Requests](#pull-requests)).

## Pull Requests
=======

- One issue per PR where possible, with `Closes #N` in the description.
- Keep CI green: fmt, clippy, tests, the wasm build, and the client
  typecheck all run on every PR.
- New detectors must rate their findings per the
  [severity model](docs/severity.md) and add an example there.
- If you change `contracts/`, commit the regenerated files under
  `contracts/deprader_router/test_snapshots/`.
- Security-sensitive findings go through [SECURITY.md](SECURITY.md), not
  public issues.

## Closing Issues

No issue is closed without an entry in [PROGRESS.md](PROGRESS.md)
describing what was built, by whom, and which issue it closes. This is
called out explicitly in the Definition of Done on the issue templates
(`.github/ISSUE_TEMPLATE/`), but it applies to any issue, templated or not.

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
