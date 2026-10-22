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
```

Bounty `1` is real on-chain data: the `create_bounty` -> `submit_claim`
(x2) -> `reject_claim` -> `release` cycle recorded in
[PROGRESS.md](../PROGRESS.md), so it shows one rejected and one accepted
claim. Bounty `2` was cancelled and refunded. An unknown ID prints
`(no bounty with this ID)` rather than erroring, since `get_bounty` returns
`None` for unknown IDs on-chain.

## Configuration

The contract ID and RPC endpoint can be overridden without editing code, so
the client can be pointed at another deployment.

| Setting     | Flag         | Environment variable     | Default                                                        |
| ----------- | ------------ | ------------------------ | -------------------------------------------------------------- |
| Contract ID | `--contract` | `DEPRADAR_CONTRACT_ID`   | `CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ`     |
| RPC URL     | `--rpc`      | `DEPRADAR_RPC_URL`       | `https://soroban-testnet.stellar.org`                          |

Precedence is **flag > environment variable > default**: a flag always wins,
otherwise the environment variable is used if set, otherwise the built-in
default applies.

### Examples

```bash
# Override both via flags
npm start -- --contract <CONTRACT_ID> --rpc <RPC_URL>

# Override via environment variables
DEPRADAR_CONTRACT_ID=<CONTRACT_ID> DEPRADAR_RPC_URL=<RPC_URL> npm start

# Flags take precedence over environment variables
DEPRADAR_CONTRACT_ID=<IGNORED_ID> npm start -- --contract <CONTRACT_ID>

# Combine with other options
npm start -- --bounty-id 2 --contract <CONTRACT_ID> --rpc <RPC_URL>
