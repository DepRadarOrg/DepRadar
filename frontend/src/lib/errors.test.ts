import { describe, expect, it } from "vitest";
import { friendlyError } from "./errors";

describe("friendlyError", () => {
  it.each(["Failed to fetch", "Network Error", "NetworkError when attempting to fetch resource.", "connect ECONNREFUSED"])(
    "maps %j to a network message",
    (message) => {
      expect(friendlyError(new Error(message)).title).toBe("Can't reach the Stellar network.");
    },
  );

  it.each(["Request failed with status code 429", "Too Many Requests", "rate limit exceeded"])(
    "maps %j to a rate-limit message",
    (message) => {
      expect(friendlyError(new Error(message)).title).toBe("The Stellar RPC is rate limiting requests.");
    },
  );

  it.each([
    "contract not found",
    "HostError: Error(Storage, MissingValue)",
    "trying to get non-existing value for contract instance",
  ])("maps %j to a missing-contract message", (message) => {
    expect(friendlyError(new Error(message)).title).toBe("The bounty contract wasn't found.");
  });

  it("keeps the raw message for unrecognised failures", () => {
    expect(friendlyError(new Error("Unexpected enum value: 7"))).toEqual({
      title: "Couldn't read the contract.",
      detail: "Unexpected enum value: 7",
    });
  });
});
