import { describe, expect, it } from "vitest";
import { DEFAULT_SORT, isSortKey, sortBounties } from "./bountySort";
import type { Bounty } from "./types";

const bounty = (overrides: Partial<Bounty> & { id: number }): Bounty => ({
  creator: "GCREATOR",
  issueRef: `npm:pkg-${overrides.id}`,
  amount: 0n,
  token: "NATIVE",
  status: "Open",
  createdAt: 1_700_000_000,
  deadline: 1_900_000_000,
  claimCount: 0,
  pendingClaims: 0,
  ...overrides,
});

const ids = (list: Bounty[]) => list.map((b) => b.id);

describe("sortBounties", () => {
  it("sorts newest first by createdAt, breaking ties by id", () => {
    const list = [
      bounty({ id: 1, createdAt: 100 }),
      bounty({ id: 2, createdAt: 300 }),
      bounty({ id: 3, createdAt: 300 }),
    ];
    expect(ids(sortBounties(list, "newest"))).toEqual([3, 2, 1]);
  });

  it("sorts highest reward first, comparing amounts as bigint", () => {
    // These differ only past Number.MAX_SAFE_INTEGER, so a float comparison
    // would treat them as equal and string comparison would order them wrong.
    const list = [
      bounty({ id: 1, amount: 9_007_199_254_740_992n }),
      bounty({ id: 2, amount: 9_007_199_254_740_993n }),
      bounty({ id: 3, amount: 10n }),
      bounty({ id: 4, amount: 9n }),
    ];
    expect(ids(sortBounties(list, "reward"))).toEqual([2, 1, 3, 4]);
  });

  it("handles amounts far beyond 64 bits", () => {
    const huge = 10n ** 30n;
    const list = [bounty({ id: 1, amount: huge }), bounty({ id: 2, amount: huge + 1n })];
    expect(ids(sortBounties(list, "reward"))).toEqual([2, 1]);
  });

  it("sorts closing soon by earliest deadline first", () => {
    const list = [
      bounty({ id: 1, deadline: 500 }),
      bounty({ id: 2, deadline: 200 }),
      bounty({ id: 3, deadline: 800 }),
    ];
    expect(ids(sortBounties(list, "closing"))).toEqual([2, 1, 3]);
  });

  it("sorts most claims first", () => {
    const list = [
      bounty({ id: 1, claimCount: 1 }),
      bounty({ id: 2, claimCount: 5 }),
      bounty({ id: 3, claimCount: 5 }),
    ];
    expect(ids(sortBounties(list, "claims"))).toEqual([3, 2, 1]);
  });

  it("does not mutate the input array", () => {
    const list = [bounty({ id: 1, amount: 1n }), bounty({ id: 2, amount: 2n })];
    sortBounties(list, "reward");
    expect(ids(list)).toEqual([1, 2]);
  });
});

describe("isSortKey", () => {
  it("accepts known keys and rejects anything else", () => {
    expect(isSortKey("reward")).toBe(true);
    expect(isSortKey("bogus")).toBe(false);
    expect(isSortKey(null)).toBe(false);
    expect(DEFAULT_SORT).toBe("newest");
  });
});
