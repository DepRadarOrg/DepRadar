import { Link, useParams } from "react-router-dom";
import { CopyButton, Empty, ErrorState, Identicon, Loading, StatusBadge } from "../components/ui";
import { explorer } from "../config";
import { listBounties, listClaims } from "../lib/contract";
import { formatAmount, formatDate, tokenSymbol } from "../lib/format";
import type { Bounty, Claim } from "../lib/types";
import { useAsync } from "../lib/useAsync";

interface AddressActivity {
  funded: Bounty[];
  claimed: { claim: Claim; bounty: Bounty }[];
}

/** Everything one account has funded and claimed. Scans all bounties, since there's no claimant index yet. */
async function loadActivity(address: string): Promise<AddressActivity> {
  const bounties = await listBounties();
  const funded = bounties.filter((b) => b.creator === address);

  const withClaims = bounties.filter((b) => b.claimCount > 0);
  const claimLists = await Promise.all(withClaims.map((b) => listClaims(b)));
  const byId = new Map(bounties.map((b) => [b.id, b]));
  const claimed = claimLists
    .flat()
    .filter((c) => c.claimant === address)
    .flatMap((claim) => {
      const bounty = byId.get(claim.bountyId);
      return bounty ? [{ claim, bounty }] : [];
    });

  return { funded, claimed };
}

export function Address() {
  const address = useParams().id ?? "";
  const isAccount = address.startsWith("G");

  const state = useAsync(async () => (isAccount ? loadActivity(address) : null), `address:${address}`);

  return (
    <div className="detail">
      <Link to="/" className="back">
        ← Back to the board
      </Link>

      {!isAccount && <Empty>Not a Stellar account address.</Empty>}
      {isAccount && state.status === "loading" && <Loading label="Loading address…" />}
      {isAccount && state.status === "error" && <ErrorState error={state.error} onRetry={state.reload} />}
      {isAccount && state.status === "success" && state.data && (
        <AddressDetail address={address} activity={state.data} />
      )}
    </div>
  );
}

function AddressDetail({ address, activity }: { address: string; activity: AddressActivity }) {
  return (
    <>
      <header className="detail-hero">
        <div className="detail-hero-main">
          <h1 className="addr-title">
            <Identicon address={address} size={32} /> <span className="mono">{address}</span>
            <a
              className="address-explorer"
              href={explorer.account(address)}
              target="_blank"
              rel="noreferrer"
              aria-label="View on Stellar Expert"
              title="View on Stellar Expert"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
                <path d="M14 3h7v7M21 3l-9 9M19 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h5" />
              </svg>
            </a>
            <CopyButton value={address} label="Copy address" />
          </h1>
          <p className="muted">
            {activity.funded.length} funded · {activity.claimed.length} claims
          </p>
        </div>
      </header>

      <div className="detail-grid">
        <section>
          <h2>Bounties funded</h2>
          {activity.funded.length === 0 ? (
            <Empty>No bounties funded by this address.</Empty>
          ) : (
            <ul className="addr-list">
              {activity.funded.map((b) => (
                <li key={b.id}>
                  <Link to={`/bounty/${b.id}`}>Bounty #{b.id}</Link> <StatusBadge status={b.status} />{" "}
                  <span className="mono">
                    {formatAmount(b.amount)} {tokenSymbol(b.token)}
                  </span>{" "}
                  <span className="muted">{formatDate(b.createdAt)}</span>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section>
          <h2>Claims made</h2>
          {activity.claimed.length === 0 ? (
            <Empty>No claims submitted by this address.</Empty>
          ) : (
            <ul className="addr-list">
              {activity.claimed.map(({ claim, bounty }) => (
                <li key={`${claim.bountyId}-${claim.id}`}>
                  <Link to={`/bounty/${bounty.id}`}>Bounty #{bounty.id}</Link> · claim #{claim.id}{" "}
                  <StatusBadge status={claim.status} />{" "}
                  <span className="muted">{formatDate(claim.submittedAt)}</span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </>
  );
}
