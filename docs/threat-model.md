# Threat Model for the DepRadar Bounty Router

This document analyzes the security architecture, trust boundaries, threat vectors, and mitigations for the `deprader_router` Soroban smart contract.

---

## 1. Overview & Trust Boundary

The Bounty Router (`contracts/deprader_router/`) acts as the decentralized escrow and trust boundary of the DepRadar ecosystem. Its core function is to lock creator funds against verifiable security findings, track researcher claims, and enforce payout or refund conditions without relying on off-chain databases.

```text
               +----------------------------------------+
               |        deprader_router Contract        |
+----------+   |                                        |   +------------+
| Creator  |-->| 1. create_bounty(issue_ref, amount)    |-->| SAC / Token|
+----------+   |    - Escrow locked                     |   | (Escrow)   |
               |                                        |   +------------+
+----------+   | 2. submit_claim(bounty_id, proof_ref)  |
|Claimants |-->|    - Up to 50 competing claims         |
+----------+   |                                        |
               | 3. reject_claim / release              |   +------------+
+----------+   |    - Admin review & payout             |-->| Winner     |
|  Admin   |-->|                                        |   | Claimant   |
+----------+   +----------------------------------------+   +------------+
```

---

## 2. Actors & Roles

| Actor | Capabilities | Trust Level |
|---|---|---|
| **Funder / Creator** | Calls `create_bounty` to lock funds; calls `cancel_bounty` to reclaim funds after deadline + review window. | Untrusted. |
| **Claimant / Researcher** | Calls `submit_claim` with proof reference (e.g. PR, CVE advisory, writeup URL). | Untrusted. |
| **Admin** | Calls `release` to disburse funds to a verified claimant; calls `reject_claim` on invalid claims; calls `cancel_bounty` anytime; proposes admin rotations. | Trusted to triage honestly and verify claims. |
| **Adversary** | Attempts claim front-running, spamming, fund draining, re-entrancy, or denial of service. | Hostile. |

---

## 3. Assets Protected

1. **Locked Token Balances:** Funds deposited into contract escrow for open bounties.
2. **Claim Integrity:** Record of who claimed which finding, when, and with what proof reference.
3. **State Continuity:** Preventing premature storage archival via Soroban TTL management.

---

## 4. Trust Assumptions

- **Admin Integrity:** The admin is assumed not to collude with malicious actors to release funds to false claims. Admin compromise is mitigated via rotation and auditability (see Threat 2).
- **Stellar / Soroban Core:** Cryptographic signature verification (`require_auth`) and native Stellar Asset Contract (SAC) transfers operate as specified by the protocol.
- **Monotonic Ledger Time:** Time-based rules (deadlines, review windows) rely on `env.ledger().timestamp()`.

---

## 5. Threat Vectors, Mitigations, and Status

### Threat 1: Claim Squatting / Front-Running
- **Attack Scenario:** An attacker observes a researcher's public report or mempool submission and rushes to call `submit_claim` first, hoping to block the legitimate finder.
- **Contract Mitigation:** **Multi-claim architecture.** Up to 50 claims (`MAX_CLAIMS_PER_BOUNTY = 50`) can be recorded for a single bounty. The contract does not grant funds based on arrival order. The admin can reject the squatted claim via `reject_claim`, keeping the bounty `Open` to pay the legitimate researcher.
- **Status:** ✅ Mitigated in v2 router.

### Threat 2: Admin Compromise & Rogue Takeover
- **Attack Scenario:** A compromised admin key immediately rotates ownership to an attacker address or drains funds.
- **Contract Mitigation:**
  - **Two-Step Admin Rotation:** Admin rotation requires `propose_admin` by current admin and an explicit `accept_admin` signed by the new admin.
  - **Constrained Release:** Funds can *only* be paid out to addresses that have previously submitted a valid claim against the specific bounty (`load_open_pending`). The admin cannot transfer funds to an arbitrary third party.
- **Open Improvements:** Future governance timelock or multi-signature requirement.
- **Status:** ✅ Mitigated.

### Threat 3: Re-entrancy and Token Manipulation
- **Attack Scenario:** An attacker specifies an adversarial token contract during `create_bounty` that attempts re-entrant calls into the router during transfer.
- **Contract Mitigation:** Strict **Checks-Effects-Interactions (CEI)** pattern. In `release`, claim status is set to `Accepted`, bounty status is set to `Released`, and storage is updated *before* calling `token::Client::transfer`.
- **Related Issue:** [#172 (Minimum bounty amount per token)](https://github.com/DepRadarOrg/DepRadar/issues/172) ensures bounties cannot be created with dusting amounts.
- **Status:** ✅ Mitigated.

### Threat 4: Griefing via Claim Spam
- **Attack Scenario:** A malicious actor spams dummy claims to exhaust the 50-claim cap and lock out legitimate researchers.
- **Contract Mitigation:**
  - One claim per claimant address per bounty enforced by the `ClaimBy(bounty_id, claimant)` persistent storage index.
  - An attacker must generate and fund separate Stellar addresses for each spam claim.
- **Related Issue:** [#171 (Reject self-claims by bounty creator)](https://github.com/DepRadarOrg/DepRadar/issues/171) prevents creators from polluting their own claim queue.
- **Status:** ✅ Mitigated; [#171](https://github.com/DepRadarOrg/DepRadar/issues/171) open for self-claim guard.

### Threat 5: Indefinite Fund Lockup (Creator Griefing)
- **Attack Scenario:** The admin goes offline, refuses to triage, or abandons the project, leaving creator deposits locked forever.
- **Contract Mitigation:**
  - If no claims were submitted: Creator can cancel immediately after the deadline passes (`cancel_bounty`).
  - If unreviewed claims exist: Creator can still cancel once the 7-day review window (`REVIEW_WINDOW_SECS = 604,800s`) has elapsed after the deadline.
- **Status:** ✅ Mitigated.

### Threat 6: Storage Expiration & Data Eviction
- **Attack Scenario:** Soroban persistent storage keys for active bounties reach their TTL threshold and are evicted from state.
- **Contract Mitigation:** The contract automatically extends the TTL of instance data and persistent keys (`INSTANCE_TTL_EXTEND_TO` = 30 days, `PERSISTENT_TTL_EXTEND_TO` = 90 days) during state-changing calls and updates.
- **Status:** ✅ Mitigated.

### Threat 7: Reference String Spoofing & Bloat
- **Attack Scenario:** Attackers submit unbounded payload strings in `issue_ref` or `proof_ref` to bloat contract storage or inject deceptive identifiers.
- **Contract Mitigation:** Input validation (`check_ref`) strictly limits string length to `MAX_REF_LEN = 256` bytes and rejects empty strings.
- **Related Issue:** [#162 (Agree on a standard issue_ref format for findings)](https://github.com/DepRadarOrg/DepRadar/issues/162) standardizes finding references across scanners.
- **Status:** ✅ Mitigated.

---

## 6. Related Issues & Hardening Roadmap

- [#171 — Reject self-claims by the bounty creator](https://github.com/DepRadarOrg/DepRadar/issues/171)
- [#172 — Minimum bounty amount per token](https://github.com/DepRadarOrg/DepRadar/issues/172)
- [#173 — Add a get_config getter](https://github.com/DepRadarOrg/DepRadar/issues/173)
- [#162 — Agree on a standard issue_ref format for findings](https://github.com/DepRadarOrg/DepRadar/issues/162)
