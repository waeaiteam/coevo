import { NavLink } from "react-router-dom";
import { t, useLanguage } from "../settings/i18n";

const links = [
  { to: "/", key: "nav.new_chat", icon: "+" },
  { to: "/dashboard", key: "nav.opc", icon: "O" },
  { to: "/work-orders", key: "nav.work_orders", icon: "W" },
  { to: "/audit", key: "nav.audit", icon: "A" },
  { to: "/settings/general", key: "nav.settings", icon: "S" },
];

export default function Sidebar() {
  useLanguage();
  return (
    <aside className="flex w-56 min-w-0 flex-col border-r" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
      <div className="border-b p-4" style={{ borderColor: "var(--border-subtle)" }}>
        <div className="mb-1 flex items-center gap-2">
          <span className="grid h-7 w-7 place-items-center rounded-md text-sm font-bold text-white" style={{ background: "var(--accent)" }}>c</span>
          <span className="truncate text-sm font-bold tracking-tight">{t("app.name")}</span>
        </div>
        <div className="truncate text-xs" style={{ color: "var(--text-muted)" }}>{t("app.tagline")}</div>
      </div>
      <nav aria-label={t("nav.primary")} className="flex-1 space-y-1 overflow-y-auto px-2 py-3">
        {links.map((link) => (
          <NavLink key={link.to} to={link.to} end={link.to === "/"} className={({ isActive }) => `flex min-w-0 items-center gap-3 rounded-md px-3 py-2 text-xs transition-colors ${isActive ? "font-semibold" : ""}`} style={({ isActive }) => ({ background: isActive ? "var(--accent-dim)" : "transparent", color: isActive ? "var(--accent)" : "var(--text-secondary)" })}>
            <span className="w-5 shrink-0 text-center text-sm" aria-hidden="true">{link.icon}</span>
            <span className="truncate">{t(link.key)}</span>
          </NavLink>
        ))}
      </nav>
      <div className="border-t p-3 text-xs" style={{ borderColor: "var(--border-subtle)", color: "var(--text-muted)" }}>
        {t("app.alpha_console")}
      </div>
    </aside>
  );
}
