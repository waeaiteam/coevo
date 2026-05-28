import { NavLink } from "react-router-dom";

const links = [
  { to: "/", label: "Dashboard", icon: "◈" },
  { to: "/contracts", label: "Contracts", icon: "⊡" },
  { to: "/plans", label: "Plans", icon: "↗" },
  { to: "/customs", label: "Customs", icon: "◎" },
  { to: "/risk", label: "Risk Gate", icon: "⚠" },
  { to: "/resolution", label: "Resolution", icon: "⚖" },
  { to: "/demos", label: "Demos", icon: "▶" },
];

export default function Sidebar() {
  return (
    <aside className="w-52 flex flex-col border-r" style={{ background: "#0d0d16", borderColor: "var(--border-subtle)" }}>
      <div className="p-4 border-b" style={{ borderColor: "var(--border-subtle)" }}>
        <div className="flex items-center gap-2 mb-1">
          <span className="text-lg font-bold tracking-tight" style={{ color: "var(--accent)" }}>◈</span>
          <span className="text-sm font-bold tracking-wide" style={{ color: "var(--text-primary)" }}>coevo</span>
        </div>
        <div className="text-xs tracking-widest uppercase" style={{ color: "var(--text-muted)" }}>
          Control Plane
        </div>
      </div>

      <nav className="flex-1 py-3 space-y-0.5 px-2">
        {links.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            end={link.to === "/"}
            className={({ isActive }) =>
              `flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-all duration-150 ${
                isActive
                  ? "text-white"
                  : "hover:text-white"
              }`
            }
            style={({ isActive }) => ({
              background: isActive ? "var(--accent-dim)" : "transparent",
              color: isActive ? "#fff" : "var(--text-secondary)",
            })}
          >
            <span className="text-base w-5 text-center">{link.icon}</span>
            <span className="text-xs tracking-wide">{link.label}</span>
          </NavLink>
        ))}
      </nav>

      <div className="p-3 border-t text-xs tracking-wider" style={{ borderColor: "var(--border-subtle)", color: "var(--text-muted)" }}>
        v1.0.0
      </div>
    </aside>
  );
}
