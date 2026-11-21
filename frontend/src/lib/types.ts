/** Mirrors `BountyStatus` in contracts/deprader_router/src/lib.rs. */
export type BountyStatus = "Open" | "Released" | "Cancelled";

/** Mirrors `ClaimStatus` in contracts/deprader_router/src/lib.rs. */
export type ClaimStatus = "Pending" | "Rejected" | "Accepted";

/** Mirrors the contract's `Bounty` struct, with the ID attached. */
export interface Bounty {
  id: number;
  creator: string;
  issueRef: string;
  /** Raw token amount in the token's smallest unit (stroops for XLM). */
  amount: bigint;
  token: string;
  status: BountyStatus;
  /** Unix seconds. */
  createdAt: number;
  /** Unix seconds. */
  deadline: number;
  claimCount: number;
  pendingClaims: number;
  /** Optional structured finding metadata: ecosystem symbol (e.g. "stellar"). */
  ecosystem?: string;
  /** Optional structured finding metadata: affected package name. */
  package?: string;
  /** Optional structured finding metadata: severity level. */
  severity?: number;
}

/** Mirrors the contract's `Claim` struct, with its IDs attached. */
export interface Claim {
  bountyId: number;
  id: number;
  claimant: string;
  proofRef: string;
  /** Unix seconds. */
  submittedAt: number;
  status: ClaimStatus;
}