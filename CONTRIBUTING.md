# Contributing

Thanks for helping make DepRadar better. Start with an open issue labelled
`good first issue` or `help wanted`, comment that you're picking it up, and
open a PR that references it.

## Picking an Issue

Browse the [open issues](https://github.com/DepRadarOrg/DepRadar/issues) and look for the labels matching your focus:

- **`Stellar Wave`**: Issues tracked and eligible for bounty payouts under the Drips Stellar Wave program. Make sure to include your Stellar public key in your PR description.
- **`good first issue`**: Well-scoped tasks with clear definitions of done, ideal for your first contribution to DepRadar.
- **`help wanted`**: Open tasks where community implementation or feedback is welcomed.
- **Area labels** (`contract`, `frontend`, `client`, `scanner`, `typosquat`, `cve`, `cli`): Group issues by codebase layer so you can filter for your preferred stack.

Before starting work, leave a comment on the issue stating that you are working on it to avoid duplicated efforts.

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

## Review Process

### Response Times
Maintainers typically review pull requests within **24–48 hours** on business days. If a PR has not received comments after 3 business days, a polite reminder comment on the PR thread is welcome.

### Reviewer Checklist
Every pull request is reviewed against the following criteria before approval:
1. **CI Status**: All GitHub Actions workflows (`test`, `fmt`, `clippy`, `wasm`, `client` typecheck, and `frontend` CI) pass with 0 errors and 0 warnings.
2. **Issue Linking**: The PR description references the related issue using GitHub keywords (e.g. `Closes #123`).
3. **Progress Tracking**: An accurate entry is included in [PROGRESS.md](PROGRESS.md) adhering to the repository's log format.
4. **Focused Diff**: The changeset remains focused on the issue's requirements without bundling unrelated formatting or dependency changes.
5. **Testing**: New functionality, edge cases, and bug fixes are backed by appropriate unit, component, or contract tests.
6. **Documentation**: Any newly introduced flags, configuration options, endpoints, or UI states are documented in the corresponding README.
7. **Snapshot Consistency**: Contract changes include updated snapshots in `contracts/deprader_router/test_snapshots/`.

### Common Reasons for Requested Changes
- **Missing PROGRESS.md entry**: All merged contributions must be logged in `PROGRESS.md`.
- **Failing CI or Unresolved Lints**: Clippy warnings or formatting differences fail the build; run `cargo fmt --all -- --check` and `cargo clippy` locally prior to push.
- **Unrelated Changes**: Incidental edits to package locks, unrelated files, or scope creep should be removed or moved to dedicated PRs.
- **Missing Tests**: Non-trivial changes or bug fixes submitted without test coverage.
- **Secret Exposure**: Unintended commits containing keys or tokens.

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
