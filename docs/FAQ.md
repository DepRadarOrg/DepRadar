# Frequently Asked Questions (FAQ)

Answers to common questions about DepRadar, the on-chain Bounty Router contract, and how bounties and claims operate.

---

### 1. What is DepRadar?
DepRadar is an open-source decentralized dependency security and bounty network built on Stellar / Soroban. It pairs dependency vulnerability and typosquat scanners with an on-chain Bounty Router contract (`deprader_router`) that locks funds against findings and awards bounties directly to security researchers.

### 2. Is DepRadar live on Stellar Mainnet or Testnet?
DepRadar is currently live on **Stellar Testnet**. The Bounty Router contract is deployed and active at:  
[`CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`](https://stellar.expert/explorer/testnet/contract/CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ)

### 3. What counts as an eligible finding?
An eligible finding is any verifiably malicious, vulnerable, or deceptive package within supported package registries (such as npm, PyPI, or crates.io). Examples include:
- Typosquatting packages targeting popular libraries (detected via Levenshtein edit distance or homoglyph character impersonation).
- Pinned packages affected by known critical vulnerabilities registered in the [OSV](https://osv.dev) database.
- Packages exhibiting malicious behaviors (such as credential harvesting or malicious install scripts).

Findings are identified on-chain via an `issue_ref` string (e.g. `npm:expresss typosquat of express`).

### 4. Who decides whether a claim is accepted or rejected?
The designated contract **Admin** reviews claims. The admin is configured during contract deployment and initialization. The admin can:
- Call `release(bounty_id, claim_id)` to approve a claim and disburse locked funds to the claimant.
- Call `reject_claim(bounty_id, claim_id)` to mark an invalid or incorrect claim as rejected while keeping the bounty open for other claimants.

### 5. Can the contract admin be rotated or updated?
Yes. The contract implements a secure two-step admin transfer process:
1. The current admin calls `propose_admin(new_admin)`.
2. The new admin calls `accept_admin()`.

This ensures ownership cannot be accidentally assigned to an invalid or unauthenticated address.

### 6. What prevents front-running or squatting claims?
The contract allows up to 50 distinct claims per bounty (`MAX_CLAIMS_PER_BOUNTY = 50`), with one claim per unique claimant address. If an attacker rushes to submit a junk claim before the real researcher, the admin can simply reject the junk claim via `reject_claim`, leaving the bounty open to pay the legitimate researcher.

### 7. How are bounty funds locked and protected?
When a bounty is created with `create_bounty`, the specified token amount is transferred directly from the creator's address into the contract. The funds remain locked in escrow on-chain until either an admin authorizes payout to a verified claim via `release`, or the bounty is cancelled under protocol refund rules.

### 8. Under what conditions can a bounty creator cancel and claim a refund?
Bounties can be cancelled and refunded via `cancel_bounty`:
- **Admin cancellation**: The admin can cancel at any time to return funds to the creator.
- **Creator cancellation after deadline**: The creator can cancel once the deadline has passed if there are no pending claims.
- **Creator cancellation after review window**: If unreviewed pending claims exist, the creator can still cancel once the 7-day review window after the deadline has expired (`deadline + REVIEW_WINDOW_SECS`), ensuring creator funds cannot remain locked indefinitely.

### 9. What is the review window, and can claims be paid after the deadline?
The review window is 7 days (`REVIEW_WINDOW_SECS = 604,800 seconds` / 7 days). While claims must be submitted *before* the bounty deadline, the admin can execute `release` during or after the deadline. This allows maintainers adequate time to triage complex proof writeups before releasing funds.

### 10. Do I need a funded Stellar account or wallet to use the dashboard?
No. The web dashboard (`frontend/`) and the read client (`client/`) perform read-only simulation calls against public Stellar RPC endpoints (`get_bounty`, `get_claim`, `bounty_count`). You do not need a wallet extension, Stellar secret keys, or funded testnet accounts to inspect bounties and claims.

### 11. What tokens can be used for bounties?
The contract works with any Soroban-compatible token or Stellar Asset Contract (SAC). The token address is passed into `create_bounty(creator, issue_ref, amount, token, deadline)`.

### 12. How do I report a security vulnerability in DepRadar itself?
Vulnerabilities found within DepRadar's own smart contract, scanners, or frontend should **never** be posted in public issues. Please follow the responsible disclosure process documented in [SECURITY.md](../SECURITY.md) via GitHub Private Vulnerability Reporting or by contacting the security team.
