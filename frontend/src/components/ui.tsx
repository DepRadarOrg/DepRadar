import { useState, type ReactNode } from "react";
import { explorer } from "../config";
import { friendlyError } from "../lib/errors";
import { shortAddress } from "../lib/format";
import { BOUNTY_LABEL, CLAIM_LABEL } from "../lib/insights";
import type { BountyStatus, ClaimStatus } from "../lib/types";

const LABELS: Record<string, string> = { ...BOUNTY_LABEL, ...CLAIM_LABEL };

export function StatusBadge({ status }: { status: BountyStatus | ClaimStatus }) {
  return (
    <span className={`badge badge-${status.toLowerCase()}`}>
      <span className="badge-dot" aria-hidden />
      {LABELS[status] ?? status}
    </span>
  );
}

/** Deterministic two-tone avatar for a Stellar address. */
export function Identicon({ address, size = 28 }: { address: string; size?: number }) {
  let h = 0;
  for (const ch of address) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  const hue = h % 360;
  return (
    <span
      className="identicon"
      aria-hidden
      style={{
        width: size,
        height: size,
        background: `linear-gradient(135deg, hsl(${hue} 70% 55%), hsl(${(hue + 60) % 360} 70% 35%))`,
      }}
    />
  );
}

export function AddressLink({ address, avatar = false, copy = false }: { address: string; avatar?: boolean; copy?: boolean }) {
  const href = address.startsWith("C") ? explorer.contract(address) : explorer.account(address);
  return (
    <span className="address-wrap">
      <a className="address" href={href} target="_blank" rel="noreferrer" title={address}>
        {avatar && <Identicon address={address} size={20} />}
        <span className="mono">{shortAddress(address)}</span>
      </a>
      {copy && <CopyButton value={address} />}
    </span>
  );
}

export function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="copy"
      aria-label="Copy to clipboard"
      onClick={() => {
        navigator.clipboard?.writeText(value).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
}

export function Skeleton({ className = "" }: { className?: string }) {
  return <span className={`skeleton ${className}`} aria-hidden />;
}

export function Loading({ label = "Scanning testnet…" }: { label?: string }) {
  return (
    <div className="state" role="status">
      <span className="spinner" aria-hidden /> {label}
    </div>
  );
}

export function ErrorState({ error, onRetry }: { error: Error; onRetry?: () => void }) {
  const { title, detail } = friendlyError(error);
  return (
    <div className="state state-error" role="alert">
      <strong>{title}</strong>
      <span className="muted">{detail}</span>
      {onRetry && (
        <button type="button" className="btn" onClick={onRetry}>
          Retry
        </button>
      )}
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="state muted">{children}</div>;
}
