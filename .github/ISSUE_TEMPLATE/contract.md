---
name: Contract feature or change
about: Propose or track changes to the Soroban deprader_router contract
title: "[contract] "
labels: contract
---

## Summary

Describe the proposed contract function, storage layout modification, or interface enhancement in `contracts/deprader_router/`.

## Interface & Methods

- **New / modified methods:** method name, signature, and parameters.
- **Storage keys:** what ledger entries are read, written, or extended (e.g. `DataKey::Bounty(id)`, `DataKey::Claim(id)`).
- **Error conditions:** what new `Error` variants should be emitted, and when do they revert.

## Security & Authorization

- Which caller identity is authenticated (`funder.require_auth()`, `admin.require_auth()`, `claimant.require_auth()`)?
- Are token transfers or escrowed funds involved?
- How is storage TTL managed for persistent entries?

## Definition of Done

- [ ] Implemented in `contracts/deprader_router/src/lib.rs`
- [ ] Comprehensive unit tests added to `contracts/deprader_router/src/test.rs`
- [ ] Budget and instruction limits verified
- [ ] Contract error codes documented in `docs/contract-errors.md` if new variants are introduced
- [ ] Added an entry to `PROGRESS.md` describing what was built, by whom, and which issue it closes
