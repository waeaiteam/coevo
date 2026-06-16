import { useEffect, useMemo, useState } from "react";
import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";
import { getActiveOpcId } from "../api/companies";
import { getCompanyTraceSpans, listCompanyTraces } from "../api/client";

type TraceRow = {
  trace_id: string;
  work_order_id?: string;
  agent_id?: string;
  status: string;
  started_at_ms: number;
  ended_at_ms?: number | null;
  total_tokens?: number;
  total_cost_usd?: number;
};

type TraceSpan = {
  span_id: string;
  parent_span_id?: string | null;
  name: string;
  kind: string;
  status: string;
  started_at_ms: number;
  ended_at_ms?: number | null;
  input?: string;
  output?: string;
};

export default function Traces({ embedded = false }: { embedded?: boolean }) {
  useLanguage();
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const [traces, setTraces] = useState<TraceRow[]>([]);
  const [activeTraceId, setActiveTraceId] = useState<string | null>(null);
  const [spans, setSpans] = useState<TraceSpan[]>([]);
  const [loadingTraces, setLoadingTraces] = useState(true);
  const [loadingSpans, setLoadingSpans] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    setLoadingTraces(true);
    setError("");
    void listCompanyTraces(activeOpcId)
      .then((rows) => {
        if (cancelled) return;
        const next = Array.isArray(rows) ? (rows as TraceRow[]) : [];
        setTraces(next);
        setActiveTraceId((current) => current || next[0]?.trace_id || null);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoadingTraces(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeOpcId]);

  useEffect(() => {
    if (!activeTraceId) {
      setSpans([]);
      return;
    }
    let cancelled = false;
    setLoadingSpans(true);
    setError("");
    void getCompanyTraceSpans(activeOpcId, activeTraceId)
      .then((body) => {
        if (cancelled) return;
        const next = Array.isArray(body.spans) ? (body.spans as TraceSpan[]) : [];
        setSpans(next);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoadingSpans(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeOpcId, activeTraceId]);

  return (
    <div className={embedded ? "space-y-4" : "product-page"}>
      {embedded ? null : (
        <>
          <header className="product-header">
            <div className="min-w-0">
              <div className="product-kicker">{t("traces.kicker")}</div>
              <h1 className="product-title">{t("traces.title")}</h1>
            </div>
          </header>

          <section className="feature-hero">
            <div className="feature-hero-icon"><Icon name="history" /></div>
            <div>
              <h2>{t("traces.title")}</h2>
              <p>{t("traces.desc")}</p>
            </div>
          </section>
        </>
      )}

      <div className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("traces.list")}</h2>
            <span>{traces.length}</span>
          </div>
          {loadingTraces ? (
            <div className="empty-state"><p>{t("executors.loading")}</p></div>
          ) : traces.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="history" /></div>
              <p>{t("traces.empty")}</p>
            </div>
          ) : (
            <div className="product-list">
              {traces.map((trace) => (
                <button
                  key={trace.trace_id}
                  className="product-list-row"
                  onClick={() => setActiveTraceId(trace.trace_id)}
                  style={{ borderColor: trace.trace_id === activeTraceId ? "var(--accent)" : undefined }}
                >
                  <span className="product-row-main">{trace.work_order_id || trace.trace_id}</span>
                  <span className="flex items-center gap-2">
                    <span className="mono-chip">{trace.status}</span>
                    <span className="mono-chip">
                      {trace.ended_at_ms != null ? `${trace.ended_at_ms - trace.started_at_ms}ms` : t("traces.running")}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>

        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("traces.waterfall")}</h2>
          </div>
          {loadingSpans ? (
            <div className="empty-state"><p>{t("executors.loading")}</p></div>
          ) : spans.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="layers" /></div>
              <p>{error || t("traces.select")}</p>
            </div>
          ) : (
            <Waterfall spans={spans} />
          )}
        </div>
      </div>
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
