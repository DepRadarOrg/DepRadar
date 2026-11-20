import { describe, expect, it } from "vitest";
import { NATIVE_XLM_SAC } from "../config";
import {
  formatAmount,
  formatIssueRef,
  parseIssueRef,
  refToUrl,
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

describe("refToUrl", () => {
  describe("https URLs - returned as-is", () => {
    it("returns simple https URL as-is", () => {
      const u = "https://example.com";
      expect(refToUrl(u)).toBe(u);
    });

    it("returns https URL with path, query, fragment as-is", () => {
      const u = "https://example.com/foo/bar?x=1&y=2#frag";
      expect(refToUrl(u)).toBe(u);
    });

    it("returns https URL with port, subdomain, trailing slash", () => {
      expect(refToUrl("https://sub.example.com/")).toBe("https://sub.example.com/");
      expect(refToUrl("https://example.com:8443/a")).toBe("https://example.com:8443/a");
    });

    it("accepts uppercase host but preserves input", () => {
      const u = "https://EXAMPLE.COM/Path";
      expect(refToUrl(u)).toBe(u);
    });
  });

  describe("github issue refs - owner/repo#N", () => {
    it.each([
      ["DepRadarOrg/DepRadar#12", "https://github.com/DepRadarOrg/DepRadar/issues/12"],
      ["facebook/react#123", "https://github.com/facebook/react/issues/123"],
      ["vercel/next.js#9999", "https://github.com/vercel/next.js/issues/9999"],
      ["my-org/my-repo#1", "https://github.com/my-org/my-repo/issues/1"],
      ["owner_1/repo_2#42", "https://github.com/owner_1/repo_2/issues/42"],
      ["a/b#1", "https://github.com/a/b/issues/1"],
      ["123/456#789", "https://github.com/123/456/issues/789"],
      ["Owner-Name/Repo_Name.js#100", "https://github.com/Owner-Name/Repo_Name.js/issues/100"],
    ])("converts %s -> %s", (input, expected) => {
      expect(refToUrl(input)).toBe(expected);
    });
  });

  describe("rejects non-https schemes", () => {
    it.each([
      "http://example.com",
      "http://example.com/foo#1",
      "ftp://example.com/file",
      "data:text/html,<h1>hi</h1>",
      "data:text/html;base64,PGgxPmhpPC9oMT4=",
      "file:///etc/passwd",
      "blob:https://example.com/uuid",
    ])("returns null for %s", (input) => {
      expect(refToUrl(input)).toBeNull();
    });
  });

  describe("XSS vectors", () => {
    it.each([
      "javascript:alert(1)",
      "javascript:alert('xss')",
      "javascript:alert(\"xss\")",
      "JaVaScRiPt:alert(1)",
      "JAVASCRIPT:alert(1)",
      "javascript:void(0)",
      "data:text/html,<script>alert(1)</script>",
      "vbscript:msgbox(1)",
    ])("returns null for XSS vector %s", (input) => {
      expect(refToUrl(input)).toBeNull();
    });
  });

  describe("whitespace, empty, malformed", () => {
    it.each([
      "",
      " ",
      "   ",
      "\n",
      "\t",
      "\n\t ",
      " https://example.com",
      "https://example.com ",
      "  https://example.com  ",
      "https://exa mple.com",
      "owner /repo#1",
      "owner/ repo#1",
      "owner/repo #1",
      "owner/repo# 1",
      "facebook/react #123",
    ])("returns null for whitespace case %j", (input) => {
      expect(refToUrl(input)).toBeNull();
    });

    it.each([
      "://",
      "https://",
      "https:/example.com",
      "https:example.com",
      "https ://example.com",
      "not a url",
      "hello",
      "#123",
      "owner/repo",
      "owner/repo#",
      "owner/repo#0",
      "owner/repo#00",
      "owner/repo#abc",
      "owner/repo#1.5",
      "owner/repo#-1",
      "/repo#1",
      "owner/#1",
      "/#1",
      "owner//repo#1",
      "owner/repo#1#2",
      "owner/repo/issues/1",
      "owner/repo!#1",
      "owner/repo@#1",
      "owner/repo$#1",
      "own er/repo#1",
      "owner/re po#1",
      "owner:repo#1",
    ])("returns null for malformed %s", (input) => {
      expect(refToUrl(input)).toBeNull();
    });
  });
});
