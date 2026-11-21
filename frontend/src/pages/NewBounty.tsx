import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { Address, nativeToScVal, xdr } from "@stellar/stellar-sdk";
import { NATIVE_XLM_SAC } from "../config";
import { invokeContract } from "../lib/tx";
import { MAX_DURATION_SECS, validateNewBounty, type NewBountyErrors, type NewBountyInput } from "../lib/newBounty";
import { useWallet } from "../wallet/WalletContext";
import { WalletButton } from "../wallet/WalletButton";

/** Soroban's `None` for an `Option<T>` argument. */
const NONE = xdr.ScVal.scvVoid();

/** Calendar date `days` from now, as `YYYY-MM-DD` in UTC. */
function isoDateFromNow(days: number): string {
  return new Date(Date.now() + days * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
}

export function NewBounty() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const wallet = useWallet();
  const [form, setForm] = useState<NewBountyInput>({ ref: "", amount: "", deadline: isoDateFromNow(30) });
  const [errors, setErrors] = useState<NewBountyErrors>({});
  const [submitting, setSubmitting] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  if (wallet.status !== "connected" || !wallet.address) {
    return (
      <div className="detail">
        <h1>{t("newBounty.title")}</h1>
        <p className="muted">{t("newBounty.connectFirst")}</p>
        <WalletButton />
      </div>
    );
  }
  const creator = wallet.address;

  function update(field: keyof NewBountyInput, value: string) {
    setForm((f) => ({ ...f, [field]: value }));
    setErrors((e) => ({ ...e, [field]: undefined }));
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setFailure(null);
    const { errors: found, draft } = validateNewBounty(form, Math.floor(Date.now() / 1000));
    setErrors(found);
    if (!draft) return;

    setSubmitting(true);
    try {
      const { value } = await invokeContract(creator, "create_bounty", [
        new Address(creator).toScVal(),
        nativeToScVal(draft.ref, { type: "string" }),
        NONE, // ecosystem
        NONE, // package
        NONE, // severity
        nativeToScVal(draft.amountStroops, { type: "i128" }),
        new Address(NATIVE_XLM_SAC).toScVal(),
        nativeToScVal(draft.deadlineSecs, { type: "u64" }),
        nativeToScVal(0, { type: "u32" }), // drips_share_bps
        NONE, // drips_destination
      ]);
      navigate(`/bounty/${Number(value)}`);
    } catch (err) {
      setFailure(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="detail">
      <h1>{t("newBounty.title")}</h1>
      <p className="muted">{t("newBounty.lede")}</p>

      {!wallet.networkMatches && (
        <p className="wallet-warn" role="alert">
          {t("newBounty.networkWarning")}
        </p>
      )}

      <form className="form" onSubmit={submit} noValidate>
        <div className="field">
          <label className="field-label" htmlFor="new-bounty-ref">
            {t("newBounty.ref")}
          </label>
          <input
            id="new-bounty-ref"
            value={form.ref}
            maxLength={1024}
            aria-invalid={Boolean(errors.ref)}
            aria-describedby={errors.ref ? "new-bounty-ref-error" : undefined}
            onChange={(e) => update("ref", e.target.value)}
          />
          {errors.ref && (
            <span id="new-bounty-ref-error" className="field-error">
              {errors.ref}
            </span>
          )}
        </div>

        <div className="field">
          <label className="field-label" htmlFor="new-bounty-amount">
            {t("newBounty.amount")}
          </label>
          <input
            id="new-bounty-amount"
            inputMode="decimal"
            placeholder="0.0000000"
            value={form.amount}
            aria-invalid={Boolean(errors.amount)}
            aria-describedby={errors.amount ? "new-bounty-amount-error" : undefined}
            onChange={(e) => update("amount", e.target.value)}
          />
          {errors.amount && (
            <span id="new-bounty-amount-error" className="field-error">
              {errors.amount}
            </span>
          )}
        </div>

        <div className="field">
          <label className="field-label" htmlFor="new-bounty-deadline">
            {t("newBounty.deadline")}
          </label>
          <input
            id="new-bounty-deadline"
            type="date"
            min={isoDateFromNow(0)}
            max={isoDateFromNow(Math.floor(MAX_DURATION_SECS / 86400))}
            value={form.deadline}
            aria-invalid={Boolean(errors.deadline)}
            aria-describedby={errors.deadline ? "new-bounty-deadline-error" : undefined}
            onChange={(e) => update("deadline", e.target.value)}
          />
          {errors.deadline && (
            <span id="new-bounty-deadline-error" className="field-error">
              {errors.deadline}
            </span>
          )}
        </div>

        <div className="form-actions">
          <button type="submit" className="btn" disabled={submitting || !wallet.networkMatches}>
            {submitting ? t("newBounty.submitting") : t("newBounty.submit")}
          </button>
          {failure && (
            <span className="field-error" role="alert">
              {failure}
            </span>
          )}
        </div>
      </form>
    </div>
  );
}
