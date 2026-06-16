import { useEffect, useMemo, useState } from "react";
import { getActiveOpcId } from "../api/companies";
import { listCompanyTraces } from "../api/client";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";

type TraceRow = {
  trace_id: string;
  status?: string | null;
  started_at_ms?: number | null;
  ended_at_ms?: number | null;
  duration_ms?: number | null;
  total_tokens?: number | null;
  total_cost_usd?: number | null;
};

function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length));
  return sorted[idx];
}

function fmtMs(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "--";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function toNumber(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function traceDuration(row: TraceRow): number | null {
  if (typeof row.duration_ms === "number" && Number.isFinite(row.duration_ms) && row.duration_ms >= 0) {
    return row.duration_ms;
  }
  if (
    typeof row.started_at_ms === "number" &&
    Number.isFinite(row.started_at_ms) &&
    typeof row.ended_at_ms === "number" &&
    Number.isFinite(row.ended_at_ms)
  ) {
    return Math.max(0, row.ended_at_ms - row.started_at_ms);
  }
  return null;
}

export default function Performance() {
  useLanguage();
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const [traces, setTraces] = useState<TraceRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError("");
    void listCompanyTraces(activeOpcId)
      .then((rows) => {
        if (cancelled) return;
        setTraces(Array.isArray(rows) ? (rows as TraceRow[]) : []);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setTraces([]);
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeOpcId]);

  const metrics = useMemo(() => {
    const durations = traces.map(traceDuration).filter((value): value is number => value != null);
    const totalTokens = traces.reduce((sum, tr) => sum + toNumber(tr.total_tokens), 0);
    const totalCost = traces.reduce((sum, tr) => sum + toNumber(tr.total_cost_usd), 0);
    const errorCount = traces.filter((tr) => String(tr.status || "").toLowerCase() === "error").length;
    return {
      traceCount: traces.length,
      typical: percentile(durations, 50),
      slower: percentile(durations, 90),
      slowest: percentile(durations, 99),
      totalTokens,
      totalCost,
      errorRate: traces.length > 0 ? (errorCount / traces.length) * 100 : 0,
    };
  }, [traces]);

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("perf.kicker")}</div>
          <h1 className="product-title">{t("perf.title")}</h1>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="gauge" /></div>
        <div>
          <h2>{t("perf.title")}</h2>
          <p>{t("perf.desc")}</p>
        </div>
      </section>

      {loading ? (
        <div className="empty-state">
          <div className="empty-state-icon"><Icon name="gauge" /></div>
          <p>{t("settings.loading")}</p>
        </div>
      ) : error ? (
        <div className="empty-state">
          <div className="empty-state-icon"><Icon name="gauge" /></div>
          <p>{error}</p>
        </div>
      ) : metrics.traceCount === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon"><Icon name="gauge" /></div>
          <p>{t("perf.empty")}</p>
        </div>
      ) : (
        <>
          <section className="product-metrics-grid">
            <Metric label={t("perf.tasks")} value={String(metrics.traceCount)} />
            <Metric label={t("perf.typical")} value={fmtMs(metrics.typical)} />
            <Metric label={t("perf.slower")} value={fmtMs(metrics.slower)} />
            <Metric label={t("perf.slowest")} value={fmtMs(metrics.slowest)} />
            <Metric label={t("perf.tokens")} value={metrics.totalTokens.toLocaleString()} />
            <Metric label={t("perf.cost")} value={`$${metrics.totalCost.toFixed(4)}`} />
            <Metric
              label={t("perf.error_rate")}
              value={`${metrics.errorRate.toFixed(0)}%`}
              tone={metrics.errorRate > 10 ? "red" : "green"}
            />
          </section>

          <div className="product-panel">
            <p className="product-prose">{t("perf.explainer")}</p>
          </div>
        </>
      )}
    </div>
  );
}

function Metric({ label, value, tone }: { label: string; value: string; tone?: "green" | "red" }) {
  return (
    <div className="product-metric">
      <div
        className="product-metric-value"
        style={tone ? { color: tone === "red" ? "var(--red)" : "var(--green)" } : undefined}
      >
        {value}
      </div>
      <div className="product-metric-label">{label}</div>
    </div>
  );
}
