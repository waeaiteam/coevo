import { Outlet } from "react-router-dom";
import Sidebar from "./Sidebar";
import TopStatusBar from "./TopStatusBar";

export default function Layout() {
  return (
    <div className="flex h-screen overflow-hidden" style={{ background: "var(--bg-primary)" }}>
      <Sidebar />
      <div className="flex-1 flex flex-col overflow-hidden">
        <TopStatusBar />
        <main className="flex-1 overflow-y-auto p-5">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
