# Test fixtures

Inputs for the lockfile parser and scanner tests. `tests/fixtures.rs` checks
that every file here is listed below and is under 200 KB.

## Real-world lockfiles

Copied unchanged from open-source projects, one directory per ecosystem, so
the parsers are tested against what package managers actually write. Don't
edit them by hand. To refresh one, copy the new upstream file over it and
update its source link here.

| Fixture | Format | Source | License |
| --- | --- | --- | --- |
| [`npm/package-lock.json`](npm/package-lock.json) | `package-lock.json` v3 | [tj/commander.js @ v15.0.0](https://github.com/tj/commander.js/blob/v15.0.0/package-lock.json) | [MIT](https://github.com/tj/commander.js/blob/v15.0.0/LICENSE) |
| [`npm/package.json`](npm/package.json) | `package.json` | [tj/commander.js @ v15.0.0](https://github.com/tj/commander.js/blob/v15.0.0/package.json) | [MIT](https://github.com/tj/commander.js/blob/v15.0.0/LICENSE) |
| [`pypi/requirements-dev.txt`](pypi/requirements-dev.txt) | `requirements*.txt` | [psf/requests @ 16df2a0](https://github.com/psf/requests/blob/16df2a09173b17a82d607c31a6826b7c5dd0fc57/requirements-dev.txt) | [Apache-2.0](https://github.com/psf/requests/blob/16df2a09173b17a82d607c31a6826b7c5dd0fc57/LICENSE) |
| [`pypi/Pipfile.lock`](pypi/Pipfile.lock) | `Pipfile.lock` (spec 6) | [pypa/pipenv @ 899c465](https://github.com/pypa/pipenv/blob/899c465927e8e5ecc40eab6109b1c8e419f6cc8c/Pipfile.lock) | [MIT](https://github.com/pypa/pipenv/blob/899c465927e8e5ecc40eab6109b1c8e419f6cc8c/LICENSE) |
| [`cargo/Cargo.lock`](cargo/Cargo.lock) | `Cargo.lock` v4 | [sharkdp/hyperfine @ v2.0.0](https://github.com/sharkdp/hyperfine/blob/v2.0.0/Cargo.lock) | [MIT](https://github.com/sharkdp/hyperfine/blob/v2.0.0/LICENSE-MIT) / [Apache-2.0](https://github.com/sharkdp/hyperfine/blob/v2.0.0/LICENSE-APACHE) |

To check that a fixture still matches its source, compare it with the raw
upstream file:

```sh
curl -sL https://raw.githubusercontent.com/tj/commander.js/v15.0.0/package-lock.json \
  | cmp - tests/fixtures/npm/package-lock.json
```

## Hand-written fixtures

Small synthetic files written for this repository's tests. They contain
deliberate typosquats (`expresss`, `reqeusts`) and edge cases, and are
covered by the repository's [MIT license](../../LICENSE).

| Fixture | Format | Used by |
| --- | --- | --- |
| [`npm-basic/package-lock.json`](npm-basic/package-lock.json) | `package-lock.json` v3 | `tests/lockfile.rs`, `tests/scan_lockfile.rs` |
| [`npm-chain/package-lock.json`](npm-chain/package-lock.json) | `package-lock.json` v3 | `tests/lockfile.rs`, `tests/scan_lockfile.rs` |
| [`package-lock-v1.json`](package-lock-v1.json) | `package-lock.json` v1 | not referenced by a test yet |
| [`package-lock-v3.json`](package-lock-v3.json) | `package-lock.json` v3 | not referenced by a test yet |
| [`package.json`](package.json) | `package.json` | `src/lockfile/mod.rs`, `src/lockfile/npm.rs` |
| [`pypi-basic/requirements.txt`](pypi-basic/requirements.txt) | `requirements.txt` | `tests/lockfile.rs` |
| [`Pipfile.lock`](Pipfile.lock) | `Pipfile.lock` | `src/lockfile/mod.rs`, `src/lockfile/pypi.rs` |
