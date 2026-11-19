# Contract Error Codes Reference

When a call to the `deprader_router` contract fails, the Soroban runtime reverts the invocation with a typed contract error code in the form `Error(Contract, #<code>)`.

This reference maps every error code from [`contracts/deprader_router/src/lib.rs`](../contracts/deprader_router/src/lib.rs) to its enum variant, triggering conditions, and recommended resolution.

---

## Error Codes Table

| Code | Variant | Triggered In | Summary |
| :---: | :--- | :--- | :--- |
| **1** | `InvalidAmount` | `create_bounty`, `fund_bounty`, `split_release` | Amount is zero or negative, or split shares sum does not match bounty amount. |
| **3** | `BountyNotFound` | `get_bounty`, `submit_claim`, `release`, `split_release`, `reject_claim`, `cancel_bounty`, `fund_bounty` | The requested `bounty_id` does not exist in contract storage. |
| **5** | `AlreadyInitialized` | `initialize` | Contract admin has already been set; cannot be re-initialized. |
| **6** | `NotInitialized` | `create_bounty`, `require_admin` | The contract has not yet been initialized with an admin. |
| **7** | `NotAuthorized` | `set_paused`, `transfer_admin`, `release`, `split_release`, `reject_claim`, `cancel_bounty`, `withdraw_claim` | Caller is not authorized (not admin, not bounty creator, or not claimant). |
| **10** | `InvalidDeadline` | `create_bounty` | Deadline is in the past, or exceeds `MAX_BOUNTY_DURATION_SECS` (365 days). |
| **11** | `InvalidReference` | `create_bounty`, `submit_claim` | An `issue_ref`, `proof_ref`, `package`, or `ecosystem` string exceeds length limits or is empty. |
| **12** | `BountyNotOpen` | `submit_claim`, `fund_bounty`, `cancel_bounty` | Target bounty is not in `Open` status (already `Released` or `Cancelled`). |
| **13** | `ClaimWindowClosed` | `submit_claim` | Current ledger timestamp is past the bounty's `deadline`. |
| **14** | `DuplicateClaim` | `submit_claim` | The claimant address has already submitted a claim on this bounty. |
| **15** | `TooManyClaims` | `submit_claim` | Bounty reached `MAX_CLAIMS_PER_BOUNTY` (50) or claimant reached `MAX_CLAIMS_PER_CLAIMANT` (200). |
| **16** | `ClaimNotFound` | `release`, `split_release`, `reject_claim`, `withdraw_claim` | Target `claim_id` does not exist for the specified bounty. |
| **17** | `ClaimNotPending` | `release`, `split_release`, `reject_claim`, `withdraw_claim` | Target claim is not `Pending` (already `Accepted`, `Rejected`, or `Withdrawn`). |
| **18** | `CancelTooEarly` | `cancel_bounty` | Cancel attempted before deadline, or before the 7-day review window expired with pending claims. |
| **19** | `NoPendingAdmin` | `accept_admin` | No admin transfer is currently pending. |
| **20** | `InvalidShares` | `split_release` | Multi-claim payout share list is empty, contains non-positive values, or does not equal total bounty. |
| **21** | `TokenNotAllowed` | `create_bounty`, `fund_bounty` | Token contract address is not on the admin-approved token whitelist. |
| **22** | `Paused` | State-mutating methods | Contract operations are currently paused by the admin. |
| **23** | `CreatorCannotClaim` | `submit_claim` | Bounty creator cannot submit a claim on their own bounty. |
| **24** | `AmountTooSmall` | `create_bounty`, `fund_bounty` | Deposit amount is lower than the configured `min_amount` for the token. |

---

## Detailed Error Specifications

### 1: `InvalidAmount`
* **Condition:** Occurs when:
  1. `create_bounty` or `fund_bounty` is called with `amount <= 0`.
  2. `split_release` receives an individual share amount `<= 0`.
* **Resolution:** Ensure the funding amount is positive and greater than zero in the token's smallest unit (stroops for native XLM).

### 3: `BountyNotFound`
* **Condition:** The provided `bounty_id` does not correspond to an existing bounty record. Bounty IDs are sequential starting from `1`.
* **Resolution:** Verify the `bounty_id` via `bounty_count` or list queries before submitting transactions.

### 5: `AlreadyInitialized`
* **Condition:** `initialize` was called on a contract instance that already has an admin stored.
* **Resolution:** Do not re-initialize deployed contracts. To rotate admins, use `transfer_admin` and `accept_admin`.

### 6: `NotInitialized`
* **Condition:** A user or admin method was called before `initialize` set the initial admin address.
* **Resolution:** Call `initialize(admin)` once upon initial contract deployment.

### 7: `NotAuthorized`
* **Condition:** The transaction signer lacks required authority:
  * Admin-only functions (`set_paused`, `transfer_admin`, `set_allowed_token`, `set_min_amount`, `release`, `reject_claim`) called by a non-admin.
  * `cancel_bounty` called by an address other than the bounty `creator`.
  * `withdraw_claim` called by an address other than the `claimant`.
* **Resolution:** Ensure the transaction is signed by the appropriate identity (admin, creator, or claimant).

### 10: `InvalidDeadline`
* **Condition:** During `create_bounty`, the `deadline` ledger timestamp is:
  * In the past: `deadline <= env.ledger().timestamp()`
  * Exceeds maximum duration: `deadline > env.ledger().timestamp() + MAX_BOUNTY_DURATION_SECS` (365 days).
* **Resolution:** Set a deadline between current ledger time and 365 days into the future.

### 11: `InvalidReference`
* **Condition:** String parameter validation failed:
  * `issue_ref` or `proof_ref` is empty or exceeds `MAX_REF_LEN` (256 bytes).
  * Optional `package` metadata exceeds `MAX_PACKAGE_LEN` (128 bytes).
  * Optional `ecosystem` metadata exceeds `MAX_ECOSYSTEM_LEN` (32 bytes).
* **Resolution:** Keep reference strings within length constraints. See [`docs/issue-ref.md`](issue-ref.md) for standard formats.

### 12: `BountyNotOpen`
* **Condition:** Attempted to add funds (`fund_bounty`), submit a claim (`submit_claim`), or cancel (`cancel_bounty`) on a bounty whose status is not `BountyStatus::Open` (e.g. already `Released` or `Cancelled`).
* **Resolution:** Check `bounty.status` before submission. Claims and funding are only accepted while `Open`.

### 13: `ClaimWindowClosed`
* **Condition:** `submit_claim` was invoked after the bounty's `deadline` has elapsed (`env.ledger().timestamp() > bounty.deadline`).
* **Resolution:** Submit claims before the deadline timestamp. Once closed, no new claims can be registered.

### 14: `DuplicateClaim`
* **Condition:** A researcher address called `submit_claim` on a bounty where they already have a registered claim.
* **Resolution:** Each claimant address may submit at most one claim per bounty. To update proof, coordinate in the issue thread or withdraw the previous claim if allowed.

### 15: `TooManyClaims`
* **Condition:** Storage limits reached:
  * The bounty has reached `MAX_CLAIMS_PER_BOUNTY` (50 claims total).
  * The claimant address has reached `MAX_CLAIMS_PER_CLAIMANT` (200 claims across all bounties).
* **Resolution:** Bounties cap claims to prevent unbounded state growth. Claimants must not exceed 200 total tracked claims.

### 16: `ClaimNotFound`
* **Condition:** The specified `claim_id` was not found for the given `bounty_id`. Claim IDs are `0`-indexed within each bounty (`0..claim_count`).
* **Resolution:** Confirm the `claim_id` via `get_claim(bounty_id, claim_id)`.

### 17: `ClaimNotPending`
* **Condition:** Attempted to execute an admin decision (`release`, `split_release`, `reject_claim`) or claimant withdrawal (`withdraw_claim`) on a claim whose status is not `ClaimStatus::Pending`.
* **Resolution:** Claims can only be transitioned while in the `Pending` review state.

### 18: `CancelTooEarly`
* **Condition:** The creator attempted to call `cancel_bounty` before the cancellation conditions were met:
  1. The `deadline` has not yet passed (`env.ledger().timestamp() < bounty.deadline`).
  2. The deadline passed, but pending claims remain (`pending_claims > 0`) and the 7-day review window (`REVIEW_WINDOW_SECS` = 604,800 seconds) has not yet elapsed.
* **Resolution:** Wait until either (a) the deadline has passed and all pending claims have been reviewed, or (b) the 7-day post-deadline review window has expired.

### 19: `NoPendingAdmin`
* **Condition:** `accept_admin` was called when no admin migration was initiated via `transfer_admin`.
* **Resolution:** The current admin must first call `transfer_admin(new_admin)` before `new_admin` can call `accept_admin`.

### 20: `InvalidShares`
* **Condition:** In `split_release`, the provided shares list:
  * Is empty.
  * Contains a share amount `<= 0`.
  * The sum of share amounts does not exactly match `bounty.amount`.
* **Resolution:** Ensure shares sum precisely to the total bounty pool and allocate strictly positive amounts.

### 21: `TokenNotAllowed`
* **Condition:** `create_bounty` or `fund_bounty` specified a token that is not allowlisted in contract storage.
* **Resolution:** Use an approved token (e.g. native XLM contract) or request the admin to call `set_allowed_token(token, true)`.

### 22: `Paused`
* **Condition:** The contract is currently paused (`is_paused() == true`).
* **Resolution:** Wait for the contract admin to resume operations with `set_paused(false)`.

### 23: `CreatorCannotClaim`
* **Condition:** The creator address of a bounty attempted to call `submit_claim` on their own bounty.
* **Resolution:** Creators cannot claim their own bounties. Submissions must come from independent researcher accounts.

### 24: `AmountTooSmall`
* **Condition:** The deposited amount is lower than the minimum amount set for that token via `set_min_amount`.
* **Resolution:** Check `get_min_amount(token)` and provide an amount equal to or greater than the threshold.

---

## Constants Reference

Limits defined in [`contracts/deprader_router/src/lib.rs`](../contracts/deprader_router/src/lib.rs):

| Constant | Value | Description |
| :--- | :--- | :--- |
| `REVIEW_WINDOW_SECS` | `604,800` (7 days) | Admin review window after deadline before creator can reclaim with pending claims. |
| `MAX_BOUNTY_DURATION_SECS` | `31,536,000` (365 days) | Maximum time a bounty can remain open. |
| `MAX_CLAIMS_PER_BOUNTY` | `50` | Maximum number of claims allowed per bounty. |
| `MAX_CLAIMS_PER_CLAIMANT` | `200` | Maximum lifetime claims indexed per claimant address. |
| `MAX_REF_LEN` | `256` bytes | Maximum length for `issue_ref` and `proof_ref`. |
| `MAX_PACKAGE_LEN` | `128` bytes | Maximum length for optional package metadata. |
| `MAX_ECOSYSTEM_LEN` | `32` bytes | Maximum length for optional ecosystem metadata. |
