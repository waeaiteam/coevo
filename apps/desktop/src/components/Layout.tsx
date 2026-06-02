import { Outlet, useLocation } from "react-router-dom";
import Sidebar from "./Sidebar";
import TopStatusBar from "./TopStatusBar";
import CommandPalette from "./CommandPalette";
import { ThemeProvider } from "../hooks/useTheme";

export default function Layout() {
  const loc = useLocation();
  const isHome = loc.pathname === "/" || loc.pathname === "/mission" || loc.pathname.startsWith("/conversations/");

  return (
    <ThemeProvider>
      <div className="app-shell flex h-screen overflow-hidden">
        <Sidebar />
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <TopStatusBar />
          <div className="flex min-h-0 flex-1 overflow-hidden">
            <main className={`min-w-0 flex-1 overflow-y-auto ${isHome ? "p-0" : "p-5"}`}>
              <Outlet />
            </main>
          </div>
        </div>
        <CommandPalette />
      </div>
    </ThemeProvider>
  );
}
