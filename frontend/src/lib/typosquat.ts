import { splitIssueRef } from "./format";
import type { Bounty } from "./types";

/** One typosquat signal that fired; mirrors `Signal` in crates/typosquat-wasm. */
export interface Signal {
  kind: "distance" | "keyboard" | "homoglyph" | "digit_swap" | "scope_confusion" | "combosquat";
  detail: string;
}

/** The scanner's verdict for a package; mirrors `Detection` in crates/typosquat-wasm. */
export interface Detection {
  candidate: string;
  flagged: boolean;
  /** 0 (no signal) to 100. */
  confidence: number;
  signals: Signal[];
}

/** The wasm-bindgen exports of `crates/typosquat-wasm` (built with `--target web`). */
interface ScannerModule {
  default: () => Promise<unknown>;
  scan: (name: string) => string;
}

let scanner: Promise<ScannerModule | null> | undefined;

/**
 * Loads the typosquat scorer compiled to WebAssembly from `public/wasm/`
 * (`npm run build:wasm`). Resolves to `null` when the module wasn't built,
 * so the dashboard still works without a Rust toolchain.
 */
function loadScanner(): Promise<ScannerModule | null> {
  scanner ??= import(/* @vite-ignore */ new URL("wasm/typosquat.js", document.baseURI).href)
    .then(async (mod: ScannerModule) => {
      await mod.default();
      return mod;
    })
    .catch(() => null);
  return scanner;
}

/** Scores `name` with the typosquat scanner, or `null` if the scanner isn't available. */
export async function detectPackage(name: string): Promise<Detection | null> {
  const mod = await loadScanner();
  return mod && (JSON.parse(mod.scan(name)) as Detection);
}

const PACKAGE_NAME = /^(@[a-z0-9][\w.~-]*\/)?[a-z0-9][\w.~-]*/i;

/**
 * The package a bounty is about: its structured `package` field when set,
 * otherwise the first token of the issue reference after the ecosystem
 * prefix (`npm:expresss typosquat of express` → `expresss`), without any
 * `@version` suffix.
 */
export function bountyPackage(bounty: Pick<Bounty, "issueRef" | "package">): string | null {
  if (bounty.package) return bounty.package;
  const match = PACKAGE_NAME.exec(splitIssueRef(bounty.issueRef).text.trim());
  return match ? match[0] : null;
}
