import { NETWORK } from "../config";

/** Freighter's name for a network, e.g. `TESTNET` or `PUBLIC`. */
export type FreighterNetwork = string;

const PASSPHRASES: Record<string, string> = {
  TESTNET: "Test SDF Network ; September 2015",
  PUBLIC: "Public Global Stellar Network ; September 2015",
};

/** The network DepRadar is configured for, named the way Freighter names it. */
export function expectedNetwork(): FreighterNetwork {
  const name = NETWORK.toLowerCase();
  return name === "mainnet" || name === "public" ? "PUBLIC" : name.toUpperCase();
}

/** Passphrase used to sign transactions for the configured network. */
export function networkPassphrase(): string {
  const passphrase = PASSPHRASES[expectedNetwork()];
  if (!passphrase) throw new Error(`Signing is not supported on ${NETWORK}.`);
  return passphrase;
}
