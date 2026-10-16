# Frequently Asked Questions (FAQ)

This document answers common questions about DepRadar's architecture, bounty lifecycle, security model, and development workflow.

---

## General & Network

### 1. Is DepRadar running on Stellar mainnet or testnet?
DepRadar is currently pre-1.0 and deployed exclusively on the **Stellar Testnet**.

- **Active Router Contract ID:** [`CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`](https://stellar.expert/explorer/testnet/contract/CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ)
- **Token:** Native testnet XLM via the Stellar Asset Contract (SAC) or custom testnet Soroban tokens.

Testnet tokens carry no monetary value, but all contracts, state transitions, events, and authorizations run against live Soroban nodes exactly as they will on mainnet.

### 2. What is DepRadar's core mission?
DepRadar crawls open-source package registries, scans dependency graphs for vulnerabilities (CVEs, typosquatting, and behavioral attacks), and pays out bounties via decentralized smart contracts and [Drips](https://www.drips.network/) to the researcher who catches the issue first. It removes centralized platforms and foundations from between finders and payouts.

---

## Findings & Detection

### 3. What counts as a valid security finding?
A finding is an identified supply-chain risk or vulnerability in an open-source package:
- **Known CVEs:** Vulnerable package releases identified via the OSV API client (`src/cve/osv_client.rs`).
- **Typosquats:** Deceptive packages impersonating popular dependencies, detected via Levenshtein distance and Unicode homoglyph normalization (`src/typosquat/`).
- **Behavioral Anomalies:** Suspicious install-time hooks, obfuscation, or unauthorized permission requests (planned crawler/scanner modules).

Each finding is represented by an `issue_ref` (up to 256 bytes) pointing to the package, advisory, or vulnerability report.

### 4. How do I report a security vulnerability in DepRadar itself?
Do **not** open a public GitHub issue. Report security issues in DepRadar's code privately via GitHub's private vulnerability reporting:
1. Navigate to the **Security** tab of the DepRadar repository.
2. Click **Report a vulnerability**.
3. Provide reproduction steps, affected commit, and impact.

See [SECURITY.md](../SECURITY.md) for full response SLA and disclosure policy.

### 5. What should I do if I find a malicious package in a public registry?
If you discover malware or an active typosquat on a public registry (such as npm, PyPI, or crates.io), report it directly to the affected registry's security team first:
- [npm Malware Reporting](https://docs.npmjs.com/reporting-malware-in-an-npm-package)
- [PyPI Security](https://pypi.org/security/)
- [crates.io Security Policy](https://crates.io/policies/security)

Once reported upstream, you can open an issue or claim a bounty on DepRadar so our automated scanners can verify detection against the malicious artifact.

---

## Bounties & Claims

### 6. Who decides whether a bounty claim is approved?
The contract **admin** (initialized during contract deployment) reviews submitted claims. When a claim is verified:
- The admin executes `release(bounty_id, claim_id)`.
- The contract marks the claim as `Accepted`, updates the bounty status to `Released`, and directly transfers the locked funds from the contract to the claimant's Stellar address.
- If a claim is invalid or duplicate, the admin executes `reject_claim(bounty_id, claim_id)`.

### 7. Can multiple researchers claim the same bounty?
**Yes.** The Bounty Router contract supports up to **50 claims per bounty** (`MAX_CLAIMS_PER_BOUNTY = 50`), with a limit of one claim per unique claimant address (`DataKey::ClaimBy`).

This multi-claim architecture guarantees that a junk or front-running claim cannot block legitimate researchers from submitting proof for the same issue.

### 8. What happens when a junk or invalid claim is submitted?
Submitting a claim (`submit_claim`) moves zero funds and does not close the bounty. When the admin evaluates a bad claim:
1. The admin calls `reject_claim(bounty_id, claim_id)`.
2. The claim's status transitions to `Rejected`.
3. The bounty's `pending_claims` count decreases by 1.
4. The bounty remains `Open` for any other pending or future claims until its deadline.

### 9. Can a creator cancel an open bounty and reclaim their funds?
Yes, subject to rules enforced by `cancel_bounty`:
- **Admin cancellation:** The admin can cancel an open bounty at any time (e.g. if an issue reference was flawed or retracted).
- **Creator cancellation:** The creator may cancel their bounty once either:
  1. The bounty's `deadline` has passed **and** there are `0` pending claims awaiting review, or
  2. The bounty's deadline plus the **7-day review window** (`REVIEW_WINDOW_SECS = 604,800`) has passed.

This prevents creators from pulling funds out from under researchers whose claims are under active review, while guaranteeing funds never remain locked indefinitely.

### 10. What are the limits on bounty duration and reference sizes?
The router contract enforces the following constraints:
- **Maximum Bounty Duration:** 365 days (`MAX_BOUNTY_DURATION_SECS = 31,536,000`).
- **Maximum Reference String Length:** 256 bytes for both `issue_ref` and `proof_ref` (`MAX_REF_LEN = 256`).
- **Maximum Claims per Bounty:** 50 claims (`MAX_CLAIMS_PER_BOUNTY = 50`).
- **Review Window:** 7 days following the deadline (`REVIEW_WINDOW_SECS = 604,800`).

---

## Architecture & Governance

### 11. How does admin rotation work?
Admin authority is transferred using a two-step handshake:
1. The active admin calls `propose_admin(new_admin)`.
2. The nominated successor calls `accept_admin()`, requiring their cryptographic signature.

This two-step process prevents catastrophic loss of contract control due to an address typo.

### 12. How does Drips integrate with DepRadar?
[Drips](https://www.drips.network/) is planned as the continuous payout and distribution layer. While the Soroban router contract provides on-chain escrow, verification, and settlement, Drips enables splitting and streaming rewards between security researchers, project maintainers, and dependency upstream authors.

### 13. How can I inspect bounties without running a full node or connecting a wallet?
You can view public bounty data without holding private keys or connecting a wallet:
- **Web Dashboard:** Run the read-only React dashboard (`cd frontend && npm ci && npm run dev`).
- **CLI Read Client:** Run the testnet read client (`cd client && npm ci && npm start`).
- **Stellar Explorer:** Inspect contract events, storage, and transaction history at [StellarExpert](https://stellar.expert/explorer/testnet/contract/CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ).
