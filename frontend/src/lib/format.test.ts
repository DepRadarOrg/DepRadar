import { describe, expect, it } from "vitest";
import { NATIVE_XLM_SAC } from "../config";
import {
  formatAmount,
  formatIssueRef,
  parseIssueRef,
  relativeTime,
  shortAddress,
  tokenSymbol,
  type IssueRef,
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

describe("issue refs", () => {
  const cases: IssueRef[] = [
    { ecosystem: "npm", package: "expresss", version: "1.0.0", kind: "typosquat", detail: "express" },
    { ecosystem: "npm", package: "@babel/core", version: "7.0.0", kind: "deprecated", detail: null },
    { ecosystem: "npm", package: "@babel/core", version: null, kind: "typosquat", detail: null },
    { ecosystem: "crates.io", package: "serde", version: "1.0.0", kind: "license", detail: "MIT OR Apache-2.0" },
    {
      ecosystem: "maven",
      package: "org.apache.logging.log4j:log4j-core",
      version: "2.14.1",
      kind: "vulnerability",
      detail: "GHSA-jfh8-c2jp-5v3q",
    },
  ];

  it("formats the documented example", () => {
    expect(formatIssueRef(cases[0])).toBe("npm:expresss@1.0.0#typosquat:express");
  });
  it("round-trips", () => {
    for (const ref of cases) expect(parseIssueRef(formatIssueRef(ref))).toEqual(ref);
  });
  it("parses the ecosystem case-insensitively", () => {
    expect(parseIssueRef("PyPI:flask#deprecated")?.ecosystem).toBe("pypi");
  });
  it("rejects free-form and malformed refs", () => {
    for (const ref of [
      "npm:expresss typosquat of express (demo)",
      "org/repo#123",
      "cargo:serde#typosquat",
      "npm:#typosquat",
      "npm:lodash@#vulnerability",
      "npm:lodash@4.17.0#vulnerability:",
      "npm:lodash@4.17.0#unknown",
    ]) {
      expect(parseIssueRef(ref)).toBeNull();
    }
  });
});
