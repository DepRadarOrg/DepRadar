import { HashRouter, Route, Routes } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Layout } from "./components/Layout";
import { Empty } from "./components/ui";
import { About } from "./pages/About";
import { BountyDetail } from "./pages/BountyDetail";
import { BountyList } from "./pages/BountyList";

/** Hash routing so the build works on any static host with no rewrite rules. */
export default function App() {
  const { t } = useTranslation();
  return (
    <ErrorBoundary>
      <HashRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={<BountyList />} />
            <Route path="bounty/:id" element={<BountyDetail />} />
            <Route path="about" element={<About />} />
            <Route path="*" element={<Empty>{t("app.notFound")}</Empty>} />
          </Route>
        </Routes>
      </HashRouter>
    </ErrorBoundary>
  );
}
