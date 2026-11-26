import { describe, expect, it } from "vitest";
import {
  ACTIVE_NETWORK,
  buildExplorerUrl,
  createExplorerUrls,
  explorer,
  explorerBaseUrl,
} from "./config";

describe("Stellar Expert URL builder", () => {
  const TEST_ACCOUNT = "GCFFP6RU6C3JOZBIUNZN2ELJAITUZ3HWEOUUCFXIAHE6M2GHVHZB4D6A";
  const TEST_CONTRACT = "CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ";
  const TEST_TX = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

  describe("explorerBaseUrl", () => {
    it("returns testnet URL for testnet", () => {
      expect(explorerBaseUrl("testnet")).toBe("https://stellar.expert/explorer/testnet");
    });

    it("returns public URL for public", () => {
      expect(explorerBaseUrl("public")).toBe("https://stellar.expert/explorer/public");
    });

    it("maps mainnet identifier to public explorer network", () => {
      expect(explorerBaseUrl("mainnet")).toBe("https://stellar.expert/explorer/public");
    });

    it("defaults to active network", () => {
      expect(explorerBaseUrl()).toBe(
        `https://stellar.expert/explorer/${ACTIVE_NETWORK.explorerNetwork}`,
      );
    });
  });

  describe("buildExplorerUrl per network", () => {
    it("builds testnet account URL", () => {
      expect(buildExplorerUrl("account", TEST_ACCOUNT, "testnet")).toBe(
        `https://stellar.expert/explorer/testnet/account/${TEST_ACCOUNT}`,
      );
    });

    it("builds testnet contract URL", () => {
      expect(buildExplorerUrl("contract", TEST_CONTRACT, "testnet")).toBe(
        `https://stellar.expert/explorer/testnet/contract/${TEST_CONTRACT}`,
      );
    });

    it("builds testnet transaction URL", () => {
      expect(buildExplorerUrl("tx", TEST_TX, "testnet")).toBe(
        `https://stellar.expert/explorer/testnet/tx/${TEST_TX}`,
      );
    });

    it("builds public (mainnet) account URL", () => {
      expect(buildExplorerUrl("account", TEST_ACCOUNT, "public")).toBe(
        `https://stellar.expert/explorer/public/account/${TEST_ACCOUNT}`,
      );
      expect(buildExplorerUrl("account", TEST_ACCOUNT, "mainnet")).toBe(
        `https://stellar.expert/explorer/public/account/${TEST_ACCOUNT}`,
      );
    });

    it("builds public (mainnet) contract URL", () => {
      expect(buildExplorerUrl("contract", TEST_CONTRACT, "public")).toBe(
        `https://stellar.expert/explorer/public/contract/${TEST_CONTRACT}`,
      );
      expect(buildExplorerUrl("contract", TEST_CONTRACT, "mainnet")).toBe(
        `https://stellar.expert/explorer/public/contract/${TEST_CONTRACT}`,
      );
    });

    it("builds public (mainnet) transaction URL", () => {
      expect(buildExplorerUrl("tx", TEST_TX, "public")).toBe(
        `https://stellar.expert/explorer/public/tx/${TEST_TX}`,
      );
    });
  });

  describe("createExplorerUrls factory", () => {
    it("creates scoped explorer helpers for testnet", () => {
      const exp = createExplorerUrls("testnet");
      expect(exp.account(TEST_ACCOUNT)).toBe(
        `https://stellar.expert/explorer/testnet/account/${TEST_ACCOUNT}`,
      );
      expect(exp.contract(TEST_CONTRACT)).toBe(
        `https://stellar.expert/explorer/testnet/contract/${TEST_CONTRACT}`,
      );
      expect(exp.tx(TEST_TX)).toBe(
        `https://stellar.expert/explorer/testnet/tx/${TEST_TX}`,
      );
    });

    it("creates scoped explorer helpers for mainnet", () => {
      const exp = createExplorerUrls("mainnet");
      expect(exp.account(TEST_ACCOUNT)).toBe(
        `https://stellar.expert/explorer/public/account/${TEST_ACCOUNT}`,
      );
      expect(exp.contract(TEST_CONTRACT)).toBe(
        `https://stellar.expert/explorer/public/contract/${TEST_CONTRACT}`,
      );
      expect(exp.tx(TEST_TX)).toBe(
        `https://stellar.expert/explorer/public/tx/${TEST_TX}`,
      );
    });

    it("default explorer export is configured for active network", () => {
      const expectedBase = `https://stellar.expert/explorer/${ACTIVE_NETWORK.explorerNetwork}`;
      expect(explorer.account(TEST_ACCOUNT)).toBe(`${expectedBase}/account/${TEST_ACCOUNT}`);
      expect(explorer.contract(TEST_CONTRACT)).toBe(`${expectedBase}/contract/${TEST_CONTRACT}`);
      expect(explorer.tx(TEST_TX)).toBe(`${expectedBase}/tx/${TEST_TX}`);
    });
  });
});
