import { useEffect, useMemo, useState } from "react";
import { getActiveOpcId } from "../api/companies";
import { listCompanyAuditEvents, listCompanyWorkOrders } from "../api/org";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";

type WorkOrderRow = Record<string, unknown>;
type AuditRow = Record<string, unknown>;

function stringField(row: Record<string, unknown> | undefined, key: string): string {
  const value = row?.[key];
  return value == null ? "" : String(value);
}

function parseJson(value: unknown): Record<string, unknown> {
  if (typeof value !== "string") return {};
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed as Record<string, unknown> : {};
  } catch {
    return {};
  }
}

function timeLabel(row: Record<string, unknown> | undefined): string {
  const raw = Number(row?.recorded_at_ms || row?.created_at_ms || 0);
  if (!Number.isFinite(raw) || raw <= 0) return "-";
  return new Date(raw).toLocaleString();
}

export default function RiskGate() {
  useLanguage();
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const [workOrders, setWorkOrders] = useState<WorkOrderRow[]>([]);
  const [auditEvents, setAuditEvents] = useState<AuditRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState("");

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void Promise.all([
      listCompanyWorkOrders(activeOpcId),
      listCompanyAuditEvents(activeOpcId, { limit: 40 }),
    ])
      .then(([orders, audits]) => {
        if (!alive) return;
        setWorkOrders(Array.isArray(orders) ? orders : []);
        setAuditEvents(Array.isArray(audits) ? audits : []);
      })
      .catch(() => {
        if (!alive) {
          return;
        }
        setWorkOrders([]);
        setAuditEvents([]);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [activeOpcId]);

  useEffect(() => {
    if (workOrders.length === 0) {
      setSelectedId("");
      return;
    }
    const stillExists = workOrders.some((row) => stringField(row, "work_order_id") === selectedId);
    if (!stillExists) setSelectedId(stringField(workOrders[0], "work_order_id"));
  }, [workOrders, selectedId]);

  const summary = useMemo(() => {
    const waiting = workOrders.filter((row) => String(row.status || "") === "WaitingApproval").length;
    const blocked = workOrders.filter((row) => String(row.track || "") === "red").length;
    const completed = workOrders.filter((row) => String(row.status || "") === "Completed").length;
    return { total: workOrders.length, waiting, blocked, completed, audits: auditEvents.length };
  }, [workOrders, auditEvents]);

  const selected = useMemo(() => {
    return workOrders.find((row) => stringField(row, "work_order_id") === selectedId) || workOrders[0];
  }, [workOrders, selectedId]);

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("risk.title")}</div>
          <h1 className="product-title">{t("risk.title")}</h1>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="shield-check" /></div>
        <div>
          <h2>{t("risk.title")}</h2>
          <p>{t("riskgate.hero_desc")}</p>
        </div>
      </section>

      <div className="product-grid-2">
        <section className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("riskgate.queue")}</h2>
            <span>{summary.total}</span>
          </div>
          <div className="product-grid-3 mb-3">
            <MiniMetric label={t("riskgate.waiting")} value={summary.waiting} />
            <MiniMetric label={t("riskgate.blocked")} value={summary.blocked} />
            <MiniMetric label={t("riskgate.completed")} value={summary.completed} />
          </div>
          {loading ? (
            <div className="empty-state">
              <p>{t("settings.loading")}</p>
            </div>
          ) : workOrders.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="shield-check" /></div>
              <p>{t("risk.summary")}</p>
            </div>
          ) : (
            <div className="product-list">
              {workOrders.map((row) => {
                const id = stringField(row, "work_order_id");
                const active = id === selectedId;
                return (
                  <button
                    key={id}
                    type="button"
                    className="product-list-row text-left"
                    onClick={() => setSelectedId(id)}
                    style={{ borderColor: active ? "var(--accent)" : undefined }}
                  >
                    <span className="min-w-0">
                      <span className="product-row-main block truncate">
                        {stringField(row, "mission_intent") || t("riskgate.untitled")}
                      </span>
                      <span className="mt-1 block text-[11px]" style={{ color: "var(--text-muted)" }}>
                        {timeLabel(row)}
                      </span>
                    </span>
                    <span className="flex shrink-0 items-center gap-2">
                      <span className="mono-chip">{stringField(row, "status") || "-"}</span>
                      <span className="mono-chip">{stringField(row, "track") || "-"}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </section>

        <section className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("riskgate.recent_audit")}</h2>
            <span>{summary.audits}</span>
          </div>
          {auditEvents.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="history" /></div>
              <p>{t("audit.empty")}</p>
            </div>
          ) : (
            <div className="space-y-2">
              {auditEvents.map((event) => {
                const data = parseJson(event.event_data_json);
                return (
                  <div key={stringField(event, "id")} className="rounded border p-3" style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}>
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-semibold">{stringField(event, "event_type")}</div>
                        <div className="mt-1 text-[11px]" style={{ color: "var(--text-muted)" }}>{timeLabel(event)}</div>
                      </div>
                      <span className="mono-chip">{String(data.decision || "recorded")}</span>
                    </div>
                    <div className="mt-2 text-xs" style={{ color: "var(--text-secondary)" }}>
                      {String(data.work_order_id || data.approval_id || data.reason || "-")}
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          <div className="mt-4">
            <div className="product-panel-heading">
              <h2>Selected work order</h2>
            </div>
            {!selected ? (
              <div className="empty-state">
                <p>{t("risk.summary")}</p>
              </div>
            ) : (
              <div className="product-grid-2">
                <InfoRow label={t("riskgate.work_order")} value={stringField(selected, "work_order_id") || "-"} mono />
                <InfoRow label={t("riskgate.intent")} value={stringField(selected, "mission_intent") || "-"} />
                <InfoRow label={t("riskgate.track")} value={stringField(selected, "track") || "-"} />
                <InfoRow label={t("riskgate.status")} value={stringField(selected, "status") || "-"} />
                <InfoRow label={t("riskgate.assigned")} value={stringField(selected, "selected_agents") || "-"} />
                <InfoRow label={t("riskgate.skills")} value={stringField(selected, "required_skills") || "-"} />
              </div>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}

function MiniMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded border p-3" style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}>
      <div className="text-lg font-semibold">{value}</div>
      <div className="text-[11px]" style={{ color: "var(--text-muted)" }}>{label}</div>
    </div>
  );
}

function InfoRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="rounded border p-3" style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}>
      <div className="text-[11px] uppercase" style={{ color: "var(--text-muted)" }}>{label}</div>
      <div className={`mt-1 text-xs ${mono ? "font-mono" : ""}`} style={{ color: "var(--text-secondary)" }}>{value}</div>
    </div>
  );
}
