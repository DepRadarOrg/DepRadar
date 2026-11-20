export function bigIntReplacer(_key: string, value: unknown): unknown {
  return typeof value === "bigint" ? value.toString() : value;
}

export function formatHeader(
  rpcUrl: string,
  health: { status: string; latestLedger: number | string },
  contractId: string,
  count: unknown,
): string {
  return [
    `Connected to ${rpcUrl} (status: ${health.status}, latest ledger: ${health.latestLedger})`,
    `Contract: ${contractId}`,
    `Bounties created: ${count}`,
  ].join("\n");
}

export function formatBounty(bountyId: number, bounty: unknown): string {
  if (bounty === null) {
    return `get_bounty(${bountyId}) =>\n  (no bounty with this ID)`;
  }
  return `get_bounty(${bountyId}) =>\n${JSON.stringify(bounty, bigIntReplacer, 2)}`;
}

export function formatClaim(bountyId: number, claimId: number, claim: unknown): string {
  if (claim === null) {
    return `get_claim(${bountyId}, ${claimId}) =>\n  (no claim with this ID)`;
  }
  return `get_claim(${bountyId}, ${claimId}) =>\n${JSON.stringify(claim, bigIntReplacer, 2)}`;
}
