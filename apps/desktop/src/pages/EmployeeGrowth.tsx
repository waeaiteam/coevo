import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { getAgentGrowth, approveSkillProposal, type AgentGrowth } from "../api/client";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";

function directionLabel(dir: AgentGrowth["direction"]): { text: string; tone: string; icon: Parameters<typeof Icon>[0]["name"] } {
  switch (dir) {
    case "improving": return { text: t("growth.improving"), tone: "green", icon: "badge-check" };
    case "declining": return { text: t("growth.declining"), tone: "red", icon: "alert" };
    case "steady": return { text: t("growth.steady"), tone: "blue", icon: "check" };
    default: return { text: t("growth.new"), tone: "blue", icon: "sparkles" };
  }
}

export default function EmployeeGrowth({ embedded = false }: { embedded?: boolean }) {
  useLanguage();
  const params = useParams();
  const agentId = params.agentId ? decodeURIComponent(params.agentId) : "";
  const [growth, setGrowth] = useState<AgentGrowth | null>(null);
  const [loading, setLoading] = useState(true);
  const [approving, setApproving] = useState<string>("");

  async function load() {
    setLoading(true);
    try {
      const g = await getAgentGrowth(agentId);
      setGrowth(g);
    } catch {
      setGrowth(null);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (agentId) load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId]);

  async function approve(proposalId: string) {
    setApproving(proposalId);
    try {
      await approveSkillProposal(proposalId);
      await load();
    } finally {
      setApproving("");
    }
  }

  const dir = growth ? directionLabel(growth.direction) : null;

  return (
    <div className={embedded ? "space-y-4" : "product-page"}>
      {!embedded && (
        <header className="product-header">
          <div className="min-w-0">
            <div className="product-kicker">{t("growth.kicker")}</div>
            <h1 className="product-title">{agentId}</h1>
          </div>
          <div className="product-actions">
            <Link to="/employees" className="product-link-button">
              <Icon name="chevron-right" style={{ transform: "rotate(180deg)" }} /> {t("growth.back")}
            </Link>
          </div>
        </header>
      )}

      {loading && <div className="product-empty">{t("settings.loading")}</div>}

      {!loading && !growth && (
        <div className="empty-state">
          <div className="empty-state-icon"><Icon name="user" /></div>
          <p>{t("growth.no_data")}</p>
        </div>
      )}

      {!loading && growth && (
        <>
          <section className="feature-hero">
            <div className="feature-hero-icon"><Icon name={dir!.icon} /></div>
            <div className="min-w-0 flex-1">
              <h2>{t("growth.headline")}</h2>
              <p>
                {growth.total_tasks === 0
                  ? t("growth.headline_new")
                  : t(`growth.headline_${growth.direction}`)}
              </p>
              <div className="mt-3 flex items-center gap-3">
                <span className="metric-value" style={{ color: `var(--${dir!.tone === "red" ? "red" : dir!.tone === "green" ? "green" : "accent"})` }}>
                  {growth.current_score}
                </span>
                <span className={`product-pill ${dir!.tone}`}>{dir!.text}</span>
              </div>
            </div>
          </section>

          <section className="product-metrics-grid">
            <Metric label={t("growth.total_tasks")} value={String(growth.total_tasks)} />
            <Metric label={t("growth.success_rate")} value={`${growth.success_rate}%`} tone={growth.success_rate >= 70 ? "green" : "yellow"} />
            <Metric label={t("growth.avg_response")} value={growth.avg_latency_ms > 0 ? `${Math.round(growth.avg_latency_ms / 100) / 10}s` : "—"} />
            <Metric label={t("growth.usage")} value={growth.total_usage.toLocaleString()} />
          </section>

          <div className="product-panel">
            <div className="product-panel-heading">
              <h2>{t("growth.trend_title")}</h2>
              <span>{growth.trend.length}</span>
            </div>
            {growth.trend.length < 2 ? (
              <div className="empty-state">
                <div className="empty-state-icon"><Icon name="history" /></div>
                <p>{t("growth.trend_empty")}</p>
              </div>
            ) : (
              <TrendChart points={growth.trend} />
            )}
          </div>

          <div className="product-panel">
            <div className="product-panel-heading">
              <h2>{t("growth.improvements_title")}</h2>
              <span>{growth.pending_improvements.length}</span>
            </div>
            {growth.pending_improvements.length === 0 ? (
              <div className="empty-state">
                <div className="empty-state-icon"><Icon name="check" /></div>
                <p>{t("growth.no_improvements")}</p>
              </div>
            ) : (
              <div className="product-list">
                {growth.pending_improvements.map((p) => (
                  <div key={p.proposal_id} className="product-list-row static" style={{ flexWrap: "wrap", gap: 10, flexDirection: "column", alignItems: "stretch" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                      <span className="product-row-main" style={{ flex: "1 1 240px" }}>{p.diagnosis}</span>
                      <button
                        className="primary-button"
                        disabled={approving === p.proposal_id}
                        onClick={() => approve(p.proposal_id)}
                      >
                        {approving === p.proposal_id
                          ? <Icon name="spinner" className="icon-spin" />
                          : <Icon name="check" />} {t("growth.approve")}
                      </button>
                    </div>
                    {p.proposed_changes && (
                      <details style={{ marginTop: 6 }}>
                        <summary style={{ cursor: "pointer", fontSize: 13, color: "var(--text-secondary)", fontWeight: 600 }}>
                          {t("growth.proposed_changes")}
                        </summary>
                        <pre style={{ fontSize: 12, whiteSpace: "pre-wrap", background: "var(--surface-sunken)", borderRadius: 6, padding: "8px 10px", marginTop: 6, lineHeight: 1.5 }}>
                          {p.proposed_changes}
                        </pre>
                      </details>
                    )}
                    {!p.proposed_changes && (
                      <div style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 4 }}>
                        {t("growth.no_proposed_changes")}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function Metric({ label, value, tone }: { label: string; value: string; tone?: "green" | "yellow" | "red" }) {
  return (
    <div className="product-metric">
      <div className="product-metric-value" style={tone ? { color: `var(--${tone})` } : undefined}>{value}</div>
      <div className="product-metric-label">{label}</div>
    </div>
  );
}

function TrendChart({ points }: { points: Array<{ at: number; score: number; task_count: number }> }) {
  const w = 100;
  const h = 40;
  const max = 100;
  const min = 0;
  const step = points.length > 1 ? w / (points.length - 1) : w;
  const coords = points.map((p, i) => {
    const x = i * step;
    const y = h - ((p.score - min) / (max - min)) * h;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  const path = `M ${coords.join(" L ")}`;
  const last = points[points.length - 1];
  const first = points[0];
  const rising = last.score >= first.score;

  return (
    <div className="trend-chart">
      <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" className="trend-svg">
        <polyline
          points={coords.join(" ")}
          fill="none"
          stroke={rising ? "var(--green)" : "var(--red)"}
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
        />
        <path
          d={`${path} L ${w},${h} L 0,${h} Z`}
          fill={rising ? "var(--green-dim)" : "var(--red-dim)"}
          opacity="0.4"
        />
      </svg>
      <div className="trend-axis">
        <span>{first.score}</span>
        <span>{last.score}</span>
      </div>
    </div>
  );
}
