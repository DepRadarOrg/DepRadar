import { useTranslation } from "react-i18next";
import { Link, useParams } from "react-router-dom";
import { ScannerDetection } from "../components/ScannerDetection";
import { AddressLink, CopyButton, Empty, ErrorState, Identicon, Skeleton, StatusBadge } from "../components/ui";
import { NATIVE_XLM_SAC } from "../config";
import { getBounty, listClaims } from "../lib/contract";
import { formatAmount, formatDate, relativeTime, splitIssueRef, tokenSymbol } from "../lib/format";
import { bountyTimeline, type TimelineEvent } from "../lib/insights";
import { bountyPackage } from "../lib/typosquat";
import type { Bounty, Claim } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function BountyDetail() {
  const { t } = useTranslation();
  const idParam = useParams().id;
  const id = Number(idParam);
  const state = useAsync(async () => {
    if (!Number.isInteger(id) || id < 1) return null;
    const bounty = await getBounty(id);
    return bounty && { bounty, claims: await listClaims(bounty) };
  }, `bounty:${idParam}`);

  return (
    <div className="detail">
      <Link to="/" className="back">
        {t("app.backToBoard")}
      </Link>

      {state.status === "loading" && <DetailSkeleton />}
      {state.status === "error" && <ErrorState error={state.error} onRetry={state.reload} />}
      {state.status === "success" && state.data === null && <Empty>{t("app.bountyMissing", { id: idParam })}</Empty>}
      {state.status === "success" && state.data && <Detail bounty={state.data.bounty} claims={state.data.claims} />}
    </div>
  );
}

function Detail({ bounty, claims }: { bounty: Bounty; claims: Claim[] }) {
  const { t } = useTranslation();
  const { ecosystem, text } = splitIssueRef(bounty.issueRef);
  const timeline = bountyTimeline(bounty, claims);
  const packageName = bountyPackage(bounty);

  return (
    <>
      <header className={`detail-hero status-${bounty.status.toLowerCase()}`}>
        <div className="detail-hero-main">
          <div className="card-top">
            <span className="card-id mono">{t("bounty.id", { id: bounty.id })}</span>
            {ecosystem && <span className="eco">{ecosystem}</span>}
            <StatusBadge status={bounty.status} />
          </div>
          <h1>{text}</h1>
          <p className="muted">
            {t("bounty.fundedBy", { address: "", date: "" })}
            <AddressLink address={bounty.creator} avatar /> {formatDate(bounty.createdAt)}
          </p>
        </div>
        <div className="detail-reward">
          <span className="reward-label">{t("bounty.reward")}</span>
          <span className="reward-amount">{formatAmount(bounty.amount)}</span>
          <span className="reward-token">{tokenSymbol(bounty.token)}</span>
        </div>
      </header>

      <div className="detail-grid">
        <section>
          <h2>{t("bounty.timeline")}</h2>
          <ol className="timeline">
            {timeline.map((e, i) => (
              <TimelineItem key={i} event={e} />
            ))}
          </ol>
        </section>

        <aside className="facts">
          <h2>{t("bounty.details")}</h2>
          <dl>
            <div>
              <dt>{t("bounty.status")}</dt>
              <dd>
                <StatusBadge status={bounty.status} />
              </dd>
            </div>
            <div>
              <dt>{t("bounty.claims")}</dt>
              <dd>{t("bounty.claimsSummary", { total: bounty.claimCount, review: bounty.pendingClaims })}</dd>
            </div>
            <div>
              <dt>{t("bounty.claimsClose")}</dt>
              <dd>
                {formatDate(bounty.deadline)}
                {bounty.status === "Open" && <span className="muted"> · {relativeTime(bounty.deadline)}</span>}
              </dd>
            </div>
            <div>
              <dt>{t("bounty.token")}</dt>
              <dd>
                {tokenSymbol(bounty.token)}
                {bounty.token === NATIVE_XLM_SAC ? <span className="muted"> · {t("bounty.native")}</span> : null}{" "}
                <AddressLink address={bounty.token} />
              </dd>
            </div>
            <div>
              <dt>{t("bounty.findingRef")}</dt>
              <dd className="ref">
                <code>{bounty.issueRef}</code>
                <CopyButton value={bounty.issueRef} />
              </dd>
            </div>
          </dl>
          {packageName && <ScannerDetection name={packageName} />}
        </aside>
      </div>
    </>
  );
}

function TimelineItem({ event }: { event: TimelineEvent }) {
  const { t } = useTranslation();
  const tone = event.claim ? event.claim.status.toLowerCase() : event.kind;
  return (
    <li className={`tl tl-${tone} ${event.upcoming ? "tl-upcoming" : ""}`}>
      <span className="tl-dot" aria-hidden />
      <div className="tl-body">
        <div className="tl-head">
          <strong>{event.title}</strong>
          {event.claim && event.kind === "claim" && <StatusBadge status={event.claim.status} />}
        </div>
        {event.at !== undefined && (
          <span className="muted tl-time">
            {formatDate(event.at)} · {relativeTime(event.at)}
          </span>
        )}
        {event.claim && event.kind === "claim" && (
          <div className="claim-box">
            <Identicon address={event.claim.claimant} />
            <div>
              <AddressLink address={event.claim.claimant} />
              <p className="proof">{event.claim.proofRef}</p>
            </div>
          </div>
        )}
        {event.kind === "paid" && event.claim && (
          <p className="muted">
            {t("bounty.sentTo", { address: "" })}
            <AddressLink address={event.claim.claimant} avatar />
          </p>
        )}
      </div>
    </li>
  );
}

function DetailSkeleton() {
  return (
    <div aria-hidden>
      <div className="detail-hero">
        <div className="detail-hero-main">
          <Skeleton className="sk-sm" />
          <Skeleton className="sk-h1" />
          <Skeleton className="sk-title" />
        </div>
        <Skeleton className="sk-reward" />
      </div>
      <div className="detail-grid">
        <div>
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="sk-block" />
          ))}
        </div>
        <Skeleton className="sk-block sk-tall" />
      </div>
    </div>
  );
}
