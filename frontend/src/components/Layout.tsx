import { useEffect } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import { CONTRACT_ID, NETWORK, explorer } from "../config";
import { shortAddress } from "../lib/format";

export function Layout() {
  const { pathname } = useLocation();
  useEffect(() => {
    window.scrollTo(0, 0);
  }, [pathname]);

  return (
    <div className="app">
      <div className="bg-grid" aria-hidden />
      <header className="topbar">
        <div className="topbar-inner">
          <Link to="/" className="brand">
            <span className="brand-mark" aria-hidden>
              <span className="brand-sweep" />
            </span>
            DepRadar
          </Link>
          <nav className="topbar-meta">
            <Link to="/about" className="btn btn-ghost">
              About
            </Link>
            <span className="net">
              <span className="live-dot" aria-hidden />
              {NETWORK}
            </span>
            <a href={explorer.contract(CONTRACT_ID)} target="_blank" rel="noreferrer" className="mono hide-sm">
              {shortAddress(CONTRACT_ID)}
            </a>
            <a className="btn btn-ghost" href="https://github.com/DepRadarOrg/DepRadar" target="_blank" rel="noreferrer">
              GitHub
            </a>
          </nav>
        </div>
      </header>
      <main className="content">
        <Outlet />
      </main>
      <footer className="footer">
        <span>
          Read-only view of the <code>deprader_router</code> contract — nothing here signs or submits transactions.
        </span>
        <span className="muted">MIT · Built on Stellar Soroban</span>
      </footer>
    </div>
  );
}
