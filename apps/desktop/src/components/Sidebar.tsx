import { NavLink } from "react-router-dom";

const links = [
  { to: "/", label: "Dashboard", icon: "◈" },
  { to: "/contracts", label: "Contracts", icon: "⚙" },
  { to: "/plans", label: "Plans", icon: "↗" },
  { to: "/customs", label: "Customs", icon: "⊡" },
  { to: "/risk", label: "Risk Gate", icon: "⚠" },
  { to: "/resolution", label: "Resolution", icon: "⚖" },
  { to: "/demos", label: "Demos", icon: "▶" },
];

export default function Sidebar() {
  return (
    <aside className="w-56 bg-gray-900 text-gray-200 flex flex-col">
      <div className="p-4 border-b border-gray-700">
        <h1 className="text-lg font-bold text-white">coevo</h1>
        <p className="text-xs text-gray-400">Agent Governance Mesh</p>
      </div>
      <nav className="flex-1 py-2">
        {links.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            end={link.to === "/"}
            className={({ isActive }) =>
              `flex items-center gap-3 px-4 py-2.5 text-sm transition-colors ${
                isActive
                  ? "bg-gray-700 text-white border-l-2 border-green-500"
                  : "hover:bg-gray-800 text-gray-300"
              }`
            }
          >
            <span>{link.icon}</span>
            {link.label}
          </NavLink>
        ))}
      </nav>
      <div className="p-3 border-t border-gray-700 text-xs text-gray-500">
        v1.0.0
      </div>
    </aside>
  );
}
