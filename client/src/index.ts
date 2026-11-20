/**
 * Minimal read-only client for the deprader_router Soroban contract on
 * Stellar testnet. Fetches a bounty and every claim submitted against it
 * and prints the result — no submitted transactions, no UI.
 *
 * Usage:
 *   npm start -- --bounty-id 1
 *   npm start -- --all
 *   npm start -- --json
 *   npm start -- --all --json
 *   npm start -- --bounty-id 1 --contract C... --rpc https://...
 *
 * --bounty-id defaults to 1, the bounty used in the on-chain
 * create_bounty -> submit_claim -> reject_claim -> release cycle described
 * in PROGRESS.md, so running with no flags exercises real data.
 *
 * --all fetches and prints every bounty from 1 to bounty_count.
 *
 * --json outputs a single JSON document to stdout instead of human-readable text.
 *
 * --contract / --rpc override the target deployment. When omitted, the
 * DEPRADAR_CONTRACT_ID / DEPRADAR_RPC_URL env vars are used, falling back
 * to the default testnet deployment below.
 */
import { rpc } from "@stellar/stellar-sdk";

const DEFAULT_CONTRACT_ID = "CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ";
const DEFAULT_RPC_URL = "https://soroban-testnet.stellar.org";

interface Args {
  bountyId: number;
  contractId: string;
  rpcUrl: string;
  all: boolean;
  json: boolean;
}

interface BountyResult {
  bountyId: number;
  bounty: unknown;
  claims: unknown[];
}

function parseArgs(argv: string[]): Args {
  const flags = new Map<string, string>();
  let all = false;
  let json = false;

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--all") {
      all = true;
    } else if (arg === "--json") {
      json = true;
    } else if (arg.startsWith("--")) {
      const eq = arg.indexOf("=");
      if (eq > 0) {
        flags.set(arg.slice(2, eq), arg.slice(eq + 1));
      } else {
        flags.set(arg.slice(2), argv[i + 1]);
        i += 1;
      }
    }
  }

  const bountyId = Number(flags.get("bounty-id") ?? "1");
  if (!Number.isInteger(bountyId) || bountyId < 0) {
    throw new Error(`--bounty-id must be a non-negative integer, got: ${flags.get("bounty-id")}`);
  }

  const contractId = flags.get("contract") ?? process.env.DEPRADAR_CONTRACT_ID ?? DEFAULT_CONTRACT_ID;
  const rpcUrl = flags.get("rpc") ?? process.env.DEPRADAR_RPC_URL ?? DEFAULT_RPC_URL;

  return { bountyId, contractId, rpcUrl, all, json };
}

async function fetchBounty(server: rpc.Server, contractId: string, bountyId: number): Promise<BountyResult> {
  const { result: bounty } = await server.queryContract(contractId, "get_bounty", { bounty_id: bountyId });
  if (bounty === null || bounty === undefined) {
    return { bountyId, bounty: null, claims: [] };
  }

  const claimCount = Number((bounty as { claim_count?: number | bigint }).claim_count ?? 0);
  const claims: unknown[] = [];
  for (let claimId = 0; claimId < claimCount; claimId += 1) {
    const { result: claim } = await server.queryContract(contractId, "get_claim", {
      bounty_id: bountyId,
      claim_id: claimId,
    });
    claims.push(claim);
  }

  return { bountyId, bounty, claims };
}

async function printHumanBounty(server: rpc.Server, contractId: string, bountyId: number): Promise<void> {
  const { result: bounty } = await server.queryContract(contractId, "get_bounty", { bounty_id: bountyId });
  console.log(`get_bounty(${bountyId}) =>`);
  if (bounty === null || bounty === undefined) {
    console.log("  (no bounty with this ID)");
    return;
  }
  console.log(JSON.stringify(bounty, bigIntReplacer, 2));

  const claimCount = Number((bounty as { claim_count?: number | bigint }).claim_count ?? 0);
  for (let claimId = 0; claimId < claimCount; claimId += 1) {
    const { result: claim } = await server.queryContract(contractId, "get_claim", {
      bounty_id: bountyId,
      claim_id: claimId,
    });
    console.log(`\nget_claim(${bountyId}, ${claimId}) =>`);
    console.log(claim === null ? "  (no claim with this ID)" : JSON.stringify(claim, bigIntReplacer, 2));
  }
}

async function main() {
  const { bountyId, contractId, rpcUrl, all, json } = parseArgs(process.argv.slice(2));
  const server = new rpc.Server(rpcUrl);

  const { result: countRaw } = await server.queryContract(contractId, "bounty_count", {});
  const count = Number(countRaw ?? 0);

  if (json) {
    if (all) {
      const results: BountyResult[] = [];
      for (let id = 1; id <= count; id += 1) {
        results.push(await fetchBounty(server, contractId, id));
      }
      console.log(JSON.stringify(results, bigIntReplacer, 2));
    } else {
      const result = await fetchBounty(server, contractId, bountyId);
      console.log(JSON.stringify(result, bigIntReplacer, 2));
    }
    return;
  }

  const health = await server.getHealth();
  console.log(`Connected to ${rpcUrl} (status: ${health.status}, latest ledger: ${health.latestLedger})`);
  console.log(`Contract: ${contractId}`);
  console.log(`Bounties created: ${count}\n`);

  if (all) {
    if (count === 0) {
      console.log("  (no bounties created yet)");
      return;
    }
    for (let id = 1; id <= count; id += 1) {
      await printHumanBounty(server, contractId, id);
      if (id < count) console.log("\n" + "-".repeat(40) + "\n");
    }
  } else {
    await printHumanBounty(server, contractId, bountyId);
  }
}

function bigIntReplacer(_key: string, value: unknown) {
  return typeof value === "bigint" ? value.toString() : value;
}

main().catch((err) => {
  console.error("Read failed:", err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
