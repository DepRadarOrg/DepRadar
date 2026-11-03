import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";
import { formatAmount, relativeTime, splitIssueRef, tokenSymbol } from "../lib/format";
import type { Bounty } from "../lib/types";
import { Identicon, Skeleton, StatusBadge } from "./ui";

export function BountyCard({ bounty }: { bounty: Bounty }) {
  const { t } = useTranslation();
  const { ecosystem, text } = splitIssueRef(bounty.issueRef);
  const open = bounty.status === "Open";
  return (
    <Link to={`/bounty/${bounty.id}`} className={`card bounty-card status-${bounty.status.toLowerCase()}`}>
      <div className="card-top">
        <span className="card-id mono">#{bounty.id}</span>
        {ecosystem && <span className="eco">{ecosystem}</span>}
        <StatusBadge status={bounty.status} />
      </div>

      <h3 className="card-title">{text}</h3>

      <div className="card-reward">
        <span className="reward-amount">{formatAmount(bounty.amount)}</span>
        <span className="reward-token">{tokenSymbol(bounty.token)}</span>
      </div>

      <div className="card-foot">
        <span className="creator">
          <Identicon address={bounty.creator} size={20} />
          <span className="muted">{t("bounty.claim", { count: bounty.claimCount })}</span>
        </span>
        <span className={open ? "deadline-live" : "muted"}>
          {open ? t("bounty.closesIn", { when: relativeTime(bounty.deadline) }) : t("bounty.closed")}
        </span>
      </div>
    </Link>
  );
}

export function BountyCardSkeleton() {
  return (
    <div className="card bounty-card" aria-hidden>
      <div className="card-top">
        <Skeleton className="sk-sm" />
        <Skeleton className="sk-pill" />
      </div>
      <Skeleton className="sk-title" />
      <Skeleton className="sk-reward" />
      <div className="card-foot">
        <Skeleton className="sk-sm" />
        <Skeleton className="sk-sm" />
      </div>
    </div>
  );
}
