# Deployment Guide

This document describes how to deploy and upgrade the `deprader_router` contract on Stellar.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) with the `wasm32-unknown-unknown` target:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- The [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli) (v21 or newer):
  ```bash
  cargo install --locked stellar-cli --features opt
  ```
- Node.js 18+ (only needed to verify the deployment with the read client in `client/`).

## Identities

The Stellar CLI stores signing keys in named identities. Create one for the deployer
and, if you want a separate admin, one for the admin:

```bash
stellar keys generate deployer --network testnet --fund
stellar keys generate admin --network testnet --fund
```

- `--network testnet` records the network the identity defaults to.
- `--fund` uses friendbot to fund the new account on testnet. On mainnet you must
  fund the account manually and omit `--fund`.

Show an identity's public key (this is the address you pass as `--admin`):

```bash
stellar keys address deployer
stellar keys address admin
```

Check the balance of an identity:

```bash
stellar keys fund deployer --network testnet   # top up on testnet
```

## Scripted Deployment

`scripts/deploy.sh` wraps the whole flow: it builds the optimized WASM, deploys it,
initializes the contract with an admin, and prints the resulting contract ID.

Usage:

```bash
./scripts/deploy.sh [--network <NETWORK>] [--source <IDENTITY>] [--admin <ADDRESS>]
```

| Argument | Default | Description |
| --- | --- | --- |
| `--network` | `testnet` | Network to deploy to (`testnet`, `futurenet`, `mainnet`, or a custom RPC name). |
| `--source` | `deployer` | Stellar CLI identity used to sign the deploy and `initialize` transactions. |
| `--admin` | address of `--source` | Address passed to `initialize --admin`. |

Example — fresh testnet deploy with a dedicated admin:

```bash
./scripts/deploy.sh --network testnet --source deployer --admin "$(stellar keys address admin)"
```

The script prints the contract ID at the end, e.g.:

```
Contract ID: CCPR...YQJQ
```

Save that ID — it is needed for verification and for updating the client and frontend.

### Manual Equivalent

If you prefer to run the steps by hand, the script performs:

1. Build the optimized WASM:
```bash
cargo build --target wasm32-unknown-unknown --release --package deprader_router
```

2. Optimize the WASM (optional but recommended):
```bash
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/deprader_router.wasm \
  --wasm-out deprader_router_optimized.wasm
```

3. Deploy the contract:
```bash
stellar contract deploy \
  --wasm deprader_router_optimized.wasm \
  --source <YOUR_FUNDED_ACCOUNT> \
  --network testnet
```

This returns the contract ID (e.g., `CCPR...YQJQ`).

4. Initialize the contract with an admin address:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network testnet \
  -- initialize --admin <ADMIN_ADDRESS>
```

## Verifying the Deployment

### With the Stellar CLI

Confirm the contract responds and the admin was set:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- get_admin
```

### With the Read Client

The read client in [`client/`](client/) queries the contract over RPC. Point it at the
new contract ID and run it:

```bash
cd client
npm install
CONTRACT_ID=<CONTRACT_ID> npm start
```

If the client reads the contract ID from a different variable or file, set it there
instead — see [`client/README.md`](client/README.md) for the exact configuration.

## Updating the Contract ID

After a successful deploy, update the contract ID everywhere it is referenced:

- **`README.md`** — the contract ID shown in the project overview / usage examples.
- **`client/README.md`** and the client configuration — the ID used by the read client.
- **`frontend/`** — the ID in `frontend/src/config.ts` (or the equivalent config module)
  and the value documented in `frontend/.env.example`. If the frontend reads the ID from
  an environment variable, update your local `.env` as well.

Search the repository to make sure no stale ID remains:

```bash
grep -rn "C[A-Z0-9]\{55\}" --include="*.md" --include="*.ts" --include="*.tsx" --include="*.example" .
```

## Contract Upgrade Procedure

The `upgrade` function allows the admin to update the contract to new WASM without losing state or changing the contract ID.

### Prerequisites
- Admin account with signing authority
- New contract WASM built and optimized
- Contract must already be deployed and initialized

### Upgrade Steps

1. Build the new contract version:
```bash
cargo build --target wasm32-unknown-unknown --release --package deprader_router
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/deprader_router.wasm \
  --wasm-out deprader_router_v2.wasm
```

2. Install (upload) the new WASM to the network and get its hash:
```bash
stellar contract install \
  --wasm deprader_router_v2.wasm \
  --source <ADMIN_ACCOUNT> \
  --network testnet
```

This returns a 32-byte hash like `a1b2c3d4...`.

3. Invoke the `upgrade` function with the new WASM hash:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network testnet \
  -- upgrade --new_wasm_hash <WASM_HASH>
```

4. Verify the upgrade succeeded:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ANY_ACCOUNT> \
  --network testnet \
  -- get_admin
```

If this returns the expected admin address, the upgrade succeeded and all state is preserved.

### Important Notes

- **State Preservation**: All bounties, claims, and admin settings remain intact after an upgrade. Only the contract code changes.
- **Contract ID**: The contract ID remains the same. Clients do not need to update their contract addresses.
- **Admin-Only**: Only the current admin can invoke `upgrade`. The call will fail for any other caller.
- **Testing**: Always test upgrades on testnet before applying them to mainnet.
- **Events**: The upgrade emits an `upgraded` event with the admin address and new WASM hash, which can be monitored for audit purposes.
- **Emergency Use**: The upgrade function can be used during the pause state if critical fixes are needed.

### Rollback

If an upgrade introduces issues, the admin can "roll back" by uploading the previous contract WASM and calling `upgrade` again with its hash. Since state is preserved, this restores the old behavior without data loss.

## Network Endpoints

- **Testnet RPC**: `https://soroban-testnet.stellar.org`
- **Mainnet RPC**: `https://soroban-mainnet.stellar.org`

## Useful Commands

Check contract admin:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- get_admin
```

Check pause state:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- is_paused
```

Get bounty count:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- bounty_count
```
