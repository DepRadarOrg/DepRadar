import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { getAddress, getNetworkDetails, isAllowed, isConnected, requestAccess } from "@stellar/freighter-api";
import { expectedNetwork, type FreighterNetwork } from "./network";

export type WalletStatus = "unavailable" | "disconnected" | "connecting" | "connected";

export interface WalletValue {
  /** `unavailable` when the Freighter extension isn't installed. */
  status: WalletStatus;
  address: string | null;
  /** Network the wallet is on, e.g. `TESTNET`. */
  network: FreighterNetwork | null;
  /** False when the wallet is on a different network from DepRadar's `NETWORK`. */
  networkMatches: boolean;
  error: string | null;
  connect: () => Promise<void>;
  disconnect: () => void;
}

const WalletContext = createContext<WalletValue | null>(null);

/** Reads the Freighter address and network, throwing the wallet's own message on failure. */
async function readWallet(): Promise<{ address: string; network: FreighterNetwork }> {
  const account = await getAddress();
  if (account.error) throw new Error(account.error.message);
  const details = await getNetworkDetails();
  if (details.error) throw new Error(details.error.message);
  return { address: account.address, network: details.network };
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<WalletStatus>("disconnected");
  const [address, setAddress] = useState<string | null>(null);
  const [network, setNetwork] = useState<FreighterNetwork | null>(null);
  const [error, setError] = useState<string | null>(null);

  const apply = useCallback((wallet: { address: string; network: FreighterNetwork }) => {
    setAddress(wallet.address);
    setNetwork(wallet.network);
    setStatus("connected");
  }, []);

  // On load, detect the extension and silently reconnect if this site was already approved.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const installed = await isConnected();
      if (cancelled) return;
      if (!installed.isConnected) {
        setStatus("unavailable");
        return;
      }
      const allowed = await isAllowed();
      if (cancelled || !allowed.isAllowed) return;
      try {
        const wallet = await readWallet();
        if (!cancelled) apply(wallet);
      } catch {
        // Stay disconnected; the user can connect explicitly.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [apply]);

  const connect = useCallback(async () => {
    setError(null);
    setStatus("connecting");
    try {
      const access = await requestAccess();
      if (access.error) throw new Error(access.error.message);
      const details = await getNetworkDetails();
      if (details.error) throw new Error(details.error.message);
      apply({ address: access.address, network: details.network });
    } catch (err) {
      setStatus("disconnected");
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [apply]);

  const disconnect = useCallback(() => {
    setAddress(null);
    setNetwork(null);
    setError(null);
    setStatus("disconnected");
  }, []);

  const value = useMemo<WalletValue>(
    () => ({
      status,
      address,
      network,
      networkMatches: network === null || network === expectedNetwork(),
      error,
      connect,
      disconnect,
    }),
    [status, address, network, error, connect, disconnect],
  );

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}

/** The connected Freighter wallet. Must be used inside `<WalletProvider>`. */
export function useWallet(): WalletValue {
  const value = useContext(WalletContext);
  if (!value) throw new Error("useWallet must be used inside <WalletProvider>");
  return value;
}
