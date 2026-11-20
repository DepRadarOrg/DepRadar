# Testnet Read Client

Minimal read-only client for the `deprader_router` Soroban contract on
Stellar testnet. Fetches a bounty via `get_bounty` and every claim
submitted against it via `get_claim`, and prints the result. No transactions are
submitted, no funded account is required — reads are simulated calls
against the RPC endpoint.

Default contract ID: `CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`

## Usage

```bash
npm install
npm start                      # defaults to bounty-id 1
npm start -- --bounty-id 2
npm start -- --all             # prints every bounty 1..=bounty_count
npm start -- --json            # outputs single JSON document to stdout
npm start -- --all --json      # outputs all bounties as a JSON array
```

Bounty `1` is real on-chain data: the `create_bounty` -> `submit_claim`
(x2) -> `reject_claim` -> `release` cycle recorded in
[PROGRESS.md](../PROGRESS.md), so it shows one rejected and one accepted
claim. Bounty `2` was cancelled and refunded. An unknown ID prints
`(no bounty with this ID)` rather than erroring, since `get_bounty` returns
`None` for unknown IDs on-chain.

## Options & Flags

| Flag | Description | Default |
| --- | --- | --- |
| `--bounty-id <id>` | Specific bounty ID to fetch | `1` |
| `--all` | Fetch every bounty (`1..=bounty_count`) and its claims | `false` |
| `--json` | Output pure JSON to stdout instead of human-readable text | `false` |
| `--contract <id>` | Target contract ID (env: `DEPRADAR_CONTRACT_ID`) | Built-in testnet contract |
| `--rpc <url>` | Target Soroban RPC URL (env: `DEPRADAR_RPC_URL`) | `https://soroban-testnet.stellar.org` |

Precedence is **flag > environment variable > default**: a flag always wins,
otherwise the environment variable is used if set, otherwise the built-in
default applies.

## Output Examples

### Single Bounty with `--json`

```bash
npm start -- --bounty-id 1 --json
```

```json
{
  "bountyId": 1,
  "bounty": {
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
  },
  "claims": [
    {
      "claimant": "GC2OYUR53G6IHMRXS2IH2WV3ANIXHIKXM3OJRXSDKESMLEYEOXQN4BLY",
      "proof_ref": "junk",
      "status": {
        "tag": "Rejected"
      },
      "submitted_at": "1791546157"
    },
    {
      "claimant": "GDLSO26NR37KTXN5KO4TVG53TDEWAYSXIT2CXZZ6VHS2BZB2FEP2J2VW",
      "proof_ref": "DepRadarOrg/DepRadar#demo-report",
      "status": {
        "tag": "Accepted"
      },
      "submitted_at": "1791546167"
    }
  ]
}
```

### All Bounties with `--all --json`

```bash
npm start -- --all --json
```

```json
[
  {
    "bountyId": 1,
    "bounty": {
      "amount": "50000000",
      "claim_count": 2,
      "status": { "tag": "Released" },
      ...
    },
    "claims": [ ... ]
  },
  {
    "bountyId": 2,
    "bounty": {
      "amount": "20000000",
      "claim_count": 1,
      "status": { "tag": "Cancelled" },
      ...
    },
    "claims": [ ... ]
  }
]
```

### Overriding Contract and RPC

```bash
# Override both via flags
npm start -- --contract <CONTRACT_ID> --rpc <RPC_URL>

# Override via environment variables
DEPRADAR_CONTRACT_ID=<CONTRACT_ID> DEPRADAR_RPC_URL=<RPC_URL> npm start

# Flags take precedence over environment variables
DEPRADAR_CONTRACT_ID=<IGNORED_ID> npm start -- --contract <CONTRACT_ID>
```
