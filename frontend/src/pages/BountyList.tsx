import { useSearchParams } from "react-router-dom";
import { BountyCard, BountyCardSkeleton } from "../components/BountyCard";
import { Radar } from "../components/Radar";
import { EmptyCTA, ErrorState, Skeleton } from "../components/ui";
import { listBounties } from "../lib/contract";
import { formatAmount } from "../lib/format";
import { boardStats } from "../lib/insights";
import type { Bounty, BountyStatus } from "../lib/types";
import { useAsync } from "../lib/useAsync";

const FILTERS: { key: string; label: string; status?: BountyStatus }[] = [
  { key: "all", label: "All" },
  { key: "open", label: "Open", status: "Open" },
  { key: "paid", label: "Paid", status: "Released" },
  { key: "refunded", label: "Refunded", status: "Cancelled" },
];

export function BountyList() {
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
            <span className="live-dot" aria-hidden /> Live on Stellar testnet
          </p>
          <h1>
            Funded bounties, <span className="grad">not empty advisories.</span>
          </h1>
          <p className="lede">
            DepRadar watches open-source registries for typosquats, malicious packages and known CVEs — and pays whoever
            catches them first, on-chain, with every step verifiable.
          </p>
          <HeroStats bounties={state.status === "success" ? state.data : null} />
        </div>
        <Radar bounties={bounties} />
      </section>

      <section className="board">
        <div className="board-head">
          <h2>Bounty board</h2>
          <div className="chips" role="tablist" aria-label="Filter by status">
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
                  {f.label}
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
            <EmptyCTA
              title={
                bounties.length === 0
                  ? "No bounties yet — the radar is quiet."
                  : `No ${filter.label.toLowerCase()} bounties right now.`
              }
              description="DepRadar coordinates security bounties for dependency typosquats and supply-chain vulnerabilities on Stellar. Creators lock XLM rewards against security findings, and researchers earn bounties by discovering and submitting verifiable proof."
            />
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
