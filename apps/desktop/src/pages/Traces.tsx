import { useMemo, useState } from "react";
import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";
import { useTraceStore } from "../stores/traceStore";
import { tracer } from "../tracing/tracer";
import type { Span } from "../tracing/tracer";

export default function Traces() {
  useLanguage();
  const { traces, spans, activeTraceId, setActiveTrace, clearTraces } = useTraceStore();

  const traceList = useMemo(
    () => Object.values(traces).sort((a, b) => b.start_time - a.start_time),
    [traces]
  );
  const activeSpans = activeTraceId ? spans[activeTraceId] || [] : [];

  function emitDemoTrace() {
    const root = tracer.startSpan(t("traces.sample_mission"), "mission", {
      attributes: { user: "founder" },
      input: t("traces.sample_intent"),
    });
    const plan = root.child(t("traces.sample_plan"), "subtask");
    setTimeout(() => {
      plan.setTokens(120, 340, 0.004).setOutput("3 subtasks created");
      plan.end("ok");
      const model = root.child(t("traces.sample_think"), "model_call");
      setTimeout(() => {
        model.setTokens(800, 1200, 0.03).setOutput("Analysis complete");
        model.end("ok");
        const gov = root.child(t("traces.sample_check"), "governance");
        setTimeout(() => {
          gov.setAttribute("track", "green").end("ok");
          root.setTokens(920, 1540, 0.034).setOutput("Mission complete");
          root.end("ok");
        }, 250);
      }, 400);
    }, 300);
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("traces.kicker")}</div>
          <h1 className="product-title">{t("traces.title")}</h1>
        </div>
        <div className="product-actions">
          <button className="product-link-button" onClick={emitDemoTrace}>
            <Icon name="sparkles" /> {t("traces.demo")}
          </button>
          <button className="product-link-button" onClick={clearTraces}>
            <Icon name="x" /> {t("traces.clear")}
          </button>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="history" /></div>
        <div>
          <h2>{t("traces.title")}</h2>
          <p>{t("traces.desc")}</p>
        </div>
      </section>

      <div className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("traces.list")}</h2>
            <span>{traceList.length}</span>
          </div>
          {traceList.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="history" /></div>
              <p>{t("traces.empty")}</p>
            </div>
          ) : (
            <div className="product-list">
              {traceList.map((tr) => (
                <button
                  key={tr.trace_id}
                  className="product-list-row"
                  onClick={() => setActiveTrace(tr.trace_id)}
                  style={{ borderColor: tr.trace_id === activeTraceId ? "var(--accent)" : undefined }}
                >
                  <span className="product-row-main">{tr.name}</span>
                  <span className="flex items-center gap-2">
                    <span className="mono-chip">{tr.span_count} {t("traces.steps")}</span>
                    <span className={`product-pill ${tr.status === "ok" ? "green" : tr.status === "error" ? "red" : "blue"}`}>
                      {tr.duration_ms != null ? `${tr.duration_ms}ms` : t("traces.running")}
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
          {activeSpans.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="layers" /></div>
              <p>{t("traces.select")}</p>
            </div>
          ) : (
            <Waterfall spans={activeSpans} />
          )}
        </div>
      </div>
    </div>
  );
}

function Waterfall({ spans }: { spans: Span[] }) {
  const minStart = Math.min(...spans.map((s) => s.start_time));
  const maxEnd = Math.max(...spans.map((s) => s.end_time || Date.now()));
  const totalDuration = Math.max(1, maxEnd - minStart);

  // Sort by start, then nest by parent for indentation depth.
  const depthMap = new Map<string, number>();
  const sorted = [...spans].sort((a, b) => a.start_time - b.start_time);
  for (const s of sorted) {
    const parentDepth = s.parent_span_id != null ? depthMap.get(s.parent_span_id) ?? 0 : -1;
    depthMap.set(s.span_id, parentDepth + 1);
  }

  return (
    <div className="trace-waterfall">
      {sorted.map((s) => {
        const offset = ((s.start_time - minStart) / totalDuration) * 100;
        const width = (((s.end_time || Date.now()) - s.start_time) / totalDuration) * 100;
        const depth = depthMap.get(s.span_id) ?? 0;
        const color =
          s.status === "error" ? "var(--red)" : s.status === "running" ? "var(--blue)" : "var(--accent)";
        return (
          <div key={s.span_id} className="trace-span-row">
            <div className="trace-span-label" style={{ paddingLeft: depth * 14 }}>
              <Icon name={kindIcon(s.kind)} />
              <span className="truncate">{s.name}</span>
            </div>
            <div className="trace-span-track">
              <div
                className="trace-span-bar"
                style={{ marginLeft: `${offset}%`, width: `${Math.max(2, width)}%`, background: color }}
                title={`${s.duration_ms ?? "?"}ms`}
              />
            </div>
            <span className="trace-span-time">{s.duration_ms != null ? `${s.duration_ms}ms` : "…"}</span>
          </div>
        );
      })}
    </div>
  );
}

function kindIcon(kind: Span["kind"]) {
  switch (kind) {
    case "mission": return "sparkles" as const;
    case "subtask": return "list-checks" as const;
    case "model_call": return "brain" as const;
    case "tool_call": return "wrench" as const;
    case "governance": return "shield-check" as const;
    default: return "info" as const;
  }
}
