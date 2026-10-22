# Testnet Read Client

Minimal read-only client for the `deprader_router` Soroban contract on
Stellar testnet. Fetches a bounty via `get_bounty` and every claim
submitted against it via `get_claim`, and prints the result. No transactions are
submitted, no funded account is required — reads are simulated calls
against the RPC endpoint.

Contract ID: `CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`

## Usage

```bash
npm install
npm start                      # defaults to bounty-id 1
npm start -- --bounty-id 2
```

Bounty `1` is real on-chain data: the `create_bounty` -> `submit_claim`
(x2) -> `reject_claim` -> `release` cycle recorded in
[PROGRESS.md](../PROGRESS.md), so it shows one rejected and one accepted
claim. Bounty `2` was cancelled and refunded. An unknown ID prints
`(no bounty with this ID)` rather than erroring, since `get_bounty` returns
`None` for unknown IDs on-chain.

## Verifying a fresh deploy

After deploying a new contract with [`scripts/deploy.sh`](../scripts/deploy.sh),
update the contract ID above and use this client to confirm the deploy works.
See [DEPLOYMENT.md](../DEPLOYMENT.md) for the full procedure, including how to
verify a fresh deploy with the client.