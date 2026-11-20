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
import { parseArgs } from "./args";
import { formatBounty, formatClaim, formatHeader } from "./format";

export * from "./args";
export * from "./format";

async function main() {
  const { bountyId, contractId, rpcUrl } = parseArgs(process.argv.slice(2));
  const server = new rpc.Server(rpcUrl);

  const health = await server.getHealth();
  const { result: count } = await server.queryContract(contractId, "bounty_count", {});
  console.log(formatHeader(rpcUrl, health, contractId, count));
  console.log();

  const { result: bounty } = await server.queryContract(contractId, "get_bounty", { bounty_id: bountyId });
  console.log(formatBounty(bountyId, bounty));
  if (bounty === null) {
    return;
  }

  const claimCount = Number((bounty as { claim_count: number }).claim_count);
  for (let claimId = 0; claimId < claimCount; claimId += 1) {
    const { result: claim } = await server.queryContract(contractId, "get_claim", {
      bounty_id: bountyId,
      claim_id: claimId,
    });
    console.log();
    console.log(formatClaim(bountyId, claimId, claim));
  }
}

main().catch((err) => {
  console.error("Read failed:", err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
