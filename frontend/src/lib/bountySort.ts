import type { Bounty } from "./types";

export const SORT_KEYS = ["newest", "reward", "closing", "claims"] as const;
export type SortKey = (typeof SORT_KEYS)[number];

export const DEFAULT_SORT: SortKey = "newest";

export function isSortKey(value: string | null): value is SortKey {
  return value !== null && (SORT_KEYS as readonly string[]).includes(value);
}

/** Compares two bigints without going through strings or floats. */
function compareBigInt(a: bigint, b: bigint): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * Returns a sorted copy of `bounties`. Ties fall back to newest-first by id
 * so the order is stable across renders.
 *
 * - `newest`: most recently created first
 * - `reward`: highest amount first (compared as bigint)
 * - `closing`: earliest deadline first
 * - `claims`: most claims first
 */
export function sortBounties(bounties: Bounty[], key: SortKey): Bounty[] {
  const newestFirst = (a: Bounty, b: Bounty) => b.id - a.id;
  const compare: Record<SortKey, (a: Bounty, b: Bounty) => number> = {
    newest: (a, b) => b.createdAt - a.createdAt || newestFirst(a, b),
    reward: (a, b) => compareBigInt(b.amount, a.amount) || newestFirst(a, b),
    closing: (a, b) => a.deadline - b.deadline || newestFirst(a, b),
    claims: (a, b) => b.claimCount - a.claimCount || newestFirst(a, b),
  };
  return [...bounties].sort(compare[key]);
}
