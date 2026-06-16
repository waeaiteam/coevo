import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { getActiveOpcId } from "../api/companies";
import { getCompanyTraceSpans, listCompanyTraces } from "../api/client";
import { listCompanyWorkOrders } from "../api/org";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";

type Row = Record<string, unknown>;
type TraceSpan = {
  span_id: string;
  parent_span_id?: string | null;
  name: string;
  kind: string;
  status: string;
  started_at_ms: number;
  ended_at_ms?: number | null;
};

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
          <Link to="/work-orders" className="product-link-button">
            <Icon name="layers" /> {t("nav.tasks")}
          </Link>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="history" /></div>
        <div>
          <h2>{t("workflows.title")}</h2>
          <p>Operational workflow traces and work-order activity pulled from the live company scope.</p>
        </div>
      </section>

      <div className="product-grid-2">
        <section className="product-panel">
          <div className="product-panel-heading">
            <h2>Operational flows</h2>
            <span>{summary.workOrders}</span>
          </div>
          <div className="product-grid-3 mb-3">
            <MiniMetric label="Traces" value={summary.traces} />
            <MiniMetric label="Running" value={summary.running} />
            <MiniMetric label="Completed" value={summary.completed} />
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
                    onClick={() => traceId && setSelectedTraceId(traceId)}
                    style={{ borderColor: active ? "var(--accent)" : undefined }}
                  >
                    <span className="min-w-0">
                      <span className="product-row-main block truncate">
                        {stringField(row, "mission_intent") || "Untitled flow"}
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
            <h2>Trace waterfall</h2>
          </div>
          {traces.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="layers" /></div>
              <p>No company traces were returned yet.</p>
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
                  <p>No trace spans were returned for the selected record.</p>
                </div>
              ) : (
                <Waterfall spans={spans} />
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

function Waterfall({ spans }: { spans: TraceSpan[] }) {
  const minStart = Math.min(...spans.map((span) => span.started_at_ms));
  const maxEnd = Math.max(...spans.map((span) => span.ended_at_ms || Date.now()));
  const totalDuration = Math.max(1, maxEnd - minStart);
  const depthMap = new Map<string, number>();
  const sorted = [...spans].sort((a, b) => a.started_at_ms - b.started_at_ms);

  for (const span of sorted) {
    const parentDepth = span.parent_span_id ? depthMap.get(span.parent_span_id) ?? 0 : -1;
    depthMap.set(span.span_id, parentDepth + 1);
  }

  return (
    <div className="trace-waterfall">
      {sorted.map((span) => {
        const offset = ((span.started_at_ms - minStart) / totalDuration) * 100;
        const width = (((span.ended_at_ms || Date.now()) - span.started_at_ms) / totalDuration) * 100;
        const depth = depthMap.get(span.span_id) ?? 0;
        const color = span.status === "error" ? "var(--red)" : span.status === "running" ? "var(--blue)" : "var(--accent)";
        return (
          <div key={span.span_id} className="trace-span-row">
            <div className="trace-span-label" style={{ paddingLeft: depth * 14 }}>
              <Icon name={kindIcon(span.kind)} />
              <span className="truncate">{span.name}</span>
            </div>
            <div className="trace-span-track">
              <div
                className="trace-span-bar"
                style={{ marginLeft: `${offset}%`, width: `${Math.max(2, width)}%`, background: color }}
                title={`${(span.ended_at_ms || Date.now()) - span.started_at_ms}ms`}
              />
            </div>
            <span className="trace-span-time">
              {span.ended_at_ms != null ? `${span.ended_at_ms - span.started_at_ms}ms` : t("traces.running")}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function kindIcon(kind: string) {
  switch (kind) {
    case "mission":
      return "sparkles" as const;
    case "subtask":
      return "list-checks" as const;
    case "model_call":
      return "brain" as const;
    case "tool_call":
      return "wrench" as const;
    case "governance":
      return "shield-check" as const;
    default:
      return "info" as const;
  }
}
