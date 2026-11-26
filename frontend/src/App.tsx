import { lazy, Suspense } from "react";
import { HashRouter, Route, Routes } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Layout } from "./components/Layout";
import { Empty, Loading } from "./components/ui";
import { WalletProvider } from "./wallet/WalletContext";

// Pages are lazy so the shell renders immediately and the Stellar SDK (pulled
// in by the contract layer) loads only with the routes that need it.
const Address = lazy(() => import("./pages/Address").then((m) => ({ default: m.Address })));
const About = lazy(() => import("./pages/About").then((m) => ({ default: m.About })));
const BountyDetail = lazy(() => import("./pages/BountyDetail").then((m) => ({ default: m.BountyDetail })));
const BountyList = lazy(() => import("./pages/BountyList").then((m) => ({ default: m.BountyList })));
const Leaderboard = lazy(() => import("./pages/Leaderboard").then((m) => ({ default: m.Leaderboard })));
const NewBounty = lazy(() => import("./pages/NewBounty").then((m) => ({ default: m.NewBounty })));
const Report = lazy(() => import("./pages/Report"));
const RepoScanReport = lazy(() => import("./pages/Report"));

/** Hash routing so the build works on any static host with no rewrite rules. */
export default function App() {
  const { t } = useTranslation();
  return (
    <ErrorBoundary>
      <WalletProvider>
        <HashRouter>
          <Suspense fallback={<Loading />}>
            <Routes>
              <Route element={<Layout />}>
                <Route index element={<BountyList />} />
                <Route path="new" element={<NewBounty />} />
                <Route path="bounty/:id" element={<BountyDetail />} />
                <Route path="leaderboard" element={<Leaderboard />} />
                <Route path="report" element={<Report />} />
                <Route path="repo-scan" element={<RepoScanReport />} />
                <Route path="address/:id" element={<Address />} />
                <Route path="about" element={<About />} />
                <Route path="*" element={<Empty>{t("app.notFound")}</Empty>} />
              </Route>
            </Routes>
          </Suspense>
        </HashRouter>
      </WalletProvider>
    </ErrorBoundary>
  );
}
