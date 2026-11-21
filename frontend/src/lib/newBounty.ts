/**
 * Input rules for `create_bounty`, mirrored from `contracts/deprader_router`
 * so the form rejects what the contract would reject.
 */

/** Matches `MAX_REF_LEN` in the contract. */
export const MAX_REF_BYTES = 256;
/** Matches `MAX_BOUNTY_DURATION_SECS` in the contract (365 days). */
export const MAX_DURATION_SECS = 365 * 24 * 60 * 60;
/** Stellar amounts have seven decimal places (stroops). */
const STROOPS_PER_XLM = 10_000_000n;
const XLM_PATTERN = /^\d+(\.\d{1,7})?$/;

export interface NewBountyInput {
  /** Finding reference, free text. */
  ref: string;
  /** Reward in XLM, as typed, e.g. `12.5`. */
  amount: string;
  /** Deadline as a `YYYY-MM-DD` calendar date (UTC). */
  deadline: string;
}

export interface NewBountyErrors {
  ref?: string;
  amount?: string;
  deadline?: string;
}

/** A validated bounty, ready to send to the contract. */
export interface NewBountyDraft {
  ref: string;
  amountStroops: bigint;
  /** Unix seconds: the end of the chosen day, UTC. */
  deadlineSecs: number;
}

/** Converts a decimal XLM string to stroops, or `null` if it isn't a valid amount. */
export function xlmToStroops(xlm: string): bigint | null {
  const trimmed = xlm.trim();
  if (!XLM_PATTERN.test(trimmed)) return null;
  const [whole, fraction = ""] = trimmed.split(".");
  return BigInt(whole) * STROOPS_PER_XLM + BigInt(fraction.padEnd(7, "0"));
}

/** Unix seconds for the last second of a `YYYY-MM-DD` day in UTC, or `null` if malformed. */
export function endOfDaySecs(date: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (!m) return null;
  const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const ms = Date.UTC(year, month - 1, day, 23, 59, 59);
  const check = new Date(ms);
  // Reject dates that overflow, such as 2026-02-30.
  if (check.getUTCFullYear() !== year || check.getUTCMonth() !== month - 1 || check.getUTCDate() !== day) return null;
  return Math.floor(ms / 1000);
}

/** Checks every field. Returns the draft only when there are no errors. */
export function validateNewBounty(
  input: NewBountyInput,
  nowSecs: number,
): { errors: NewBountyErrors; draft: NewBountyDraft | null } {
  const errors: NewBountyErrors = {};

  const ref = input.ref.trim();
  const refBytes = new TextEncoder().encode(ref).length;
  if (refBytes === 0) errors.ref = "Enter a finding reference.";
  else if (refBytes > MAX_REF_BYTES) errors.ref = `Must be at most ${MAX_REF_BYTES} bytes (this is ${refBytes}).`;

  const amountStroops = xlmToStroops(input.amount);
  if (amountStroops === null) errors.amount = "Enter an amount with up to 7 decimal places.";
  else if (amountStroops <= 0n) errors.amount = "Amount must be greater than zero.";

  const deadlineSecs = endOfDaySecs(input.deadline);
  if (deadlineSecs === null) errors.deadline = "Pick a valid date.";
  else if (deadlineSecs <= nowSecs) errors.deadline = "Pick a date in the future.";
  else if (deadlineSecs - nowSecs > MAX_DURATION_SECS) errors.deadline = "Deadline can be at most one year away.";

  const valid = Object.keys(errors).length === 0 && amountStroops !== null && deadlineSecs !== null;
  return {
    errors,
    draft: valid ? { ref, amountStroops, deadlineSecs } : null,
  };
}
