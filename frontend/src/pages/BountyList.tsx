import { useSearchParams } from "react-router-dom";
import { useEffect, useState } from "react";
import { BountyCard, BountyCardSkeleton } from "../components/BountyCard";
import { bountyPackage } from "../lib/typosquat";
import { Radar } from "../components/Radar";
import { Empty, ErrorState, Skeleton } from "../components/ui";
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

  // Search by finding text (#122): case-insensitive substring over the package
  // name and issue reference. The input is debounced (150ms) into the `q`
  // URL param so the view stays shareable without re-rendering every keystroke.
  const urlQuery = params.get("q") ?? "";
  const [input, setInput] = useState(urlQuery);
  useEffect(() => {
    const t = setTimeout(() => {
      const next = new URLSearchParams(params);
      if (input) next.set("q", input);
      else next.delete("q");
      if (next.toString() !== params.toString()) setParams(next, { replace: true });
    }, 150);
    return () => clearTimeout(t);
  }, [input, params, setParams]);
  useEffect(() => {
    // Keep the input in sync when the URL changes externally (back/forward
    // navigation, shared links with ?q=). Deliberate sync-in-effect.
    setInput(urlQuery);
  }, [urlQuery]);
  const query = urlQuery.trim().toLowerCase();
  const searched = query
    ? shown.filter(
        (b) =>
          b.issueRef.toLowerCase().includes(query) ||
          (bountyPackage(b) ?? "").toLowerCase().includes(query),
      )
    : shown;

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
          <form className="board-search" role="search" onSubmit={(e) => e.preventDefault()}>
            <input
              type="search"
              aria-label="Search bounties by finding text"
              placeholder="Search by package or finding…"
              value={input}
              onChange={(e) => setInput(e.target.value)}
            />
            {input && (
              <button
                type="button"
                className="chip"
                aria-label="Clear search"
                onClick={() => setInput("")}
              >
                Clear
              </button>
            )}
          </form>
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
          (searched.length === 0 ? (
            bounties.length === 0 ? (
              <Empty>
                <div className="empty-cta">
                  <p className="empty-cta-title">No bounties yet — the radar is quiet.</p>
                  <p>
                    Bounties are funded on-chain by maintainers. The first valid claim wins: submit your finding, an
                    admin reviews it, and the reward is released to your wallet once accepted.
                  </p>
                  <ul className="empty-cta-links">
                    <li>
                      <a href="https://github.com/DepRadarOrg/DepRadar/blob/main/docs/TUTORIAL.md">Read the tutorial</a>
                    </li>
                    <li>
                      <a href="https://github.com/DepRadarOrg/DepRadar/blob/main/CONTRIBUTING.md">Contributing guide</a>
                    </li>
                    <li>
                      <a href="https://github.com/DepRadarOrg/DepRadar/issues">Browse GitHub issues</a>
                    </li>
                  </ul>
                </div>
              </Empty>
            ) : query ? (
              <Empty>{`No bounties match “${urlQuery.trim()}”${filter.status ? ` in ${filter.label.toLowerCase()}` : ""}.`}</Empty>
            ) : (
              <Empty>{`No ${filter.label.toLowerCase()} bounties right now.`}</Empty>
            )
          ) : (
            <div className="grid">
              {searched.map((b) => (
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
