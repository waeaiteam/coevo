import { useEffect, useMemo, useState } from "react";
import { useParams, Link } from "react-router-dom";
import Icon from "../components/Icon";
import { listKpi, type KpiRecord } from "../api/org";
import { listEmployees, getAgentGrowth, type AgentGrowth } from "../api/client";
import { getActiveOpcId } from "../api/companies";
import { t, useLanguage } from "../settings/i18n";

type Employee = {
  agent_id: string;
  display_name: string;
  department: string;
  lifecycle_status: string;
  reputation: number | null;
};

const DIM_LABELS: Record<string, string> = {
  completion: "kpi.dim_completion",
  speed: "kpi.dim_speed",
  clarity: "kpi.dim_clarity",
  compliance: "kpi.dim_compliance",
  cost: "kpi.dim_cost",
};

function titleCase(key: string): string {
  return key
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (ch) => ch.toUpperCase())
    .trim();
}

function dimensionLabel(key: string): string {
  const mapped = DIM_LABELS[key];
  return mapped ? t(mapped) : titleCase(key);
}

function asString(value: unknown): string {
  return typeof value === "string" ? value : value == null ? "" : String(value);
}

function asReputation(value: unknown): number | null {
  if (typeof value !== "number" || Number.isNaN(value)) return null;
  return value;
}

function toEmployee(row: Record<string, unknown>): Employee {
  return {
    agent_id: asString(row.agent_id),
    display_name: asString(row.display_name),
    department: asString(row.department),
    lifecycle_status: asString(row.lifecycle_status),
    reputation: asReputation(row.reputation),
  };
}

function averageScore(record: KpiRecord | null): number | null {
  if (!record) return null;
  const values = Object.values(record.scores);
  if (values.length === 0) return null;
  const sum = values.reduce((acc, n) => acc + n, 0);
  return Math.round(sum / values.length);
}

function reputationDisplay(rep: number | null): string {
  if (rep == null) return "—";
  const pct = rep <= 1 ? rep * 100 : rep;
  return String(Math.round(pct));
}

function directionLabel(direction: AgentGrowth["direction"]): string {
  switch (direction) {
    case "improving":
      return t("growth.improving");
    case "declining":
      return t("growth.declining");
    case "steady":
      return t("growth.steady");
    default:
      return t("growth.new");
  }
}

type Promotion = { tone: "green" | "red" | "blue"; label: string };

function promotionStatus(avg: number | null, growth: AgentGrowth | null): Promotion {
  const dir = growth?.direction;
  if ((avg != null && avg >= 85) || dir === "improving") {
    return { tone: "green", label: t("kpi.promotion_ready") };
  }
  if (dir === "declining" || (avg != null && avg < 70)) {
    return { tone: "red", label: t("kpi.promotion_watch") };
  }
  return { tone: "blue", label: t("kpi.promotion_steady") };
}

export default function PerformanceBoard() {
  useLanguage();
  const params = useParams();
  const opcId = params.opcId ? decodeURIComponent(params.opcId) : getActiveOpcId();

  const [employees, setEmployees] = useState<Employee[]>([]);
  const [employeesLoading, setEmployeesLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string>("");

  const [records, setRecords] = useState<KpiRecord[]>([]);
  const [growth, setGrowth] = useState<AgentGrowth | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);

  useEffect(() => {
    let alive = true;
    setEmployeesLoading(true);
    listEmployees()
      .then((rows) => {
        if (!alive) return;
        const mapped = rows.map(toEmployee).filter((e) => e.agent_id);
        setEmployees(mapped);
        setSelectedId((current) => current || mapped[0]?.agent_id || "");
      })
      .catch(() => {
        if (!alive) return;
        setEmployees([]);
      })
      .finally(() => {
        if (alive) setEmployeesLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!selectedId) {
      setRecords([]);
      setGrowth(null);
      return;
    }
    let alive = true;
    setDetailLoading(true);
    Promise.all([
      listKpi(opcId, selectedId),
      getAgentGrowth(selectedId).catch(() => null),
    ])
      .then(([kpi, g]) => {
        if (!alive) return;
        setRecords(Array.isArray(kpi) ? kpi : []);
        setGrowth(g);
      })
      .catch(() => {
        if (!alive) return;
        setRecords([]);
        setGrowth(null);
      })
      .finally(() => {
        if (alive) setDetailLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [opcId, selectedId]);

  const selected = useMemo(
    () => employees.find((e) => e.agent_id === selectedId) || null,
    [employees, selectedId],
  );

  const latest = useMemo(() => {
    if (records.length === 0) return null;
    return records.reduce((acc, r) => (r.created_at_ms > acc.created_at_ms ? r : acc));
  }, [records]);

  const avg = averageScore(latest);
  const promotion = promotionStatus(avg, growth);

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("kpi.kicker")}</div>
          <h1 className="product-title">{t("kpi.title")}</h1>
          <p className="product-subtitle">{t("kpi.subtitle")}</p>
        </div>
        <div className="product-actions">
          <Link to={`/companies/${encodeURIComponent(opcId)}`} className="product-link-button">
            <Icon name="chevron-right" style={{ transform: "rotate(180deg)" }} /> {t("companies.title")}
          </Link>
        </div>
      </header>

      <div className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("kpi.select_employee")}</h2>
            <span>{employees.length}</span>
          </div>
          {employeesLoading ? (
            <div className="product-empty">{t("settings.loading")}</div>
          ) : employees.length === 0 ? (
            <div className="product-empty">{t("kpi.no_employees")}</div>
          ) : (
            <div className="product-list">
              {employees.map((emp) => {
                const active = emp.agent_id === selectedId;
                return (
                  <button
                    key={emp.agent_id}
                    type="button"
                    className="product-list-row"
                    onClick={() => setSelectedId(emp.agent_id)}
                    style={active ? { borderColor: "var(--accent)" } : undefined}
                  >
                    <span className="product-row-main">{emp.display_name || emp.agent_id}</span>
                    {emp.department && <span className="product-row-meta">{emp.department}</span>}
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div className="product-panel">
          {!selected ? (
            <div className="empty-state">
              <div className="empty-state-icon">
                <Icon name="gauge" />
              </div>
              <p>{t("kpi.select_employee")}</p>
            </div>
          ) : (
            <div className="product-prose">
              <div className="product-panel-heading">
                <h2>{selected.display_name || selected.agent_id}</h2>
                <span className="mono-chip">{selected.agent_id}</span>
              </div>

              <div className="product-metrics-grid">
                <div className="product-metric">
                  <div className="product-metric-value">{avg != null ? avg : "—"}</div>
                  <div className="product-metric-label">{t("kpi.average")}</div>
                </div>
                <div className="product-metric">
                  <div className="product-metric-value">
                    {growth ? `${growth.success_rate}%` : "—"}
                  </div>
                  <div className="product-metric-label">{t("growth.success_rate")}</div>
                </div>
                <div className="product-metric">
                  <div className="product-metric-value">
                    {growth ? growth.total_tasks : "—"}
                  </div>
                  <div className="product-metric-label">{t("growth.total_tasks")}</div>
                </div>
                <div className="product-metric">
                  <div className="product-metric-value">{reputationDisplay(selected.reputation)}</div>
                  <div className="product-metric-label">{t("office.current_score")}</div>
                </div>
              </div>

              <div className="product-panel">
                <div className="product-panel-heading">
                  <h2>{t("kpi.promotion")}</h2>
                  <span className={`product-pill ${promotion.tone}`}>
                    <Icon name="badge-check" /> {promotion.label}
                  </span>
                </div>
                {growth && (
                  <p className="product-row-meta">{directionLabel(growth.direction)}</p>
                )}
              </div>

              <div className="product-panel">
                <div className="product-panel-heading">
                  <h2>{t("kpi.latest_scores")}</h2>
                  <Icon name="sparkles" />
                </div>
                {detailLoading ? (
                  <div className="product-empty">{t("settings.loading")}</div>
                ) : !latest ? (
                  <div className="product-empty">{t("kpi.no_records")}</div>
                ) : (
                  <div className="kpi-score-grid">
                    {Object.entries(latest.scores).map(([dim, score]) => (
                      <div key={dim} className="kpi-score">
                        <div className="kpi-score-value">{score}</div>
                        <div className="kpi-score-label">{dimensionLabel(dim)}</div>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="product-panel">
                <div className="product-panel-heading">
                  <h2>{t("kpi.history")}</h2>
                  <span>{records.length}</span>
                </div>
                {detailLoading ? (
                  <div className="product-empty">{t("settings.loading")}</div>
                ) : records.length === 0 ? (
                  <div className="product-empty">{t("kpi.no_records")}</div>
                ) : (
                  <div className="product-list">
                    {records.map((rec) => (
                      <div key={rec.work_order_id} className="product-list-row">
                        <span className="product-row-main">
                          <span className="mono-chip">{rec.work_order_id}</span>
                          {rec.comment ? ` ${rec.comment}` : ""}
                        </span>
                        <span className="product-row-meta">
                          {t("kpi.reviewed_by")} {rec.reviewer}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
