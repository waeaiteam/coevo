import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTheme, type ThemeMode } from "../hooks/useTheme";
import { useAdvancedMode } from "../hooks/useAdvancedMode";
import { getActiveOpcId } from "../api/companies";
import { t, useLanguage } from "../settings/i18n";
import Icon, { type IconName } from "./Icon";

type Command = {
  id: string;
  label: string;
  hint: string;
  initials: string;
  icon: IconName;
  run: () => void;
};

// Core commands shown to every founder. Calm, jargon-free navigation.
const corePages = [
  { id: "new-chat", labelKey: "nav.new_chat", hintKey: "cmd.new_task_hint", path: "/", initials: "new chat new task xt xr xrw", icon: "plus" as IconName },
  { id: "team", labelKey: "nav.my_team", hintKey: "cmd.my_company_hint", path: "/team", initials: "team tuandui company opc employees", icon: "users" as IconName },
  { id: "projects", labelKey: "nav.projects", hintKey: "cmd.projects_hint", path: "/projects", initials: "projects xm", icon: "folder-tree" as IconName },
  { id: "tasks", labelKey: "nav.today", hintKey: "cmd.tasks_hint", path: "/tasks", initials: "tasks rw gd workorders today", icon: "list-checks" as IconName },
  { id: "market", labelKey: "nav.talent_market", hintKey: "cmd.market_hint", path: "/market", initials: "market talent hire zhaopin scs ai yuangong shichang", icon: "users" as IconName },
  { id: "settings", labelKey: "nav.settings", hintKey: "cmd.settings_hint", path: "/settings/general", initials: "settings sz", icon: "settings" as IconName },
  { id: "model", labelKey: "settings.model_provider", hintKey: "cmd.model_hint", path: "/settings/model_provider", initials: "model mx llm", icon: "brain" as IconName },
];

// Advanced commands surface the full operator console; only shown in Advanced mode.
const advancedPages = [
  { id: "company", labelKey: "nav.my_company", hintKey: "cmd.my_company_hint", path: "/company", initials: "company opc wdgs wdopc", icon: "building" as IconName },
  { id: "timeline", labelKey: "nav.timeline", hintKey: "cmd.timeline_hint", path: "/timeline", initials: "timeline sjx audit sj", icon: "history" as IconName },
  { id: "dashboard", labelKey: "nav.dashboard", hintKey: "cmd.dashboard_hint", path: "/dashboard", initials: "dashboard opc gzt advanced", icon: "gauge" as IconName },
  { id: "founder", labelKey: "adv.founder_profile", hintKey: "adv.founder_profile_desc", path: "/founder", initials: "founder cshr", icon: "user" as IconName },
  { id: "memory", labelKey: "adv.company_memory", hintKey: "adv.company_memory_desc", path: "/memory", initials: "company memory gsjy", icon: "database" as IconName },
  { id: "skills", labelKey: "adv.skills", hintKey: "adv.skills_desc", path: "/skills", initials: "skills jn", icon: "puzzle" as IconName },
  { id: "executors", labelKey: "adv.external_executors", hintKey: "adv.external_executors_desc", path: "/executors", initials: "executors zxq", icon: "external" as IconName },
  { id: "contracts", labelKey: "adv.contracts", hintKey: "adv.contracts_desc", path: "/contracts", initials: "contracts hy", icon: "file-text" as IconName },
  { id: "customs", labelKey: "adv.cognitive_customs", hintKey: "adv.cognitive_customs_desc", path: "/customs", initials: "customs rg", icon: "badge-check" as IconName },
  { id: "resolution", labelKey: "adv.resolution", hintKey: "adv.resolution_desc", path: "/resolution", initials: "resolution jj", icon: "git-branch" as IconName },
  { id: "audit", labelKey: "nav.audit", hintKey: "cmd.audit_hint", path: "/audit", initials: "audit sj hd", icon: "clipboard" as IconName },
  { id: "evaluations", labelKey: "eval.title", hintKey: "eval.desc", path: "/evaluations", initials: "evaluations eval pg pinggu", icon: "badge-check" as IconName },
  { id: "traces", labelKey: "traces.title", hintKey: "traces.desc", path: "/traces", initials: "traces trace span lianlu zhuizong", icon: "history" as IconName },
  { id: "workflows", labelKey: "workflows.title", hintKey: "workflows.desc", path: "/workflows", initials: "workflows dag gongzuoliu bianpai", icon: "git-branch" as IconName },
  { id: "performance", labelKey: "perf.title", hintKey: "perf.desc", path: "/performance", initials: "performance perf xingneng sandbox shapan", icon: "gauge" as IconName },
  { id: "mcp", labelKey: "settings.mcp_servers", hintKey: "settings.mcp_servers_desc", path: "/settings/mcp_servers", initials: "mcp servers tools jsonrpc stdio http", icon: "puzzle" as IconName },
  { id: "data", labelKey: "settings.data_management", hintKey: "adv.data_management_desc", path: "/settings/data_management", initials: "data sj", icon: "database" as IconName },
];

function matches(command: Command, query: string) {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return `${command.label} ${command.hint} ${command.initials}`.toLowerCase().includes(q);
}

export default function CommandPalette() {
  const language = useLanguage();
  const advancedMode = useAdvancedMode();
  const navigate = useNavigate();
  const { setMode } = useTheme();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const commands = useMemo<Command[]>(() => {
    const pages = advancedMode ? [...corePages, ...advancedPages] : corePages;
    const navCommands = pages.map((page) => ({
      id: page.id,
      label: t(page.labelKey),
      hint: t(page.hintKey),
      initials: page.initials,
      icon: page.icon,
      run: () => navigate(page.path),
    }));
    const opc = getActiveOpcId();
    // Org-ecosystem destinations are advanced surfaces; only offer them in Advanced mode.
    const orgCommands = (advancedMode ? ([
      ["org-meetings", "org.meetings", "meet.subtitle", "meetings huiyi hys debate", "/meetings", "users"],
      ["org-performance", "org.performance", "kpi.subtitle", "performance kpi jixiao", "/performance", "gauge"],
      ["org-reports", "org.reports", "report.subtitle", "reports briefings jianbao ribao yuebao", "/reports", "file-text"],
      ["org-cost", "org.cost", "cost.subtitle", "cost budget chengben token", "/cost", "database"],
    ] as const) : []).map(([id, labelKey, hintKey, initials, suffix, icon]) => ({
      id,
      label: t(labelKey),
      hint: t(hintKey),
      initials,
      icon: icon as IconName,
      run: () => navigate(`/companies/${encodeURIComponent(opc)}${suffix}`),
    }));
    const themeCommands = ([
      ["theme-system", "cmd.theme_system", "cmd.theme_system_hint", "system xt", "system", "monitor"],
      ["theme-light", "cmd.theme_light", "cmd.theme_light_hint", "light qs", "light", "sun"],
      ["theme-dark", "cmd.theme_dark", "cmd.theme_dark_hint", "dark ss", "dark", "moon"],
    ] as const).map(([id, labelKey, hintKey, initials, mode, icon]) => ({
      id,
      label: t(labelKey),
      hint: t(hintKey),
      initials,
      icon: icon as IconName,
      run: () => setMode(mode as ThemeMode),
    }));
    return [...navCommands, ...orgCommands, ...themeCommands];
  }, [navigate, setMode, language, advancedMode]);

  const filtered = commands.filter((command) => matches(command, query));

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const isPaletteKey = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k";
      if (isPaletteKey) {
        event.preventDefault();
        setOpen((value) => !value);
      } else if (open && event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query, open]);

  if (!open) return null;

  function run(command: Command) {
    command.run();
    setOpen(false);
    setQuery("");
  }

  return (
    <div className="command-overlay" role="dialog" aria-modal="true" aria-label={t("cmd.palette")} onMouseDown={() => setOpen(false)}>
      <div className="command-panel" onMouseDown={(event) => event.stopPropagation()}>
        <input
          autoFocus
          className="command-input"
          placeholder={t("cmd.search_placeholder")}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setActiveIndex((value) => Math.min(value + 1, filtered.length - 1));
            }
            if (event.key === "ArrowUp") {
              event.preventDefault();
              setActiveIndex((value) => Math.max(value - 1, 0));
            }
            if (event.key === "Enter" && filtered[activeIndex]) {
              event.preventDefault();
              run(filtered[activeIndex]);
            }
          }}
        />
        <div className="command-list">
          {filtered.map((command, index) => (
            <button
              key={command.id}
              type="button"
              className={`command-item ${index === activeIndex ? "active" : ""}`}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={() => run(command)}
            >
              <span className="span-icon" aria-hidden="true"><Icon name={command.icon} /></span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-semibold">{command.label}</span>
                <span className="block truncate text-xs muted">{command.hint}</span>
              </span>
            </button>
          ))}
          {filtered.length === 0 && <div className="timeline-empty">{t("cmd.no_results")}</div>}
        </div>
      </div>
    </div>
  );
}
