import { NavLink } from "react-router-dom";
import { t, useLanguage } from "../settings/i18n";

interface Section {
  key: string;
  label: string;
  icon: string;
  group?: "common" | "advanced";
}

export default function SettingsSidebar({
  sections,
  active,
  search,
}: {
  sections: Section[];
  active: string;
  search: string;
}) {
  useLanguage();
  const filtered = search
    ? sections.filter((s) => s.label.toLowerCase().includes(search.toLowerCase()))
    : sections;
  const common = filtered.filter((s) => s.group !== "advanced");
  const advanced = filtered.filter((s) => s.group === "advanced");

  return (
    <nav className="flex-1 overflow-y-auto py-1">
      <div role="group" aria-label={t("settings.common")}>
        {common.map((s) => (
          <NavItem key={s.key} s={s} />
        ))}
      </div>
      <div role="group" aria-label={t("settings.advanced")} className="mt-3 pt-2 border-t" style={{ borderColor: "var(--border-subtle)" }}>
        {advanced.map((s) => (
          <NavItem key={s.key} s={s} />
        ))}
      </div>
      {filtered.length === 0 && (
        <div className="px-3 py-4 text-xs text-center" style={{ color: "var(--text-muted)" }}>
          {t("settings.no_results")}
        </div>
      )}
    </nav>
  );
}

function NavItem({ s }: { s: Section }) {
  return (
    <NavLink
      key={s.key}
      to={`/settings/${s.key}`}
      className="flex items-center gap-2.5 px-3 py-2 text-xs transition-colors"
      style={({ isActive }) => ({
        background: isActive ? "var(--accent-dim)" : "transparent",
        color: isActive ? "var(--accent)" : "var(--text-secondary)",
        fontWeight: isActive ? 600 : 400,
      })}
    >
      <span className="text-sm w-4 text-center">{s.icon}</span>
      {s.label}
    </NavLink>
  );
}
