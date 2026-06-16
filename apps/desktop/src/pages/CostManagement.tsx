import { useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";
import { getCost, setCostQuota, type CostOverview, type CostDepartment } from "../api/org";
import { getActiveOpcId } from "../api/companies";

type Status = { tone: "green" | "yellow" | "red" | "blue"; label: string };

function titleCase(raw: string): string {
  return raw
    .split(/[\s_-]+/)
    .filter((w) => w.length > 0)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

function statusFor(tokens: number, quota: number): Status {
  if (!(quota > 0)) {
    return { tone: "blue", label: t("cost.no_quota") };
  }
  const ratio = tokens / quota;
  if (ratio >= 1) return { tone: "red", label: t("cost.over_quota") };
  if (ratio >= 0.8) return { tone: "yellow", label: t("cost.near_quota") };
  return { tone: "green", label: t("cost.within_quota") };
}

function fillClass(tokens: number, quota: number): string {
  if (!(quota > 0)) return "quota-fill";
  const ratio = tokens / quota;
  if (ratio >= 1) return "quota-fill critical";
  if (ratio >= 0.8) return "quota-fill warning";
  return "quota-fill";
}

function fillWidth(tokens: number, quota: number): number {
  if (!(quota > 0)) return 0;
  return Math.min(100, (tokens / quota) * 100);
}

export default function CostManagement() {
  useLanguage();
  const params = useParams();
  const opcId = params.opcId ? decodeURIComponent(params.opcId) : getActiveOpcId();

  const [overview, setOverview] = useState<CostOverview | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [drafts, setDrafts] = useState<Record<string, number>>({});
  const [savingDept, setSavingDept] = useState<string>("");
  const [savedDept, setSavedDept] = useState<string>("");

  useEffect(() => {
    let alive = true;
    setLoading(true);
    getCost(opcId)
      .then((data) => {
        if (!alive) return;
        setOverview(data);
        const seed: Record<string, number> = {};
        for (const d of data.by_department) {
          seed[d.dept] = d.quota ?? 0;
        }
        setDrafts(seed);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [opcId]);

  async function handleSave(dept: string): Promise<void> {
    if (savingDept) return;
    const value = drafts[dept] ?? 0;
    setSavingDept(dept);
    try {
      const res = await setCostQuota(opcId, dept, value);
      if (res.ok) {
        setOverview((prev) =>
          prev
            ? {
                ...prev,
                by_department: prev.by_department.map((d) =>
                  d.dept === dept ? { ...d, quota: value } : d
                ),
              }
            : prev
        );
        setSavedDept(dept);
        window.setTimeout(() => {
          setSavedDept((cur) => (cur === dept ? "" : cur));
        }, 2000);
      }
    } finally {
      setSavingDept((cur) => (cur === dept ? "" : cur));
    }
  }

  const departments: CostDepartment[] = overview?.by_department ?? [];
  const total = overview?.total ?? 0;
  const totalTokens = departments.reduce((sum, d) => sum + d.tokens, 0);

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("cost.kicker")}</div>
          <h1 className="product-title">{t("cost.title")}</h1>
          <p className="product-subtitle">{t("cost.subtitle")}</p>
        </div>
        <div className="product-actions">
          <Link className="product-link-button" to={`/companies/${encodeURIComponent(opcId)}`}>
            <span style={{ display: "inline-flex", transform: "rotate(180deg)", marginRight: 6 }}>
              <Icon name="chevron-right" />
            </span>
            {t("companies.title")}
          </Link>
        </div>
      </header>

      {loading ? (
        <div className="product-empty">{t("settings.loading")}</div>
      ) : (
        <>
          <section className="product-metrics-grid">
            <div className="product-metric">
              <div className="product-metric-value">{`$${total.toFixed(2)}`}</div>
              <div className="product-metric-label">{t("cost.total")}</div>
            </div>
            <div className="product-metric">
              <div className="product-metric-value">{totalTokens.toLocaleString()}</div>
              <div className="product-metric-label">{t("report.tokens")}</div>
            </div>
            <div className="product-metric">
              <div className="product-metric-value">{String(departments.length)}</div>
              <div className="product-metric-label">{t("cost.department")}</div>
            </div>
          </section>

          {departments.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon">
                <Icon name="database" />
              </div>
              <p>{t("cost.subtitle")}</p>
            </div>
          ) : (
            <div className="product-panel">
              <div className="product-panel-heading">{t("cost.title")}</div>
              <div className="product-list">
                {departments.map((d) => {
                  const quota = d.quota ?? 0;
                  const status = statusFor(d.tokens, quota);
                  const draft = drafts[d.dept] ?? 0;
                  const isSaving = savingDept === d.dept;
                  const justSaved = savedDept === d.dept;
                  return (
                    <div
                      key={d.dept}
                      style={{ borderBottom: "1px solid var(--border-subtle)", padding: "12px 0" }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                        <strong>{titleCase(d.dept)}</strong>
                        <span className={`product-pill ${status.tone}`}>{status.label}</span>
                      </div>

                      <div
                        style={{
                          color: "var(--text-secondary)",
                          fontSize: 12,
                          marginTop: 4,
                          display: "flex",
                          gap: 8,
                        }}
                      >
                        <span>
                          {`${d.tokens.toLocaleString()} ${t("report.tokens")} · $${d.cost_usd.toFixed(2)}`}
                        </span>
                        <span>·</span>
                        <span>{t("cost.usage")}</span>
                      </div>

                      <div className="quota-bar">
                        <div
                          className={fillClass(d.tokens, quota)}
                          style={{ width: `${fillWidth(d.tokens, quota)}%` }}
                        />
                      </div>

                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 8,
                          marginTop: 10,
                        }}
                      >
                        <span style={{ color: "var(--text-secondary)", fontSize: 12 }}>
                          {t("cost.quota")}
                        </span>
                        <input
                          className="select-control"
                          type="number"
                          min={0}
                          value={draft}
                          onChange={(e) => {
                            const next = Number(e.target.value);
                            setDrafts((prev) => ({
                              ...prev,
                              [d.dept]: Number.isFinite(next) ? next : 0,
                            }));
                          }}
                        />
                        <button
                          className="primary-button"
                          type="button"
                          disabled={isSaving}
                          onClick={() => {
                            void handleSave(d.dept);
                          }}
                        >
                          <span
                            style={{ display: "inline-flex", alignItems: "center", gap: 6 }}
                          >
                            <Icon name={isSaving ? "spinner" : justSaved ? "check" : "sliders"} />
                            {justSaved ? t("cost.saved") : t("cost.save")}
                          </span>
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
