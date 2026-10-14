import { NATIVE_XLM_SAC } from "../config";

/** Stellar assets use 7 decimal places. */
const DECIMALS = 7n;
const SCALE = 10n ** DECIMALS;

/** Formats a raw token amount (e.g. 50000000n stroops) as "5" or "5.25". */
export function formatAmount(raw: bigint): string {
  const negative = raw < 0n;
  const abs = negative ? -raw : raw;
  const whole = abs / SCALE;
  const frac = (abs % SCALE).toString().padStart(Number(DECIMALS), "0").replace(/0+$/, "");
  return `${negative ? "-" : ""}${whole.toLocaleString("en-US")}${frac ? `.${frac}` : ""}`;
}

export function tokenSymbol(token: string): string {
  return token === NATIVE_XLM_SAC ? "XLM" : shortAddress(token);
}

/** "GCFFP6…4D6A" */
export function shortAddress(address: string, keep = 6): string {
  return address.length <= keep * 2 + 1 ? address : `${address.slice(0, keep)}…${address.slice(-4)}`;
}

export function formatDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** "in 12 days" / "3 hours ago", relative to `now` (unix seconds). */
export function relativeTime(unixSeconds: number, now = Date.now() / 1000): string {
  const diff = unixSeconds - now;
  const abs = Math.abs(diff);
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ["day", 86_400],
    ["hour", 3_600],
    ["minute", 60],
  ];
  const rtf = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  for (const [unit, secs] of units) {
    if (abs >= secs) return rtf.format(Math.round(diff / secs), unit);
  }
  return rtf.format(Math.round(diff), "second");
}

/** Splits `npm:expresss typosquat of express` into ecosystem and description. */
export function splitIssueRef(ref: string): { ecosystem: string | null; text: string } {
  const m = /^([a-z0-9.-]{2,12}):\s*(.+)$/i.exec(ref);
  return m ? { ecosystem: m[1].toLowerCase(), text: m[2] } : { ecosystem: null, text: ref };
}

export interface FindingRef {
  ecosystem: string;
  package: string;
  version: string | null;
  kind: string;
  advisory: string | null;
}

/**
 * Formats a `FindingRef` into standard format:
 * `<ecosystem>:<package>[@<version>]#<kind>[:<advisory>]`
 */
export function formatIssueRef(finding: FindingRef): string {
  let out = `${finding.ecosystem.toLowerCase()}:${finding.package}`;
  if (finding.version) {
    out += `@${finding.version}`;
  }
  out += `#${finding.kind.toLowerCase()}`;
  if (finding.advisory) {
    out += `:${finding.advisory}`;
  }
  return out;
}

/**
 * Parses a standard finding reference:
 * `<ecosystem>:<package>[@<version>]#<kind>[:<advisory>]`
 */
export function parseIssueRef(input: string): FindingRef | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  const colonIdx = trimmed.indexOf(":");
  if (colonIdx <= 0) return null;

  const ecosystem = trimmed.slice(0, colonIdx).trim().toLowerCase();
  const rest = trimmed.slice(colonIdx + 1);

  const hashIdx = rest.indexOf("#");
  if (hashIdx <= 0) return null;

  const pkgVer = rest.slice(0, hashIdx).trim();
  const kindAdv = rest.slice(hashIdx + 1).trim();

  let packageStr = pkgVer;
  let version: string | null = null;
  const lastAt = pkgVer.lastIndexOf("@");
  if (lastAt > 0) {
    packageStr = pkgVer.slice(0, lastAt).trim();
    version = pkgVer.slice(lastAt + 1).trim() || null;
  }

  if (!packageStr) return null;

  let kind = kindAdv;
  let advisory: string | null = null;
  const advColon = kindAdv.indexOf(":");
  if (advColon >= 0) {
    kind = kindAdv.slice(0, advColon).trim().toLowerCase();
    advisory = kindAdv.slice(advColon + 1).trim() || null;
  } else {
    kind = kindAdv.toLowerCase();
  }

  if (!kind) return null;

  return {
    ecosystem,
    package: packageStr,
    version,
    kind,
    advisory,
  };
}
