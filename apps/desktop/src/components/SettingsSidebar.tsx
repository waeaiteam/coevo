import { NavLink } from "react-router-dom";

interface Section {
  key: string;
  label: string;
  icon: string;
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
  const filtered = search
    ? sections.filter((s) => s.label.toLowerCase().includes(search.toLowerCase()))
    : sections;

  return (
    <nav className="flex-1 overflow-y-auto py-1">
      {filtered.map((s) => (
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
      ))}
      {filtered.length === 0 && (
        <div className="px-3 py-4 text-xs text-center" style={{ color: "var(--text-muted)" }}>
          No results
        </div>
      )}
    </nav>
  );
}
