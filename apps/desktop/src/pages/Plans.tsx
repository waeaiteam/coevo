import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { getActiveOpcId } from "../api/companies";
import { listCompanyWorkOrders } from "../api/org";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";

type WorkOrderRow = Record<string, unknown>;

function stringField(row: WorkOrderRow | undefined, key: string): string {
  const value = row?.[key];
  return value == null ? "" : String(value);
}

function listField(row: WorkOrderRow | undefined, key: string): string {
  const value = row?.[key];
  if (Array.isArray(value)) return value.map(String).join(", ");
  return value == null ? "" : String(value);
}

function dateLabel(row: WorkOrderRow | undefined): string {
  const raw = Number(row?.created_at_ms || row?.updated_at_ms || 0);
  if (!Number.isFinite(raw) || raw <= 0) return "-";
  return new Date(raw).toLocaleString();
}

export default function Plans() {
  useLanguage();
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const [rows, setRows] = useState<WorkOrderRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState("");

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void listCompanyWorkOrders(activeOpcId)
      .then((items) => {
        if (!alive) return;
        setRows(Array.isArray(items) ? items : []);
      })
      .catch(() => {
        if (alive) setRows([]);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [activeOpcId]);

  useEffect(() => {
    if (rows.length === 0) {
      setSelectedId("");
      return;
    }
    const stillExists = rows.some((row) => stringField(row, "work_order_id") === selectedId);
    if (!stillExists) setSelectedId(stringField(rows[0], "work_order_id"));
  }, [rows, selectedId]);

  const summary = useMemo(() => {
    const planned = rows.filter((row) => String(row.status || "") === "Planned").length;
    const waiting = rows.filter((row) => String(row.status || "") === "WaitingApproval").length;
    const completed = rows.filter((row) => String(row.status || "") === "Completed").length;
    return { total: rows.length, planned, waiting, completed };
  }, [rows]);

  const selected = useMemo(() => {
    return rows.find((row) => stringField(row, "work_order_id") === selectedId) || rows[0];
  }, [rows, selectedId]);

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("plans.title")}</div>
          <h1 className="product-title">{t("plans.title")}</h1>
        </div>
        <div className="product-actions">
          <Link className="product-link-button" to="/work-orders">
            <Icon name="layers" /> {t("nav.tasks")}
          </Link>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="calendar" /></div>
        <div>
          <h2>{t("plans.title")}</h2>
          <p>Real persisted work orders that can be reviewed without changing state.</p>
        </div>
      </section>

      <div className="product-grid-2">
        <section className="product-panel">
          <div className="product-panel-heading">
            <h2>Plan queue</h2>
            <span>{summary.total}</span>
          </div>
          <div className="product-grid-3 mb-3">
            <MiniMetric label="Planned" value={summary.planned} />
            <MiniMetric label="Waiting" value={summary.waiting} />
            <MiniMetric label="Completed" value={summary.completed} />
          </div>
          {loading ? (
            <div className="empty-state">
              <p>{t("settings.loading")}</p>
            </div>
          ) : rows.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="calendar" /></div>
              <p>{t("plans.empty")}</p>
            </div>
          ) : (
            <div className="product-list">
              {rows.map((row) => {
                const id = stringField(row, "work_order_id");
                const status = stringField(row, "status");
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
                        {stringField(row, "mission_intent") || "Untitled plan"}
                      </span>
                      <span className="mt-1 block text-[11px]" style={{ color: "var(--text-muted)" }}>
                        {dateLabel(row)}
                      </span>
                    </span>
                    <span className="flex shrink-0 items-center gap-2">
                      <span className="mono-chip">{status || "-"}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </section>

        <section className="product-panel">
          <div className="product-panel-heading">
            <h2>Plan details</h2>
          </div>
          {!selected ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="calendar" /></div>
              <p>{t("plans.empty")}</p>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="rounded border p-3" style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}>
                <div className="text-sm font-semibold">{stringField(selected, "mission_intent") || "Untitled plan"}</div>
                <div className="mt-1 text-xs" style={{ color: "var(--text-muted)" }}>
                  {stringField(selected, "work_order_id") || "-"}
                </div>
              </div>
              <div className="product-grid-2">
                <InfoRow label="Track" value={stringField(selected, "track") || "-"} />
                <InfoRow label="Status" value={stringField(selected, "status") || "-"} />
                <InfoRow label="Assigned" value={listField(selected, "selected_agents") || "-"} />
                <InfoRow label="Skills" value={listField(selected, "required_skills") || "-"} />
                <InfoRow label="Contract" value={stringField(selected, "contract_hash") || "-"} mono />
                <InfoRow label="Created" value={dateLabel(selected)} />
              </div>
              <div className="flex flex-wrap gap-2">
                <Link
                  to={`/tasks/${encodeURIComponent(stringField(selected, "work_order_id"))}`}
                  className="product-link-button"
                >
                  <Icon name="external" /> Open task
                </Link>
                <Link to="/timeline" className="product-link-button">
                  <Icon name="history" /> Timeline
                </Link>
              </div>
            </div>
          )}
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
