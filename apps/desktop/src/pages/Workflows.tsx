import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { getActiveOpcId } from "../api/companies";
import { getCompanyTraceSpans, listCompanyTraces } from "../api/client";
import { listCompanyWorkOrders } from "../api/org";
import Icon from "../components/Icon";
import { TraceWaterfall, type TraceSpan } from "../components/TraceWaterfall";
import { t, useLanguage } from "../settings/i18n";

type Row = Record<string, unknown>;

function stringField(row: Row | undefined, key: string): string {
  const value = row?.[key];
  return value == null ? "" : String(value);
}

function parseTraceSpans(payload: Record<string, unknown>): TraceSpan[] {
  const spans = payload.spans;
  return Array.isArray(spans) ? (spans as TraceSpan[]) : [];
}

export default function Workflows() {
  useLanguage();
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const [workOrders, setWorkOrders] = useState<Row[]>([]);
  const [traces, setTraces] = useState<Row[]>([]);
  const [spans, setSpans] = useState<TraceSpan[]>([]);
  const [selectedTraceId, setSelectedTraceId] = useState("");
  const [loading, setLoading] = useState(true);

  const traceByWorkOrder = useMemo(() => {
    const map = new Map<string, string>();
    for (const trace of traces) {
      const workOrderId = stringField(trace, "work_order_id");
      const traceId = stringField(trace, "trace_id");
      if (workOrderId && traceId) map.set(workOrderId, traceId);
    }
    return map;
  }, [traces]);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void Promise.all([listCompanyWorkOrders(activeOpcId), listCompanyTraces(activeOpcId)])
      .then(([orders, traceRows]) => {
        if (!alive) return;
        const nextOrders = Array.isArray(orders) ? orders : [];
        const nextTraces = Array.isArray(traceRows) ? traceRows : [];
        setWorkOrders(nextOrders);
        setTraces(nextTraces);
        const nextTraceId = stringField(nextTraces[0], "trace_id");
        setSelectedTraceId((current) => current || nextTraceId);
        if (!nextTraceId && alive) {
          setSpans([]);
        }
      })
      .catch(() => {
        if (!alive) return;
        setWorkOrders([]);
        setTraces([]);
        setSpans([]);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [activeOpcId]);

  useEffect(() => {
    if (!selectedTraceId) {
      setSpans([]);
      return;
    }
    let alive = true;
    void getCompanyTraceSpans(activeOpcId, selectedTraceId)
      .then((detail) => {
        if (alive) setSpans(parseTraceSpans(detail));
      })
      .catch(() => {
        if (alive) setSpans([]);
      });
    return () => {
      alive = false;
    };
  }, [activeOpcId, selectedTraceId]);

  const summary = useMemo(() => {
    const running = traces.filter((trace) => String(trace.status || "") === "running").length;
    const completed = traces.filter((trace) => String(trace.status || "") === "completed").length;
    return { workOrders: workOrders.length, traces: traces.length, running, completed };
  }, [workOrders, traces]);

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("workflows.kicker")}</div>
          <h1 className="product-title">{t("workflows.title")}</h1>
        </div>
        <div className="product-actions">
          <Link to="/tasks" className="product-link-button">
            <Icon name="layers" /> {t("nav.today")}
          </Link>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="history" /></div>
        <div>
          <h2>{t("workflows.title")}</h2>
          <p>{t("workflows.hero_desc")}</p>
        </div>
      </section>

      <div className="product-grid-2">
        <section className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("workflows.flows")}</h2>
            <span>{summary.workOrders}</span>
          </div>
          <div className="product-grid-3 mb-3">
            <MiniMetric label={t("workflows.traces")} value={summary.traces} />
            <MiniMetric label={t("workflows.running")} value={summary.running} />
            <MiniMetric label={t("workflows.completed")} value={summary.completed} />
          </div>
          {loading ? (
            <div className="empty-state"><p>{t("settings.loading")}</p></div>
          ) : workOrders.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="history" /></div>
              <p>{t("plans.empty")}</p>
            </div>
          ) : (
            <div className="product-list">
              {workOrders.map((row) => {
                const id = stringField(row, "work_order_id");
                const traceId = traceByWorkOrder.get(id);
                const active = traceId === selectedTraceId;
                return (
                  <button
                    key={id}
                    type="button"
                    className="product-list-row text-left"
                    disabled={!traceId}
                    title={traceId ? undefined : t("workflows.no_traces")}
                    onClick={() => traceId && setSelectedTraceId(traceId)}
                    style={{ borderColor: active ? "var(--accent)" : undefined, opacity: traceId ? 1 : 0.55, cursor: traceId ? "pointer" : "default" }}
                  >
                    <span className="min-w-0">
                      <span className="product-row-main block truncate">
                        {stringField(row, "mission_intent") || t("workflows.untitled")}
                      </span>
                      <span className="mt-1 block text-[11px]" style={{ color: "var(--text-muted)" }}>
                        {stringField(row, "status") || "-"} · {stringField(row, "track") || "-"}
                      </span>
                    </span>
                    <span className="flex shrink-0 items-center gap-2">
                      <span className="mono-chip">{id || "-"}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </section>

        <section className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("workflows.waterfall")}</h2>
          </div>
          {traces.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="layers" /></div>
              <p>{t("workflows.no_traces")}</p>
            </div>
          ) : (
            <>
              <div className="product-list mb-3">
                {traces.map((trace) => {
                  const traceId = stringField(trace, "trace_id");
                  return (
                    <button
                      key={traceId}
                      type="button"
                      className="product-list-row text-left"
                      onClick={() => setSelectedTraceId(traceId)}
                      style={{ borderColor: traceId === selectedTraceId ? "var(--accent)" : undefined }}
                    >
                      <span className="product-row-main">{stringField(trace, "work_order_id") || traceId}</span>
                      <span className="flex items-center gap-2">
                        <span className="mono-chip">{stringField(trace, "status") || "-"}</span>
                      </span>
                    </button>
                  );
                })}
              </div>
              {spans.length === 0 ? (
                <div className="empty-state">
                  <p>{t("workflows.no_spans")}</p>
                </div>
              ) : (
                <TraceWaterfall spans={spans} />
              )}
            </>
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
