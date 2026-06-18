import { useEffect, useState } from "react";
import { Link, NavLink } from "react-router-dom";
import { getActiveOpcId } from "../api/companies";
import { listCompanyConversations } from "../api/org";
import { getLocalIdentity } from "../settings/identity";
import { useAdvancedMode } from "../hooks/useAdvancedMode";
import { t, useLanguage } from "../settings/i18n";
import { clearMissionSession, writeActiveConversationId } from "../utils/missionSession";
import { formatRelativeTime, shortText, stringField, type ProductRow } from "../utils/productSurface";

type IconName = "new-chat" | "company" | "market" | "projects" | "tasks" | "timeline" | "settings" | "advanced" | "workflows" | "team" | "audit" | "executors" | "memory";

// Default founder surface: the calm, jargon-free core workflow.
const primaryLinks: Array<{ to: string; key: string; icon: IconName; end?: boolean }> = [
  { to: "/team", key: "nav.my_team", icon: "team" },
  { to: "/tasks", key: "nav.today", icon: "tasks" },
  { to: "/projects", key: "nav.projects", icon: "projects" },
  { to: "/settings/general", key: "nav.settings", icon: "settings" },
];

// Advanced mode adds the full operator console on top of the core workflow.
const advancedLinks: Array<{ to: string; key: string; icon: IconName; end?: boolean }> = [
  { to: "/company", key: "nav.my_company", icon: "company" },
  { to: "/market", key: "nav.talent_market", icon: "market" },
  { to: "/workflows", key: "workflows.title", icon: "workflows" },
  { to: "/timeline", key: "nav.timeline", icon: "timeline" },
  { to: "/audit", key: "nav.audit", icon: "audit" },
  { to: "/executors", key: "nav.executors", icon: "executors" },
  { to: "/memory", key: "nav.memory", icon: "memory" },
];

function NavIcon({ name }: { name: IconName }) {
  const paths: Record<IconName, JSX.Element> = {
    "new-chat": (
      <>
        <path d="M12 5v14" />
        <path d="M5 12h14" />
      </>
    ),
    company: (
      <>
        <path d="M4 20V9l8-5 8 5v11" />
        <path d="M9 20v-6h6v6" />
      </>
    ),
    market: (
      <>
        <path d="M4 9h16l-1 3a2.5 2.5 0 0 1-5 0 2.5 2.5 0 0 1-5 0 2.5 2.5 0 0 1-5 0L4 9z" />
        <path d="M4 9l1.5-4h13L20 9" />
        <path d="M5 12v8h14v-8" />
      </>
    ),
    projects: (
      <>
        <path d="M4 6h6l2 2h8v10a2 2 0 0 1-2 2H4z" />
        <path d="M4 6v12" />
      </>
    ),
    tasks: (
      <>
        <path d="M9 11l2 2 4-5" />
        <path d="M5 6h2" />
        <path d="M5 12h2" />
        <path d="M5 18h2" />
        <path d="M11 18h8" />
      </>
    ),
    timeline: (
      <>
        <path d="M12 8v5l3 2" />
        <path d="M21 12a9 9 0 1 1-3-6.7" />
        <path d="M21 4v5h-5" />
      </>
    ),
    settings: (
      <>
        <path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z" />
        <path d="M19.4 15a1.8 1.8 0 0 0 .36 2l.05.05a2 2 0 1 1-2.83 2.83l-.05-.05a1.8 1.8 0 0 0-2-.36 1.8 1.8 0 0 0-1 1.63V21a2 2 0 1 1-4 0v-.1a1.8 1.8 0 0 0-1-1.63 1.8 1.8 0 0 0-2 .36l-.05.05a2 2 0 1 1-2.83-2.83l.05-.05a1.8 1.8 0 0 0 .36-2 1.8 1.8 0 0 0-1.63-1H3a2 2 0 1 1 0-4h.1a1.8 1.8 0 0 0 1.63-1 1.8 1.8 0 0 0-.36-2l-.05-.05a2 2 0 1 1 2.83-2.83l.05.05a1.8 1.8 0 0 0 2 .36 1.8 1.8 0 0 0 1-1.63V3a2 2 0 1 1 4 0v.1a1.8 1.8 0 0 0 1 1.63 1.8 1.8 0 0 0 2-.36l.05-.05a2 2 0 1 1 2.83 2.83l-.05.05a1.8 1.8 0 0 0-.36 2 1.8 1.8 0 0 0 1.63 1H21a2 2 0 1 1 0 4h-.1a1.8 1.8 0 0 0-1.5 1Z" />
      </>
    ),
    advanced: (
      <>
        <path d="M4 7h16" />
        <path d="M4 12h10" />
        <path d="M4 17h16" />
      </>
    ),
    workflows: (
      <>
        <path d="M6 3v12" />
        <circle cx="6" cy="18" r="3" />
        <circle cx="6" cy="6" r="3" />
        <circle cx="18" cy="6" r="3" />
        <path d="M18 9a9 9 0 0 1-9 9" />
      </>
    ),
    team: (
      <>
        <circle cx="9" cy="7" r="3" />
        <path d="M3 20v-1a6 6 0 0 1 12 0v1" />
        <path d="M16 3.5a3 3 0 0 1 0 6" />
        <path d="M19 20v-1a5 5 0 0 0-3-4.6" />
      </>
    ),
    audit: (
      <>
        <path d="M9 11l2 2 4-5" />
        <path d="M5 4h14v16l-7-3-7 3z" />
      </>
    ),
    executors: (
      <>
        <rect x="4" y="4" width="16" height="16" rx="2" />
        <path d="M9 9h6v6H9z" />
      </>
    ),
    memory: (
      <>
        <path d="M12 3a4 4 0 0 0-4 4v1a3 3 0 0 0 0 6 3 3 0 0 0 4 3 3 3 0 0 0 4-3 3 3 0 0 0 0-6V7a4 4 0 0 0-4-4z" />
      </>
    ),
  };

  return (
    <svg className="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {paths[name]}
    </svg>
  );
}

export default function Sidebar() {
  const language = useLanguage();
  const advancedMode = useAdvancedMode();
  const [conversations, setConversations] = useState<ProductRow[]>([]);
  const identity = getLocalIdentity();
  const activeOpcId = getActiveOpcId();

  useEffect(() => {
    let alive = true;
    listCompanyConversations(activeOpcId)
      .then((rows) => {
        if (alive) setConversations(Array.isArray(rows) ? rows as ProductRow[] : []);
      })
      .catch(() => {
        if (alive) setConversations([]);
      });
    return () => {
      alive = false;
    };
  }, [activeOpcId, language]);

  function rememberConversation(id: string) {
    writeActiveConversationId(activeOpcId, identity.userId, id);
  }

  function startNewChat() {
    clearMissionSession(activeOpcId, identity.userId);
  }

  return (
    <aside className="sidebar-shell product-sidebar flex min-w-0 flex-col">
      <div className="sidebar-brand">
        <Link to="/" className="flex items-center gap-2">
          <span className="sidebar-logo">c</span>
        </Link>
      </div>

      <div className="px-2 py-2">
        <NavLink to="/" end onClick={startNewChat} className={({ isActive }) => `nav-item product-new-chat ${isActive ? "active" : ""}`}>
          <span className="nav-icon" aria-hidden="true"><NavIcon name="new-chat" /></span>
          <span className="sidebar-text truncate">{t("nav.new_chat")}</span>
        </NavLink>
      </div>

      <div className="sidebar-section">
        <div className="sidebar-section-title sidebar-text">{t("nav.recent_chats")}</div>
        <div className="sidebar-conversation-list">
          {conversations.slice(0, 7).map((conversation, index) => {
            const id = stringField(conversation, "conversation_id") || `conversation-${index}`;
            return (
              <Link
                key={id}
                to={`/conversations/${encodeURIComponent(id)}`}
                className="sidebar-conversation"
                onClick={() => rememberConversation(id)}
              >
                <span className="sidebar-text truncate">{shortText(stringField(conversation, "title") || t("chat.untitled"), 34)}</span>
                <span className="sidebar-text sidebar-conversation-time">{formatRelativeTime(Number(conversation.updated_at_ms || 0))}</span>
              </Link>
            );
          })}
          {conversations.length === 0 && <div className="sidebar-empty sidebar-text">{t("chat.no_recent")}</div>}
        </div>
      </div>

      <nav aria-label={t("nav.primary")} className="flex-1 space-y-1 overflow-y-auto px-2 py-3">
        {primaryLinks.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            end={link.end}
            className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}
          >
            <span className="nav-icon" aria-hidden="true"><NavIcon name={link.icon} /></span>
            <span className="sidebar-text truncate">{t(link.key)}</span>
          </NavLink>
        ))}
        {advancedMode && (
          <>
            <div className="sidebar-section-title sidebar-text" style={{ marginTop: 12 }}>{t("nav.advanced")}</div>
            {advancedLinks.map((link) => (
              <NavLink
                key={link.to}
                to={link.to}
                end={link.end}
                className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}
              >
                <span className="nav-icon" aria-hidden="true"><NavIcon name={link.icon} /></span>
                <span className="sidebar-text truncate">{t(link.key)}</span>
              </NavLink>
            ))}
          </>
        )}
      </nav>

      <div className="sidebar-footer px-2 py-2 text-[11px] leading-5" style={{ color: "var(--text-muted)" }}>
        {advancedMode ? (
          <NavLink to="/dashboard" className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}>
            <span className="nav-icon" aria-hidden="true"><NavIcon name="advanced" /></span>
            <span className="sidebar-text truncate">{t("nav.dashboard")}</span>
          </NavLink>
        ) : (
          <NavLink to="/settings/appearance" className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}>
            <span className="nav-icon" aria-hidden="true"><NavIcon name="advanced" /></span>
            <span className="sidebar-text truncate">{t("nav.enable_advanced")}</span>
          </NavLink>
        )}
      </div>
    </aside>
  );
}
