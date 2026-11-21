import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Address, nativeToScVal } from "@stellar/stellar-sdk";
import { getClaimId } from "../lib/contract";
import { formatDate } from "../lib/format";
import { MAX_REF_BYTES } from "../lib/newBounty";
import type { Bounty } from "../lib/types";
import { invokeContract } from "../lib/tx";
import { useAsync } from "../lib/useAsync";
import { useWallet } from "../wallet/WalletContext";
import { WalletButton } from "../wallet/WalletButton";

/**
 * Claim form for an open bounty. It explains why it's disabled when the
 * wallet is not connected, is the creator, the deadline has passed, or the
 * wallet already has a claim on this bounty.
 */
export function SubmitClaim({ bounty, onSubmitted }: { bounty: Bounty; onSubmitted: () => void }) {
  const { t } = useTranslation();
  const wallet = useWallet();
  const address = wallet.status === "connected" ? wallet.address : null;
  const existing = useAsync(
    async () => (address ? getClaimId(bounty.id, address) : null),
    `claim-id:${bounty.id}:${address ?? ""}`,
  );
  const [proof, setProof] = useState("");
  const [proofError, setProofError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const now = Math.floor(Date.now() / 1000);
  let blocked: string | null = null;
  if (!address) blocked = t("claim.connectFirst");
  else if (!wallet.networkMatches) blocked = t("claim.networkWarning");
  else if (address === bounty.creator) blocked = t("claim.isCreator");
  else if (bounty.deadline <= now) blocked = t("claim.closed", { date: formatDate(bounty.deadline) });
  else if (existing.status === "loading") blocked = t("claim.checking");
  else if (existing.status === "error") blocked = existing.error.message;
  else if (existing.data !== null) blocked = t("claim.alreadyClaimed", { id: existing.data });

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!address) return;
    const ref = proof.trim();
    const bytes = new TextEncoder().encode(ref).length;
    if (bytes === 0) {
      setProofError(t("claim.proofRequired"));
      return;
    }
    if (bytes > MAX_REF_BYTES) {
      setProofError(t("claim.proofTooLong", { max: MAX_REF_BYTES }));
      return;
    }
    setProofError(null);
    setFailure(null);
    setSubmitting(true);
    try {
      await invokeContract(address, "submit_claim", [
        nativeToScVal(bounty.id, { type: "u64" }),
        new Address(address).toScVal(),
        nativeToScVal(ref, { type: "string" }),
      ]);
      onSubmitted();
    } catch (err) {
      setFailure(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section className="claim-form" aria-labelledby="submit-claim-title">
      <h2 id="submit-claim-title">{t("claim.title")}</h2>
      {blocked ? (
        <>
          <p className="muted">{blocked}</p>
          {!address && <WalletButton />}
        </>
      ) : (
        <form className="form" onSubmit={submit} noValidate>
          <div className="field">
            <label className="field-label" htmlFor="claim-proof">
              {t("claim.proof")}
            </label>
            <input
              id="claim-proof"
              value={proof}
              maxLength={1024}
              aria-invalid={Boolean(proofError)}
              aria-describedby={proofError ? "claim-proof-error" : undefined}
              onChange={(e) => {
                setProof(e.target.value);
                setProofError(null);
              }}
            />
            {proofError && (
              <span id="claim-proof-error" className="field-error">
                {proofError}
              </span>
            )}
          </div>
          <div className="form-actions">
            <button type="submit" className="btn" disabled={submitting}>
              {submitting ? t("claim.submitting") : t("claim.submit")}
            </button>
            {failure && (
              <span className="field-error" role="alert">
                {failure}
              </span>
            )}
          </div>
        </form>
      )}
    </section>
  );
}
