import { useMemo } from "react";
import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";
import { useTraceStore } from "../stores/traceStore";

function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length));
  return sorted[idx];
}

function fmtMs(ms: number): string {
  if (ms <= 0) return "—";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export default function Performance() {
  useLanguage();
  const { traces } = useTraceStore();

  const metrics = useMemo(() => {
    const list = Object.values(traces);
    const durations = list.filter((tr) => tr.duration_ms != null).map((tr) => tr.duration_ms as number);
    const totalTokens = list.reduce((sum, tr) => sum + tr.total_tokens, 0);
    const totalCost = list.reduce((sum, tr) => sum + tr.total_cost_usd, 0);
    const errorCount = list.filter((tr) => tr.status === "error").length;
    return {
      traceCount: list.length,
      typical: percentile(durations, 50),
      slower: percentile(durations, 90),
      slowest: percentile(durations, 99),
      totalTokens,
      totalCost,
      errorRate: list.length > 0 ? (errorCount / list.length) * 100 : 0,
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

      {metrics.traceCount === 0 ? (
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
      <div className="product-metric-value" style={tone ? { color: tone === "red" ? "var(--red)" : "var(--green)" } : undefined}>
        {value}
      </div>
      <div className="product-metric-label">{label}</div>
    </div>
  );
}
