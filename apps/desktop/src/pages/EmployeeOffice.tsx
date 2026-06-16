import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import Icon, { type IconName } from "../components/Icon";
import { getActiveOpcId } from "../api/companies";
import { getCompanyEmployee } from "../api/org";
import AgentWorkbenchPanel from "../components/AgentWorkbenchPanel";
import EmployeePlayground from "../components/EmployeePlayground";
import Evaluations from "./Evaluations";
import Traces from "./Traces";
import EmployeeGrowth from "./EmployeeGrowth";
import { t, useLanguage } from "../settings/i18n";

type TabId = "instructions" | "playground" | "quality" | "replay" | "growth";

const TABS: Array<{ id: TabId; labelKey: string; hintKey: string; icon: IconName }> = [
  { id: "instructions", labelKey: "office.tab_instructions", hintKey: "office.tab_instructions_hint", icon: "file-text" },
  { id: "playground", labelKey: "office.tab_playground", hintKey: "office.tab_playground_hint", icon: "sparkles" },
  { id: "quality", labelKey: "office.tab_quality", hintKey: "office.tab_quality_hint", icon: "badge-check" },
  { id: "replay", labelKey: "office.tab_replay", hintKey: "office.tab_replay_hint", icon: "history" },
  { id: "growth", labelKey: "office.tab_growth", hintKey: "office.tab_growth_hint", icon: "gauge" },
];

function str(row: Record<string, unknown> | null, key: string): string {
  return row ? String(row[key] ?? "") : "";
}

export default function EmployeeOffice() {
  useLanguage();
  const params = useParams();
  const agentId = params.agentId ? decodeURIComponent(params.agentId) : "";
  const opcId = getActiveOpcId();

  const [employee, setEmployee] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<TabId>("instructions");

  async function load() {
    setLoading(true);
    try {
      const e = await getCompanyEmployee(opcId, agentId);
      setEmployee(e);
    } catch {
      setEmployee(null);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (agentId) void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId]);

  const displayName = str(employee, "display_name") || agentId;
  const lifecycle = str(employee, "lifecycle_status").toLowerCase();
  const active = lifecycle === "active";
  const reputation = useMemo(() => {
    const raw = employee?.reputation;
    const n = Number(raw);
    if (Number.isFinite(n) && n > 0) return n <= 1 ? Math.round(n * 100) : Math.round(n);
    return null;
  }, [employee]);
  const activeHint = TABS.find((entry) => entry.id === tab)?.hintKey || "";

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("office.emp_kicker")}</div>
          <h1 className="product-title">{displayName}</h1>
        </div>
        <div className="product-actions">
          <Link to={`/companies/${encodeURIComponent(opcId)}/office`} className="product-link-button">
            <Icon name="chevron-right" style={{ transform: "rotate(180deg)" }} /> {t("office.back_to_office")}
          </Link>
        </div>
      </header>

      {loading && <div className="product-empty">{t("settings.loading")}</div>}

      {!loading && !employee && (
        <div className="empty-state">
          <div className="empty-state-icon"><Icon name="user" /></div>
          <p>{t("office.not_found")}</p>
        </div>
      )}

      {!loading && employee && (
        <>
          <section className="product-panel">
            <div className="office-summary">
              <div className="office-summary-avatar"><Icon name="user" /></div>
              <div className="min-w-0 flex-1">
                <div className="office-summary-id">{agentId}</div>
                <p className="product-prose" style={{ marginTop: 4 }}>{t("office.summary_hint")}</p>
              </div>
              <div className="flex items-center gap-2">
                <span className={`product-pill ${active ? "green" : "yellow"}`}>
                  {active ? t("office.status_active") : t("office.status_idle")}
                </span>
                {reputation != null && <span className="mono-chip">{t("office.current_score")} {reputation}</span>}
              </div>
            </div>
          </section>

          <nav className="office-tabs" aria-label={t("office.emp_kicker")}>
            {TABS.map((entry) => (
              <button
                key={entry.id}
                type="button"
                className={`office-tab ${tab === entry.id ? "active" : ""}`}
                aria-current={tab === entry.id}
                onClick={() => setTab(entry.id)}
              >
                <Icon name={entry.icon} /> {t(entry.labelKey)}
              </button>
            ))}
          </nav>
          <p className="office-tab-hint">{t(activeHint)}</p>

          {tab === "instructions" && (
            <AgentWorkbenchPanel employee={employee} onChanged={load} onDeleted={load} />
          )}
          {tab === "playground" && (
            <EmployeePlayground agentId={agentId} initialPrompt={str(employee, "system_prompt")} />
          )}
          {tab === "quality" && <Evaluations embedded />}
          {tab === "replay" && <Traces embedded />}
          {tab === "growth" && <EmployeeGrowth embedded />}
        </>
      )}
    </div>
  );
}
