# Contract events

The `deprader_router` contract emits Soroban events for each state change.
This page lists the layout of each event so indexers and the dashboard can
decode them without reading the source.

Every event has two parts:

- **Topics:** a tuple whose first element is a `Symbol` naming the event
  (at most 9 characters, which is why names are short), followed by any
  indexed values.
- **Data:** the payload. A single value is emitted as-is; several values
  are emitted as a tuple, in the order listed below.

Amounts are raw token units (stroops for XLM, 7 decimals). `bounty_id` and
`claim_id` are the contract-assigned IDs. Addresses are Stellar `G…` account
addresses or `C…` contract addresses.

## `created`

Emitted when a bounty is created with `create_bounty`.

| Part | Position | Type | Value |
| --- | --- | --- | --- |
| Topics | 0 | `Symbol` | `created` |
| Topics | 1 | `u64` | `bounty_id` |
| Data | 0 | `Address` | `creator` |
| Data | 1 | `Address` | `token` |
| Data | 2 | `i128` | `amount` |
| Data | 3 | `u64` | `deadline` (unix seconds) |

Example decoded from testnet: _TODO: fill in from a real `created` event on
the testnet contract `CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`._

## `claimed`

Emitted when a researcher submits a claim with `submit_claim`.

| Part | Position | Type | Value |
| --- | --- | --- | --- |
| Topics | 0 | `Symbol` | `claimed` |
| Topics | 1 | `u64` | `bounty_id` |
| Data | 0 | `u32` | `claim_id` |
| Data | 1 | `Address` | `claimant` |

Example decoded from testnet: _TODO: fill in from a real `claimed` event._

## `rejected`

Emitted when an admin or reviewer rejects a claim with `reject_claim`.

| Part | Position | Type | Value |
| --- | --- | --- | --- |
| Topics | 0 | `Symbol` | `rejected` |
| Topics | 1 | `u64` | `bounty_id` |
| Data | 0 | `u32` | `claim_id` |
| Data | 1 | `Address` | `caller` (the admin or reviewer who rejected it) |

Example decoded from testnet: _TODO: fill in from a real `rejected` event._

## `released`

Emitted when a bounty is paid out, once per paid claim. `release` emits one
event; `release_split` emits one event per recipient.

| Part | Position | Type | Value |
| --- | --- | --- | --- |
| Topics | 0 | `Symbol` | `released` |
| Topics | 1 | `u64` | `bounty_id` |
| Data | 0 | `u32` | `claim_id` |
| Data | 1 | `Address` | `claimant` (the payout recipient) |
| Data | 2 | `i128` | payout amount to the claimant |
| Data | 3 | `Address` | `caller` (the admin or reviewer who released it) |
| Data | 4 | `i128` | `drips_share` (amount routed to the Drips destination, `0` if none) |

`release_split` emits a shorter payload, with `claim_id`, `claimant`,
`payout` and `admin` in positions 0–3 and no `drips_share`.

Example decoded from testnet: _TODO: fill in from a real `released` event
(the release tx linked in the README is a starting point)._

## `cancelled`

Emitted when a bounty is cancelled with `cancel_bounty`. Funders are
refunded in the same transaction.

| Part | Position | Type | Value |
| --- | --- | --- | --- |
| Topics | 0 | `Symbol` | `cancelled` |
| Topics | 1 | `u64` | `bounty_id` |
| Data | — | `(Address, i128)` | `(caller, bounty.amount)`: the account that cancelled and the total amount refunded |

Example decoded from testnet: _TODO: fill in from a real `cancelled` event._

## `admin_set`

Emitted when the admin is set. It fires once in `initialize`, and again in
`accept_admin` when a proposed admin takes over.

| Part | Position | Type | Value |
| --- | --- | --- | --- |
| Topics | 0 | `Symbol` | `admin_set` |
| Data | — | `Address` | the new admin |

Example decoded from testnet: _TODO: fill in from a real `admin_set` event._

## `admin_prp`

Emitted when the current admin nominates a successor with `propose_admin`.
Nothing changes until the nominee calls `accept_admin`, which emits
`admin_set`.

| Part | Position | Type | Value |
| --- | --- | --- | --- |
| Topics | 0 | `Symbol` | `admin_prp` |
| Topics | 1 | `Address` | the current admin who proposed |
| Data | — | `Address` | the proposed (pending) admin |

Example decoded from testnet: _TODO: fill in from a real `admin_prp` event._

## Other events

The contract also emits events not listed above: `pause`, `tkn_allow`,
`tkn_deny`, `min_set`, `pool_set`, `pool_dep`, `pool_bty`, `funded`,
`extended`, and `withdrawn`. Their layouts are in `contracts/deprader_router/src/lib.rs`
next to each `publish` call.
