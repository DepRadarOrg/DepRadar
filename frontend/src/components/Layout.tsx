import { useEffect, useState } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import { CONTRACT_ID, NETWORK, explorer } from "../config";
import { shortAddress } from "../lib/format";

/** The dashboard's stored theme, `null` until the user picks one manually. */
export type ThemeChoice = "light" | "dark" | null;

const THEME_KEY = "depradar-theme";

/** Manual theme toggle (#135): persisted to localStorage, dark by default. */
// eslint-disable-next-line react-refresh/only-export-components -- oxlint fast-refresh warning; hook shares the theme key with Layout
export function useTheme() {
  const [theme, setTheme] = useState<ThemeChoice>(() => {
    try {
      return localStorage.getItem(THEME_KEY) as ThemeChoice;
    } catch {
      return null;
    }
  });
  useEffect(() => {
    if (theme === "light") document.documentElement.setAttribute("data-theme", "light");
    else document.documentElement.removeAttribute("data-theme");
    try {
      if (theme) localStorage.setItem(THEME_KEY, theme);
      else localStorage.removeItem(THEME_KEY);
    } catch {
      /* private mode: keep the choice for the session only */
    }
  }, [theme]);
  return [theme, setTheme] as const;
}

export function Layout() {
  const { pathname } = useLocation();
  const [theme, setTheme] = useTheme();
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
            <button
              type="button"
              className="btn btn-ghost theme-toggle"
              aria-label={theme === "light" ? "Switch to dark theme" : "Switch to light theme"}
              onClick={() => setTheme(theme === "light" ? null : "light")}
            >
              {theme === "light" ? "🌙 Dark" : "☀️ Light"}
            </button>
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
