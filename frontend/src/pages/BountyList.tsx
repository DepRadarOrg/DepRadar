import { useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { BountyCard, BountyCardSkeleton } from "../components/BountyCard";
import { Radar } from "../components/Radar";
import { Empty, ErrorState, Skeleton } from "../components/ui";
import { listBounties } from "../lib/contract";
import { formatAmount } from "../lib/format";
import { boardStats } from "../lib/insights";
import type { Bounty, BountyStatus } from "../lib/types";
import { useAsync } from "../lib/useAsync";

const FILTERS: { key: string; label: string; status?: BountyStatus }[] = [
  { key: "all", label: "filters.all" },
  { key: "open", label: "filters.open", status: "Open" },
  { key: "paid", label: "filters.paid", status: "Released" },
  { key: "refunded", label: "filters.refunded", status: "Cancelled" },
];

export function BountyList() {
  const { t } = useTranslation();
  const state = useAsync(listBounties, "bounties");
  const [params, setParams] = useSearchParams();
  const filter = FILTERS.find((f) => f.key === params.get("status")) ?? FILTERS[0];
  const bounties = state.status === "success" ? state.data : [];
  const shown = filter.status ? bounties.filter((b) => b.status === filter.status) : bounties;

  return (
    <>
      <section className="hero">
        <div className="hero-copy">
          <p className="eyebrow">
            <span className="live-dot" aria-hidden /> {t("hero.eyebrow")}
          </p>
          <h1>
            {t("hero.titlePrefix")} <span className="grad">{t("hero.titleAccent")}</span>
          </h1>
          <p className="lede">
            {t("hero.lede")}
          </p>
          <HeroStats bounties={state.status === "success" ? state.data : null} />
        </div>
        <Radar bounties={bounties} />
      </section>

      <section className="board">
        <div className="board-head">
          <h2>{t("board.title")}</h2>
          <div className="chips" role="tablist" aria-label={t("board.filterAria")}>
            {FILTERS.map((f) => {
              const count = f.status ? bounties.filter((b) => b.status === f.status).length : bounties.length;
              return (
                <button
                  key={f.key}
                  type="button"
                  role="tab"
                  aria-selected={f.key === filter.key}
                  className={`chip ${f.key === filter.key ? "chip-on" : ""}`}
                  onClick={() => setParams(f.key === "all" ? {} : { status: f.key })}
                >
                  {t(f.label)}
                  {state.status === "success" && <span className="chip-count">{count}</span>}
                </button>
              );
            })}
          </div>
        </div>

        {state.status === "loading" && (
          <div className="grid">
            {[0, 1, 2].map((i) => (
              <BountyCardSkeleton key={i} />
            ))}
          </div>
        )}
        {state.status === "error" && <ErrorState error={state.error} onRetry={state.reload} />}
        {state.status === "success" &&
          (shown.length === 0 ? (
            bounties.length === 0 ? (
              <Empty>
                <div className="empty-cta">
                  <p className="empty-cta-title">{t("empty.title")}</p>
                  <p>{t("empty.body")}</p>
                  <ul className="empty-cta-links">
                    <li>
                      <a href="https://github.com/DepRadarOrg/DepRadar/blob/main/docs/TUTORIAL.md">{t("empty.links.tutorial")}</a>
                    </li>
                    <li>
                      <a href="https://github.com/DepRadarOrg/DepRadar/blob/main/CONTRIBUTING.md">{t("empty.links.contributing")}</a>
                    </li>
                    <li>
                      <a href="https://github.com/DepRadarOrg/DepRadar/issues">{t("empty.links.issues")}</a>
                    </li>
                  </ul>
                </div>
              </Empty>
            ) : (
              <Empty>{t("empty.noneForFilter", { filter: t(filter.label).toLowerCase() })}</Empty>
            )
          ) : (
            <div className="grid">
              {shown.map((b) => (
                <BountyCard key={b.id} bounty={b} />
              ))}
            </div>
          ))}
      </section>
    </>
  );
}

function HeroStats({ bounties }: { bounties: Bounty[] | null }) {
  const stats = bounties && boardStats(bounties);
  const items = [
    { label: "XLM paid to researchers", value: stats && formatAmount(stats.paidXlm), accent: true },
    { label: "XLM locked in open bounties", value: stats && formatAmount(stats.lockedXlm) },
    { label: "Bounties funded", value: stats && String(stats.total) },
    { label: "Claims submitted", value: stats && String(stats.claims) },
  ];
  return (
    <dl className="hero-stats">
      {items.map((i) => (
        <div key={i.label} className={i.accent ? "stat stat-accent" : "stat"}>
          <dd>{i.value ?? <Skeleton className="sk-stat" />}</dd>
          <dt>{i.label}</dt>
        </div>
      ))}
    </dl>
  );
}
