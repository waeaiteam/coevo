import { NavLink } from "react-router-dom";
import { t, useLanguage } from "../settings/i18n";

const primaryLinks = [
  { to: "/", key: "nav.workbench", icon: "⌂" },
  { to: "/employees", key: "nav.ai_staff", icon: "人" },
  { to: "/work-orders", key: "nav.tasks", icon: "✓" },
  { to: "/memory", key: "nav.clients", icon: "名" },
  { to: "/contracts", key: "nav.files", icon: "文" },
  { to: "/plans", key: "nav.outcomes", icon: "果" },
];

export default function Sidebar() {
  useLanguage();
  return (
    <aside className="sidebar-shell flex min-w-0 flex-col">
      <div className="sidebar-brand">
        <div className="mb-1 flex items-center gap-2">
          <span className="sidebar-logo">c</span>
          <span className="sidebar-text truncate text-sm font-bold tracking-tight">{t("app.name")}</span>
        </div>
        <div className="sidebar-tagline truncate text-xs muted">{t("app.tagline")}</div>
      </div>
      <nav aria-label={t("nav.primary")} className="flex-1 space-y-1 overflow-y-auto px-2 py-3">
        {primaryLinks.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            end={link.to === "/"}
            className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}
          >
            <span className="nav-icon" aria-hidden="true">{link.icon}</span>
            <span className="sidebar-text truncate">{t(link.key)}</span>
          </NavLink>
        ))}
      </nav>
      <div className="sidebar-footer">
        <NavLink to="/settings/general" className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}>
          <span className="nav-icon" aria-hidden="true">⌘</span>
          <span className="sidebar-text truncate">{t("nav.advanced_settings")}</span>
        </NavLink>
      </div>
    </aside>
  );
}
