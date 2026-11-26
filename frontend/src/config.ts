/**
 * Network configuration. Each preset names an RPC endpoint, network
 * passphrase, router contract and explorer network. Testnet defaults to the
 * live v2 router and can be overridden with `.env.local` (see `.env.example`).
 * Mainnet is hidden until its contract ID is configured.
 */

export interface NetworkPreset {
  id: "testnet" | "mainnet";
  label: string;
  rpcUrl: string;
  passphrase: string;
  /** Router contract ID. `null` means the preset is not configured yet. */
  contractId: string | null;
  /** Native XLM Stellar Asset Contract on this network. `null` if not configured. */
  nativeSac: string | null;
  /** Network name used in stellar.expert links. */
  explorerNetwork: "testnet" | "public";
}

const TESTNET_NATIVE_SAC = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

export const NETWORK_PRESETS: Record<NetworkPreset["id"], NetworkPreset> = {
  testnet: {
    id: "testnet",
    label: "Testnet",
    rpcUrl: import.meta.env.VITE_RPC_URL ?? "https://soroban-testnet.stellar.org",
    passphrase: import.meta.env.VITE_NETWORK_PASSPHRASE ?? "Test SDF Network ; September 2015",
    contractId: import.meta.env.VITE_CONTRACT_ID ?? "CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ",
    nativeSac: TESTNET_NATIVE_SAC,
    explorerNetwork: "testnet",
  },
  mainnet: {
    id: "mainnet",
    label: "Mainnet",
    rpcUrl: import.meta.env.VITE_MAINNET_RPC_URL ?? "https://mainnet.sorobanrpc.com",
    passphrase: "Public Global Stellar Network ; September 2015",
    contractId: import.meta.env.VITE_MAINNET_CONTRACT_ID || null,
    nativeSac: import.meta.env.VITE_MAINNET_NATIVE_SAC || null,
    explorerNetwork: "public",
  },
};

/** Presets the user can pick: mainnet only once its contract ID is configured. */
export function availableNetworks(): NetworkPreset[] {
  return Object.values(NETWORK_PRESETS).filter((p) => p.id === "testnet" || p.contractId !== null);
}

const STORAGE_KEY = "depradar:network";

function storedNetworkId(): string | null {
  try {
    return window.localStorage.getItem(STORAGE_KEY);
  } catch {
    // Storage can be blocked (private mode, disabled site data); use the default.
    return null;
  }
}

/** Saves the chosen network. Returns false if it couldn't be persisted. */
export function saveNetworkChoice(id: NetworkPreset["id"]): boolean {
  try {
    window.localStorage.setItem(STORAGE_KEY, id);
    return true;
  } catch {
    return false;
  }
}

function resolveActive(): NetworkPreset {
  const stored = storedNetworkId();
  const preset = availableNetworks().find((p) => p.id === stored);
  return preset ?? NETWORK_PRESETS.testnet;
}

/** The network this session talks to. Changing it takes effect after a reload. */
export const ACTIVE_NETWORK: NetworkPreset = resolveActive();

export const CONTRACT_ID: string = ACTIVE_NETWORK.contractId ?? "";
export const RPC_URL: string = ACTIVE_NETWORK.rpcUrl;
export const NETWORK: string = ACTIVE_NETWORK.id;
export const NETWORK_PASSPHRASE: string = ACTIVE_NETWORK.passphrase;

export type ExplorerNetwork = "testnet" | "public";

export type ExplorerType = "account" | "contract" | "tx";

/** Returns the base Stellar Expert explorer URL for the network. */
export function explorerBaseUrl(
  network: ExplorerNetwork | NetworkPreset["id"] = ACTIVE_NETWORK.explorerNetwork,
): string {
  const net = network === "mainnet" ? "public" : network;
  return `https://stellar.expert/explorer/${net}`;
}

/** Builds a full Stellar Expert explorer URL for a given resource and network. */
export function buildExplorerUrl(
  type: ExplorerType,
  id: string,
  network: ExplorerNetwork | NetworkPreset["id"] = ACTIVE_NETWORK.explorerNetwork,
): string {
  const base = explorerBaseUrl(network);
  return `${base}/${type}/${id}`;
}

/** Factory that creates explorer URL builders for a specific network. */
export function createExplorerUrls(
  network: ExplorerNetwork | NetworkPreset["id"] = ACTIVE_NETWORK.explorerNetwork,
) {
  return {
    contract: (id: string) => buildExplorerUrl("contract", id, network),
    account: (id: string) => buildExplorerUrl("account", id, network),
    tx: (hash: string) => buildExplorerUrl("tx", hash, network),
  };
}

export const explorer = createExplorerUrls(ACTIVE_NETWORK.explorerNetwork);

/**
 * Native XLM's Stellar Asset Contract on the active network. Empty when a
 * network doesn't have it configured, so XLM amounts show the token address
 * rather than a wrong symbol.
 */
export const NATIVE_XLM_SAC: string = ACTIVE_NETWORK.nativeSac ?? "";
