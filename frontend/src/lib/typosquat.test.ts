import { describe, expect, it } from "vitest";
import { bountyPackage } from "./typosquat";

describe("bountyPackage", () => {
  it.each([
    ["npm:expresss typosquat of express (demo)", "expresss"],
    ["npm: lodash-utils", "lodash-utils"],
    ["pypi:reqeusts==2.0", "reqeusts"],
    ["npm:@typess/node@1.0.0 scope confusion", "@typess/node"],
    ["crates:serde_jsonn", "serde_jsonn"],
    ["left-pad", "left-pad"],
  ])("parses %j as %j", (issueRef, name) => {
    expect(bountyPackage({ issueRef })).toBe(name);
  });

  it("prefers the structured package field", () => {
    expect(bountyPackage({ issueRef: "npm:something else", package: "expresss" })).toBe("expresss");
  });

  it("returns null when the reference names no package", () => {
    expect(bountyPackage({ issueRef: "npm: (see report)" })).toBeNull();
  });
});
