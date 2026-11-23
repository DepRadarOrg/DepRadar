---
name: Bug report
about: Something in the contract, scanner, frontend, or client behaves incorrectly
title: "[bug] "
labels: bug
---

## What happened

<!-- What command or interaction did you run, and what went wrong? Include terminal output or error logs if available. -->

## Expected behavior

<!-- What did you expect to happen instead? -->

## Minimal Reproduction Steps

1.
2.
3.

<!-- If applicable, provide a minimal lockfile, package name, or Soroban CLI invocation that reproduces the issue. -->

## Component

- [ ] `contracts/deprader_router` (Soroban contract)
- [ ] Typosquat scanner (`src/typosquat/`)
- [ ] CVE matching (`src/cve/`)
- [ ] Behavioral analysis (`src/behavioral/`)
- [ ] Lockfile parsers (`src/lockfile/`)
- [ ] Frontend dashboard (`frontend/`)
- [ ] Testnet read client (`client/`)
- [ ] CI / tooling

## Environment

- Commit / Version:
- OS:
- `rustc --version` / `node --version`:

> **Security issue?** Don't file it here — see [SECURITY.md](../../SECURITY.md).
