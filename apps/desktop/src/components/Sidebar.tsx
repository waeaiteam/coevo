import { NavLink } from "react-router-dom";

const links = [
  { to: "/", label: "Mission Chat", icon: "◈" },
  { to: "/dashboard", label: "Dashboard", icon: "⊞" },
  { to: "/contracts", label: "Contracts", icon: "⊡" },
  { to: "/plans", label: "Plans", icon: "↗" },
  { to: "/customs", label: "Cognitive Customs", icon: "◎" },
  { to: "/risk", label: "Risk Gate", icon: "⚠" },
  { to: "/resolution", label: "Resolution", icon: "⚖" },
  { to: "/audit", label: "Audit", icon: "☰" },
  { to: "/demos", label: "Demos", icon: "▶" },
  { to: "/settings", label: "Settings", icon: "⚙" },
];

export default function Sidebar() {
  return (
    <aside className="w-52 flex flex-col border-r" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
      <div className="p-4 border-b" style={{ borderColor: "var(--border-subtle)" }}>
        <div className="flex items-center gap-2 mb-0.5">
          <span className="text-lg font-bold" style={{ color: "var(--accent)" }}>◈</span>
          <span className="text-sm font-bold tracking-tight">coevo</span>
        </div>
        <div className="text-xs tracking-wide" style={{ color: "var(--text-muted)" }}>Agent Governance Mesh</div>
      </div>
      <nav className="flex-1 py-2 space-y-0.5 px-2 overflow-y-auto">
        {links.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            end={link.to === "/"}
            className={({ isActive }) =>
              `flex items-center gap-3 px-3 py-2 rounded-md text-xs tracking-wide transition-colors ${
                isActive ? "font-semibold" : ""
              }`
            }
            style={({ isActive }) => ({
              background: isActive ? "var(--accent-dim)" : "transparent",
              color: isActive ? "var(--accent)" : "var(--text-secondary)",
            })}
          >
            <span className="text-sm w-5 text-center">{link.icon}</span>
            {link.label}
          </NavLink>
        ))}
      </nav>
      <div className="p-3 border-t text-xs" style={{ borderColor: "var(--border-subtle)", color: "var(--text-muted)" }}>v1.0.0</div>
    </aside>
  );
}
