export const DEFAULT_CONTRACT_ID = "CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ";
export const DEFAULT_RPC_URL = "https://soroban-testnet.stellar.org";

export interface ClientArgs {
  bountyId: number;
  contractId: string;
  rpcUrl: string;
}

export function parseArgs(
  argv: string[],
  env: Record<string, string | undefined> = process.env,
): ClientArgs {
  const flags = new Map<string, string | undefined>();
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg.startsWith("--")) {
      flags.set(arg.slice(2), argv[i + 1]);
      i += 1;
    }
  }

  const hasBountyId = flags.has("bounty-id");
  const rawBountyId = hasBountyId ? flags.get("bounty-id") : "1";
  const bountyId = Number(rawBountyId);
  if (!Number.isInteger(bountyId) || bountyId < 0) {
    throw new Error(`--bounty-id must be a non-negative integer, got: ${rawBountyId}`);
  }

  const contractId = flags.get("contract") ?? env.DEPRADAR_CONTRACT_ID ?? DEFAULT_CONTRACT_ID;
  const rpcUrl = flags.get("rpc") ?? env.DEPRADAR_RPC_URL ?? DEFAULT_RPC_URL;

  return { bountyId, contractId, rpcUrl };
}
