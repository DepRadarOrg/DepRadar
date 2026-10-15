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

## Labels

Issues and pull requests are organised with two kinds of labels: **area**
labels that point at the part of the codebase involved, and **workflow**
labels that describe triage state. Use one area label per issue; add more
only when a change genuinely spans several areas.

### Area labels

| Label | Use it for |
|---|---|
| `typosquat` | Typosquat detection (`src/typosquat/`) |
| `scanner` | Lockfile parsing and the scan pipeline |
| `crawler` | Registry crawlers and monitors |
| `cve` | Known-vulnerability matching (`src/cve/`) |
| `contract` | The Soroban bounty-router contract |
| `client` | Testnet read client (`client/`) |
| `frontend` | Web dashboard (`frontend/`) |
| `wallet` | Wallet-connected write actions |
| `cli` | The `depradar` command-line tool |
| `npm` | npm ecosystem |
| `pypi` | PyPI ecosystem |
| `crates` | crates.io ecosystem |
| `drips` | Drips integration |
| `data` | Datasets |
| `security` | Security hardening or detection |
| `performance` | Speed, size or resource usage |
| `accessibility` | Barrier affecting people with disabilities |
| `behavioral` | Behavioral / malware heuristics |
| `testing` | Tests, fixtures and test infrastructure |
| `ci` | CI, release and automation |
| `devex` | Developer experience and tooling |
| `tooling` | Scripts and tooling |
| `design` | Visual and UX design |

### Workflow labels

| Label | Meaning |
|---|---|
| `bug` | Something isn't working |
| `enhancement` | New feature or request |
| `documentation` | Improvements or additions to documentation |
| `question` | Further information is requested |
| `good first issue` | Good for newcomers — self-contained and low-risk |
| `help wanted` | Extra attention is needed |
| `duplicate` | This issue or pull request already exists |
| `invalid` | This doesn't seem right |
| `wontfix` | This will not be worked on |
| `Stellar Wave` | Eligible for the Drips Stellar Wave program |

`Stellar Wave` is applied by maintainers to issues that qualify for the
[Drips](https://www.drips.network) funding round — don't add it yourself,
but you may filter by it to find work that's eligible for a payout.

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
