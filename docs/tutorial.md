# Tutorial: Report Your First Typosquat and Claim a Bounty

This guide walks you through the complete lifecycle of finding a malicious package, proving the vulnerability, creating and claiming a bounty on Stellar testnet, and getting paid.

---

## The Workflow at a Glance

```text
1. Detect finding        cargo run --bin scan -- <candidate>
2. Report upstream       Report to npm / PyPI / crates.io security team
3. On-chain bounty       create_bounty(creator, issue_ref, amount, token, deadline)
4. Submit proof claim    submit_claim(bounty_id, claimant, proof_ref)
5. Verify on dashboard   Inspect live claims via web UI or read client CLI
6. Admin release         release(bounty_id, claim_id) -> funds transferred to finder
```

---

## Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable)
- Node.js 22+ (for client and dashboard)
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli) (optional, for submitting transactions directly)

---

## Step 1: Scan a Package Name for Typosquatting

DepRadar ships with a CLI scanner (`scan`) that evaluates candidate package names against curated datasets of the top-1000 npm and PyPI libraries using Levenshtein distance and Unicode homoglyph analysis.

Run the scanner locally:

```bash
cargo run --bin scan -- expresss
```

### Real Output: Edit Distance Signal

```text
expresss: likely typosquat
  - edit distance 1 from popular package "express"
```

The scanner flags `expresss` because its edit distance is `1` from the popular package `express` (an extra `s` at the end).

### Real Output: Homoglyph Impersonation

If an attacker substitutes a Latin character with a visually indistinguishable Cyrillic or Greek character (e.g., Cyrillic small letter `е` in `rеact`):

```bash
cargo run --bin scan -- "rеact"
```

```text
rеact: likely typosquat
  - edit distance 1 from popular package "react"
  - homoglyph impersonation of popular package "react"
```

### Real Output: Clean Package

When scanning a legitimate popular package name:

```bash
cargo run --bin scan -- express
```

```text
express: no typosquat signal detected
```

---

## Step 2: Verify Upstream and File a Disclosure

Before claiming on-chain, verify whether the package actually exists on the public registry:

1. **Check registry metadata:**
   ```bash
   npm view expresss
   ```
2. **Inspect payload:** Check if the package contains suspicious install scripts (`postinstall`), obfuscated blobs, or unauthorized telemetry.
3. **Report upstream:** Disclose the finding to the appropriate ecosystem security team:
   - **npm:** [GitHub Security Advisories](https://github.com/advisories) or npm support report.
   - **PyPI:** Email `security@pypi.org` or report on PyPI security disclosure portal.
   - **crates.io:** Report to the Rust Security Response WG.

Obtain a public or coordinated disclosure reference URL (advisory ID, GitHub issue, or CVE identifier).

---

## Step 3: Check Live Bounties on Testnet

The DepRadar Bounty Router is deployed on Stellar testnet at:  
**Contract ID:** `CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`  
**Network:** Stellar Testnet (`https://soroban-testnet.stellar.org`)

You can inspect open bounties using the read client:

```bash
cd client
npm ci
npm start -- --bounty-id 1
```

### Real Output from Testnet

```json
Connected to https://soroban-testnet.stellar.org (status: healthy, latest ledger: 5184282)
Contract: CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ
Bounties created: 2

get_bounty(1) =>
{
  "amount": "50000000",
  "claim_count": 2,
  "created_at": "1791546147",
  "creator": "GCFFP6RU6C3JOZBIUNZN2ELJAITUZ3HWEOUUCFXIAHE6M2GHVHZB4D6A",
  "deadline": "1794138138",
  "issue_ref": "npm:expresss typosquat of express (demo)",
  "pending_claims": 0,
  "status": {
    "tag": "Released"
  },
  "token": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
}
```

Notice that Bounty #1 locks `50,000,000` stroops (5 XLM) against `issue_ref: npm:expresss typosquat of express (demo)`.

---

## Step 4: Submit Your Claim

To claim a bounty, call `submit_claim` against the contract:

```bash
# Method signature:
# submit_claim(bounty_id: u64, claimant: Address, proof_ref: String) -> u32
```

Using the Stellar CLI with your testnet identity:

```bash
stellar contract invoke \
  --id CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ \
  --source <YOUR_IDENTITY> \
  --network testnet \
  -- \
  submit_claim \
  --bounty_id 1 \
  --claimant <YOUR_ADDRESS> \
  --proof_ref "DepRadarOrg/DepRadar#demo-report"
```

`submit_claim` returns the assigned `claim_id` (e.g. `1`). Funds are not moved at this stage; your claim is entered with `Pending` status.

---

## Step 5: Triage and Resolution

The admin reviews claims submitted for the bounty:

1. **Junk or Duplicate Claims:**
   If another user submitted a junk claim first (`proof_ref: "junk"`), the admin rejects it:
   ```bash
   # reject_claim(bounty_id: 1, claim_id: 0)
   ```
   The bounty remains `Open`.

2. **Valid Claim Release:**
   The admin validates your disclosure writeup and releases the bounty:
   ```bash
   # release(bounty_id: 1, claim_id: 1)
   ```

When `release` is executed:
- Claim #1 status transitions to `Accepted`.
- Bounty status transitions to `Released`.
- The contract executes an on-chain transfer of the locked token balance directly to your claimant address.

You can verify the payment on the block explorer:  
[View Release Transaction on Stellar Expert](https://stellar.expert/explorer/testnet/tx/69538e183dab1799c92be7231e562fcbca47bd32e0247a44f0cddfbeaee84cb0)

---

## Step 6: View in the Web Dashboard

To monitor claims and bounties visually:

```bash
cd frontend
npm ci
npm run dev
```

Open `http://localhost:5173` in your browser. The dashboard displays:
- Active and released bounties.
- Total XLM locked and paid out.
- The complete claim timeline showing rejected and accepted claims.
