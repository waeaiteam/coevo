import { useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import {
  listReports,
  getReport,
  generateReport,
  type ReportSummary,
  type ReportDetail,
  type ReportAlert,
} from "../api/org";
import { getActiveOpcId } from "../api/companies";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";

function titleCase(raw: string): string {
  return raw
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

function periodLabel(period: string): string {
  return period === "monthly" ? t("report.period_monthly") : t("report.period_daily");
}

function scoreTone(score: number): "green" | "yellow" | "red" {
  if (score >= 85) return "green";
  if (score >= 70) return "yellow";
  return "red";
}

function alertIconName(severity: ReportAlert["severity"]): Parameters<typeof Icon>[0]["name"] {
  return severity === "info" ? "info" : "alert";
}

export default function OperatingReports() {
  useLanguage();
  const params = useParams();
  const opcId = params.opcId ? decodeURIComponent(params.opcId) : getActiveOpcId();

  const [reports, setReports] = useState<ReportSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string>("");
  const [detail, setDetail] = useState<ReportDetail | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [detailLoading, setDetailLoading] = useState<boolean>(false);
  const [generating, setGenerating] = useState<boolean>(false);

  function newest(rows: ReportSummary[]): string {
    return rows.reduce<ReportSummary | null>((best, row) => {
      if (!best || row.created_at_ms > best.created_at_ms) return row;
      return best;
    }, null)?.report_id ?? "";
  }

  // Load the briefings list on mount and auto-select the newest one.
  useEffect(() => {
    let alive = true;
    setLoading(true);
    listReports(opcId)
      .then((rows) => {
        if (!alive) return;
        setReports(rows);
        setSelectedId((prev) => prev || newest(rows));
      })
      .catch(() => {
        if (alive) setReports([]);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [opcId]);

  // Load the selected briefing's body whenever the selection changes.
  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let alive = true;
    setDetailLoading(true);
    getReport(opcId, selectedId)
      .then((d) => {
        if (alive) setDetail(d);
      })
      .catch(() => {
        if (alive) setDetail(null);
      })
      .finally(() => {
        if (alive) setDetailLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [opcId, selectedId]);

  async function generate(period: "daily" | "monthly") {
    if (generating) return;
    setGenerating(true);
    try {
      const { report_id } = await generateReport(opcId, period);
      const rows = await listReports(opcId);
      setReports(rows);
      setSelectedId(report_id);
    } catch {
      /* leave the current view in place if generation fails */
    } finally {
      setGenerating(false);
    }
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("report.kicker")}</div>
          <h1 className="product-title">{t("report.title")}</h1>
          <p className="product-subtitle">{t("report.subtitle")}</p>
        </div>
        <div className="product-actions">
          <Link to={`/companies/${encodeURIComponent(opcId)}`} className="product-link-button">
            <Icon name="chevron-right" style={{ transform: "rotate(180deg)" }} /> {t("companies.title")}
          </Link>
          <button
            className="product-link-button"
            disabled={generating}
            onClick={() => generate("daily")}
          >
            {generating ? <Icon name="spinner" className="icon-spin" /> : <Icon name="calendar" />}{" "}
            {generating ? t("report.generating") : t("report.generate_daily")}
          </button>
          <button
            className="product-link-button"
            disabled={generating}
            onClick={() => generate("monthly")}
          >
            {generating ? <Icon name="spinner" className="icon-spin" /> : <Icon name="file-text" />}{" "}
            {generating ? t("report.generating") : t("report.generate_monthly")}
          </button>
        </div>
      </header>

      <div className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("report.list")}</h2>
            <span>{reports.length}</span>
          </div>
          {loading ? (
            <div className="product-empty">{t("settings.loading")}</div>
          ) : reports.length === 0 ? (
            <div className="product-empty">{t("report.empty")}</div>
          ) : (
            <div className="product-list">
              {reports.map((report) => {
                const active = report.report_id === selectedId;
                return (
                  <button
                    key={report.report_id}
                    className="product-list-row"
                    onClick={() => setSelectedId(report.report_id)}
                    style={active ? { borderColor: "var(--accent)" } : undefined}
                  >
                    <span className="product-row-main">{periodLabel(report.period)}</span>
                    <span className="product-row-meta">
                      <Icon name="clock" /> {new Date(report.created_at_ms).toLocaleString()}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div className="product-panel">
          {!selectedId ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="file-text" /></div>
              <p>{t("report.select")}</p>
            </div>
          ) : detailLoading ? (
            <div className="product-empty">{t("settings.loading")}</div>
          ) : !detail ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="file-text" /></div>
              <p>{t("report.select")}</p>
            </div>
          ) : (
            <>
              <div className="resolution-md">{detail.report_md}</div>

              <div className="product-panel-heading">
                <h2><Icon name="gauge" /> {t("report.dept_scores")}</h2>
                <span>{detail.kpi_summary.length}</span>
              </div>
              <div className="product-card-list">
                {detail.kpi_summary.map((row) => (
                  <div key={row.department} className="product-card-row">
                    <strong>{titleCase(row.department)}</strong>
                    <span>
                      {row.score}{" "}
                      <span className={`product-pill ${scoreTone(row.score)}`}>{row.score}</span>
                    </span>
                  </div>
                ))}
              </div>

              <div className="product-panel-heading">
                <h2><Icon name="database" /> {t("report.usage")}</h2>
                <span>{detail.token_usage.length}</span>
              </div>
              <div className="product-list">
                {detail.token_usage.map((row) => (
                  <div key={row.department} className="product-list-row static">
                    <span className="product-row-main">{titleCase(row.department)}</span>
                    <span className="product-row-meta">
                      {row.tokens.toLocaleString()} {t("report.tokens")} · ${row.cost_usd.toFixed(2)}
                    </span>
                  </div>
                ))}
              </div>

              <div className="product-panel-heading">
                <h2><Icon name="alert" /> {t("report.alerts")}</h2>
                <span>{detail.alerts.length}</span>
              </div>
              {detail.alerts.length === 0 ? (
                <div className="product-empty">{t("report.no_alerts")}</div>
              ) : (
                <div className="product-list">
                  {detail.alerts.map((alert, index) => (
                    <div key={`${alert.severity}-${index}`} className={`alert-row ${alert.severity}`}>
                      <span className="alert-icon"><Icon name={alertIconName(alert.severity)} /></span>
                      <span>{alert.message}</span>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
