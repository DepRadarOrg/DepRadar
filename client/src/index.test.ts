import { describe, expect, it } from "vitest";
import {
  DEFAULT_CONTRACT_ID,
  DEFAULT_RPC_URL,
  bigIntReplacer,
  formatBounty,
  formatClaim,
  formatHeader,
  parseArgs,
} from "./index";

describe("parseArgs", () => {
  it("uses default values when no CLI flags or env vars are provided", () => {
    const args = parseArgs([], {});
    expect(args).toEqual({
      bountyId: 1,
      contractId: DEFAULT_CONTRACT_ID,
      rpcUrl: DEFAULT_RPC_URL,
    });
  });

  it("parses valid --bounty-id values", () => {
    expect(parseArgs(["--bounty-id", "0"], {}).bountyId).toBe(0);
    expect(parseArgs(["--bounty-id", "5"], {}).bountyId).toBe(5);
    expect(parseArgs(["--bounty-id", "99999"], {}).bountyId).toBe(99999);
  });

  it("throws when --bounty-id is negative", () => {
    expect(() => parseArgs(["--bounty-id", "-1"], {})).toThrow(
      "--bounty-id must be a non-negative integer, got: -1",
    );
    expect(() => parseArgs(["--bounty-id", "-42"], {})).toThrow(
      "--bounty-id must be a non-negative integer, got: -42",
    );
  });

  it("throws when --bounty-id is a float / non-integer", () => {
    expect(() => parseArgs(["--bounty-id", "1.5"], {})).toThrow(
      "--bounty-id must be a non-negative integer, got: 1.5",
    );
  });

  it("throws when --bounty-id is non-numeric", () => {
    expect(() => parseArgs(["--bounty-id", "abc"], {})).toThrow(
      "--bounty-id must be a non-negative integer, got: abc",
    );
  });

  it("throws when --bounty-id has no subsequent value", () => {
    expect(() => parseArgs(["--bounty-id"], {})).toThrow(
      "--bounty-id must be a non-negative integer, got: undefined",
    );
  });

  it("parses custom --contract and --rpc flags", () => {
    const customContract = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const customRpc = "https://custom-soroban-rpc.example.com";

    const args = parseArgs(
      ["--contract", customContract, "--rpc", customRpc, "--bounty-id", "10"],
      {},
    );

    expect(args).toEqual({
      bountyId: 10,
      contractId: customContract,
      rpcUrl: customRpc,
    });
  });

  it("falls back to environment variables when flags are omitted", () => {
    const env = {
      DEPRADAR_CONTRACT_ID: "CENVCONTRACTID123",
      DEPRADAR_RPC_URL: "https://rpc-from-env.example.com",
    };

    const args = parseArgs([], env);
    expect(args.contractId).toBe("CENVCONTRACTID123");
    expect(args.rpcUrl).toBe("https://rpc-from-env.example.com");
    expect(args.bountyId).toBe(1);
  });

  it("prefers command-line flags over environment variables", () => {
    const env = {
      DEPRADAR_CONTRACT_ID: "CENVCONTRACTID123",
      DEPRADAR_RPC_URL: "https://rpc-from-env.example.com",
    };

    const args = parseArgs(
      ["--contract", "CCLICONTRACT", "--rpc", "https://cli-rpc.example.com"],
      env,
    );
    expect(args.contractId).toBe("CCLICONTRACT");
    expect(args.rpcUrl).toBe("https://cli-rpc.example.com");
  });
});

describe("bigIntReplacer", () => {
  it("converts bigint values to strings", () => {
    expect(bigIntReplacer("key", 100n)).toBe("100");
    expect(bigIntReplacer("amount", 9007199254740991000n)).toBe("9007199254740991000");
  });

  it("leaves non-bigint values unchanged", () => {
    expect(bigIntReplacer("key", "test")).toBe("test");
    expect(bigIntReplacer("key", 123)).toBe(123);
    expect(bigIntReplacer("key", true)).toBe(true);
    expect(bigIntReplacer("key", null)).toBe(null);
    expect(bigIntReplacer("key", undefined)).toBe(undefined);
  });

  it("works with JSON.stringify for nested structures containing bigint", () => {
    const data = {
      id: 1,
      name: "bounty",
      payout: 5000000000n,
      nested: {
        shares: [10n, 20n],
      },
    };

    const json = JSON.stringify(data, bigIntReplacer);
    expect(json).toBe('{"id":1,"name":"bounty","payout":"5000000000","nested":{"shares":["10","20"]}}');
  });
});

describe("formatHeader", () => {
  it("formats the connection header and summary", () => {
    const output = formatHeader(
      "https://soroban-testnet.stellar.org",
      { status: "healthy", latestLedger: 12345 },
      DEFAULT_CONTRACT_ID,
      3,
    );

    expect(output).toBe(
      `Connected to https://soroban-testnet.stellar.org (status: healthy, latest ledger: 12345)\n` +
      `Contract: ${DEFAULT_CONTRACT_ID}\n` +
      `Bounties created: 3`,
    );
  });
});

describe("formatBounty", () => {
  it("formats null bounty when not found", () => {
    const output = formatBounty(42, null);
    expect(output).toBe("get_bounty(42) =>\n  (no bounty with this ID)");
  });

  it("formats present bounty with JSON formatting and bigints", () => {
    const bounty = {
      creator: "GBORRZIP3OLCOP55TTQWFZ2ADMWIMHUMLV43HDYOHTCSI75SKPWBYQVE",
      amount: 1000000000n,
      claim_count: 1,
    };

    const output = formatBounty(1, bounty);
    expect(output).toBe(
      `get_bounty(1) =>\n{\n  "creator": "GBORRZIP3OLCOP55TTQWFZ2ADMWIMHUMLV43HDYOHTCSI75SKPWBYQVE",\n  "amount": "1000000000",\n  "claim_count": 1\n}`,
    );
  });
});

describe("formatClaim", () => {
  it("formats null claim when not found", () => {
    const output = formatClaim(1, 9, null);
    expect(output).toBe("get_claim(1, 9) =>\n  (no claim with this ID)");
  });

  it("formats present claim with JSON formatting and bigints", () => {
    const claim = {
      claimant: "GBORRZIP3OLCOP55TTQWFZ2ADMWIMHUMLV43HDYOHTCSI75SKPWBYQVE",
      status: "Accepted",
      bounty_id: 1n,
    };

    const output = formatClaim(1, 0, claim);
    expect(output).toBe(
      `get_claim(1, 0) =>\n{\n  "claimant": "GBORRZIP3OLCOP55TTQWFZ2ADMWIMHUMLV43HDYOHTCSI75SKPWBYQVE",\n  "status": "Accepted",\n  "bounty_id": "1"\n}`,
    );
  });
});
