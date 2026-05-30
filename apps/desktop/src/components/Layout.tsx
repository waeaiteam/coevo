import { Outlet, useLocation } from "react-router-dom";
import Sidebar from "./Sidebar";
import TopStatusBar from "./TopStatusBar";
import GovernancePanel from "./GovernancePanel";

export default function Layout() {
  const loc = useLocation();
  const isHome = loc.pathname === "/";

  return (
    <div className="flex h-screen overflow-hidden" style={{ background: "var(--bg-primary)" }}>
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <TopStatusBar />
        <div className="flex min-h-0 flex-1 overflow-hidden">
          <main className={`min-w-0 flex-1 overflow-y-auto ${isHome ? "p-0" : "p-5"}`}>
            <Outlet />
          </main>
          {isHome && <GovernancePanel />}
        </div>
      </div>
    </div>
  );
}
