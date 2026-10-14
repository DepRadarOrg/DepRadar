# Testnet Read Client

Minimal read-only client for the `deprader_router` Soroban contract on
Stellar testnet. Fetches a bounty via `get_bounty` and every claim
submitted against it via `get_claim`, and prints the result. No transactions are
submitted, no funded account is required — reads are simulated calls
against the RPC endpoint.

Default Contract ID: `CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`  
Default RPC URL: `https://soroban-testnet.stellar.org`

## Usage

```bash
npm install
npm start                      # defaults to bounty-id 1
npm start -- --bounty-id 2

# Point to custom contract or RPC endpoint via CLI flags:
npm start -- --bounty-id 1 --contract <CONTRACT_ID> --rpc <RPC_URL>

# Or configure via environment variables:
export DEPRADAR_CONTRACT_ID="<CONTRACT_ID>"
export DEPRADAR_RPC_URL="<RPC_URL>"
npm start -- --bounty-id 1
```

### Options

| Flag | Env Var | Description | Default |
|---|---|---|---|
| `--bounty-id <id>` | — | Bounty ID to inspect (non-negative integer) | `1` |
| `--contract <id>` | `DEPRADAR_CONTRACT_ID` | Soroban contract ID | `CCPRQANY...` (testnet deployment) |
| `--rpc <url>` | `DEPRADAR_RPC_URL` | Stellar RPC endpoint URL | `https://soroban-testnet.stellar.org` |

Bounty `1` is real on-chain data: the `create_bounty` -> `submit_claim`
(x2) -> `reject_claim` -> `release` cycle recorded in
[PROGRESS.md](../PROGRESS.md), so it shows one rejected and one accepted
claim. Bounty `2` was cancelled and refunded. An unknown ID prints
`(no bounty with this ID)` rather than erroring, since `get_bounty` returns
`None` for unknown IDs on-chain.
