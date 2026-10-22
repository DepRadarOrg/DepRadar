/**
 * Minimal read-only client for the deprader_router Soroban contract on
 * Stellar testnet. Fetches a bounty and every claim submitted against it
 * and prints the result — no submitted transactions, no UI.
 *
 * Usage:
 *   npm start -- --bounty-id 1
 *   npm start -- --bounty-id 1 --contract C... --rpc https://...
 *
 * --bounty-id defaults to 1, the bounty used in the on-chain
 * create_bounty -> submit_claim -> reject_claim -> release cycle described
 * in PROGRESS.md, so running with no flags exercises real data.
 *
 * --contract / --rpc override the target deployment. When omitted, the
 * DEPRADAR_CONTRACT_ID / DEPRADAR_RPC_URL env vars are used, falling back
 * to the default testnet deployment below.
 */
import { rpc } from "@stellar/stellar-sdk";

const DEFAULT_CONTRACT_ID = "CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ";
const DEFAULT_RPC_URL = "https://soroban-testnet.stellar.org";

function parseArgs(argv: string[]): { bountyId: number; contractId: string; rpcUrl: string } {
  const flags = new Map<string, string>();
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg.startsWith("--")) {
      flags.set(arg.slice(2), argv[i + 1]);
      i += 1;
    }
  }

  const bountyId = Number(flags.get("bounty-id") ?? "1");
  if (!Number.isInteger(bountyId) || bountyId < 0) {
    throw new Error(`--bounty-id must be a non-negative integer, got: ${flags.get("bounty-id")}`);
  }

  const contractId = flags.get("contract") ?? process.env.DEPRADAR_CONTRACT_ID ?? DEFAULT_CONTRACT_ID;
  const rpcUrl = flags.get("rpc") ?? process.env.DEPRADAR_RPC_URL ?? DEFAULT_RPC_URL;

  return { bountyId, contractId, rpcUrl };
}

async function main() {
  const { bountyId, contractId, rpcUrl } = parseArgs(process.argv.slice(2));
  const server = new rpc.Server(rpcUrl);

  const health = await server.getHealth();
  console.log(`Connected to ${rpcUrl} (status: ${health.status}, latest ledger: ${health.latestLedger})`);
  console.log(`Contract: ${contractId}`);

  const { result: count } = await server.queryContract(contractId, "bounty_count", {});
  console.log(`Bounties created: ${count}\n`);

  const { result: bounty } = await server.queryContract(contractId, "get_bounty", { bounty_id: bountyId });
  console.log(`get_bounty(${bountyId}) =>`);
  if (bounty === null) {
    console.log("  (no bounty with this ID)");
    return;
  }
  console.log(JSON.stringify(bounty, bigIntReplacer, 2));

  const claimCount = Number((bounty as { claim_count: number }).claim_count);
  for (let claimId = 0; claimId < claimCount; claimId += 1) {
    const { result: claim } = await server.queryContract(contractId, "get_claim", {
      bounty_id: bountyId,
      claim_id: claimId,
    });
    console.log(`\nget_claim(${bountyId}, ${claimId}) =>`);
    console.log(claim === null ? "  (no claim with this ID)" : JSON.stringify(claim, bigIntReplacer, 2));
  }
}

function bigIntReplacer(_key: string, value: unknown) {
  return typeof value === "bigint" ? value.toString() : value;
}

main().catch((err) => {
  console.error("Read failed:", err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
