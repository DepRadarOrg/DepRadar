import type { Bounty } from "./types";

/**
 * Keeps bounties whose `issueRef` contains `query`, ignoring case and
 * surrounding whitespace. A blank query keeps every bounty.
 */
export function filterByIssueRef(bounties: Bounty[], query: string): Bounty[] {
  const q = query.trim().toLowerCase();
  if (q === "") return bounties;
  return bounties.filter((b) => b.issueRef.toLowerCase().includes(q));
}
