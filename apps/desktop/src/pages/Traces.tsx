import { useEffect, useMemo, useState } from "react";
import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";
import { TraceWaterfall, type TraceSpan } from "../components/TraceWaterfall";
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
            <TraceWaterfall spans={spans} />
          )}
        </div>
      </div>
    </div>
  );
}
