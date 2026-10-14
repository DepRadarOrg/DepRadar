import { describe, expect, it } from "vitest";
import { NATIVE_XLM_SAC } from "../config";
import {
  formatAmount,
  formatIssueRef,
  parseIssueRef,
  relativeTime,
  shortAddress,
  tokenSymbol,
} from "./format";

describe("formatAmount", () => {
  it("formats whole stroop amounts as XLM", () => {
    expect(formatAmount(50_000_000n)).toBe("5");
  });
  it("keeps significant decimals and trims trailing zeros", () => {
    expect(formatAmount(52_500_000n)).toBe("5.25");
    expect(formatAmount(1n)).toBe("0.0000001");
  });
  it("groups thousands", () => {
    expect(formatAmount(12_345_670_000_000n)).toBe("1,234,567");
  });
  it("handles zero and negatives", () => {
    expect(formatAmount(0n)).toBe("0");
    expect(formatAmount(-25_000_000n)).toBe("-2.5");
  });
});

describe("shortAddress", () => {
  it("shortens long Stellar addresses", () => {
    expect(shortAddress("GCFFP6RU6C3JOZBIUNZN2ELJAITUZ3HWEOUUCFXIAHE6M2GHVHZB4D6A")).toBe("GCFFP6…4D6A");
  });
  it("leaves short strings alone", () => {
    expect(shortAddress("GABC")).toBe("GABC");
  });
});

describe("tokenSymbol", () => {
  it("labels the native XLM asset contract", () => {
    expect(tokenSymbol(NATIVE_XLM_SAC)).toBe("XLM");
  });
  it("falls back to a short contract ID for other tokens", () => {
    expect(tokenSymbol("CABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVW")).toBe("CABCDE…TUVW");
  });
});

describe("relativeTime", () => {
  const now = 1_000_000;
  it("describes future deadlines", () => {
    expect(relativeTime(now + 3 * 86_400, now)).toBe("in 3 days");
  });
  it("describes past times", () => {
    expect(relativeTime(now - 2 * 3_600, now)).toBe("2 hours ago");
  });
});

describe("issue_ref standard parsing & formatting", () => {
  it("round-trips full issue_ref with version and advisory", () => {
    const original = {
      ecosystem: "npm",
      package: "expresss",
      version: "1.0.0",
      kind: "typosquat",
      advisory: "express",
    };
    const formatted = formatIssueRef(original);
    expect(formatted).toBe("npm:expresss@1.0.0#typosquat:express");
    expect(parseIssueRef(formatted)).toEqual(original);
  });

  it("round-trips scoped package with CVE identifier", () => {
    const original = {
      ecosystem: "npm",
      package: "@fastify/deepmerge",
      version: "1.3.0",
      kind: "cve",
      advisory: "GHSA-9265-m339-ch94",
    };
    const formatted = formatIssueRef(original);
    expect(formatted).toBe("npm:@fastify/deepmerge@1.3.0#cve:GHSA-9265-m339-ch94");
    expect(parseIssueRef(formatted)).toEqual(original);
  });

  it("round-trips minimal ref without version and advisory", () => {
    const original = {
      ecosystem: "pypi",
      package: "requests",
      version: null,
      kind: "malicious",
      advisory: null,
    };
    const formatted = formatIssueRef(original);
    expect(formatted).toBe("pypi:requests#malicious");
    expect(parseIssueRef(formatted)).toEqual(original);
  });

  it("returns null for malformed references", () => {
    expect(parseIssueRef("")).toBeNull();
    expect(parseIssueRef("   ")).toBeNull();
    expect(parseIssueRef("npm-express")).toBeNull();
    expect(parseIssueRef("npm:express")).toBeNull();
    expect(parseIssueRef(":express#cve")).toBeNull();
    expect(parseIssueRef("npm:#cve")).toBeNull();
  });
});
