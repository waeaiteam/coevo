import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import Icon from "../components/Icon";
import { getCompanyDetail, companyWorkOrders, getActiveOpcId, type CompanyDetail } from "../api/companies";
import { listMemory } from "../api/client";
import { stringField, shortText, taskStatusTone, type ProductRow } from "../utils/productSurface";
import { t, useLanguage } from "../settings/i18n";

function statusLabel(status: string): string {
  if (status === "Completed") return t("workorders.status_completed");
  if (status === "Failed") return t("workorders.status_failed");
  if (status === "WaitingApproval") return t("workorders.status_waiting");
  if (status === "Running") return t("workorders.status_running");
  return t("workorders.status_ready");
}

function goalText(goal: ProductRow): string {
  return (
    stringField(goal, "title") ||
    stringField(goal, "name") ||
    stringField(goal, "objective") ||
    stringField(goal, "text") ||
    stringField(goal, "goal")
  );
}

function goalMeta(goal: ProductRow): string {
  return stringField(goal, "description") || stringField(goal, "detail") || stringField(goal, "summary");
}

export default function CompanyDetail() {
  useLanguage();
  const params = useParams();
  const opcId = params.opcId ? decodeURIComponent(params.opcId) : getActiveOpcId();

  const [detail, setDetail] = useState<CompanyDetail | null>(null);
  const [workOrders, setWorkOrders] = useState<ProductRow[]>([]);
  const [memories, setMemories] = useState<ProductRow[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void Promise.all([
      getCompanyDetail(opcId).catch(() => null),
      companyWorkOrders().catch(() => [] as unknown[]),
      listMemory({ scope: "company" }).catch(() => [] as unknown[]),
    ]).then(([nextDetail, nextWorkOrders, nextMemories]) => {
      if (!alive) return;
      setDetail(nextDetail as CompanyDetail | null);
      setWorkOrders(Array.isArray(nextWorkOrders) ? (nextWorkOrders as ProductRow[]) : []);
      setMemories(Array.isArray(nextMemories) ? (nextMemories as ProductRow[]) : []);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [opcId]);

  const officeRoute = `/companies/${encodeURIComponent(opcId)}/office`;
  const goals = detail?.goals ?? [];
  const departments = detail?.departments ?? [];
  const charter = detail?.charter_md?.trim() ?? "";
  const charterLines = charter ? charter.split(/\r?\n/) : [];
  const currentWork = workOrders.slice(0, 5);
  const recentMemory = memories.slice(0, 6);

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("companies.title")}</div>
          <h1 className="product-title">{detail?.name || opcId}</h1>
          <p className="product-subtitle">{detail?.mission || t("company.mission_empty")}</p>
        </div>
        <div className="product-actions">
          <Link to={officeRoute} className="primary-button product-action">
            <Icon name="users" /> {t("company.enter_office")}
          </Link>
          <Link to="/company" className="product-link-button">
            <Icon name="chevron-right" style={{ transform: "rotate(180deg)" }} /> {t("company.back_to_companies")}
          </Link>
        </div>
      </header>

      {loading && <div className="product-empty">{t("settings.loading")}</div>}

      {!loading && (
        <>
          <section className="feature-hero">
            <div className="feature-hero-icon"><Icon name="building" /></div>
            <div className="min-w-0 flex-1">
              <h2>{t("company.enter_office")}</h2>
              <p>{t("company.office_hint")}</p>
              <div className="mt-3">
                <Link to={officeRoute} className="product-link-button">
                  <Icon name="users" /> {t("company.enter_office")}
                </Link>
              </div>
            </div>
          </section>

          <section className="product-panel">
            <div className="product-panel-heading">
              <h2>{t("company.brief")}</h2>
            </div>
            <p className="product-prose" style={{ marginTop: -4 }}>{t("company.brief_hint")}</p>
            <div className="product-grid-2 mt-3">
              <Link to={`/companies/${encodeURIComponent(opcId)}/meetings`} className="product-card-row">
                <strong><Icon name="users" /> {t("org.meetings")}</strong>
                <span>{t("meet.subtitle")}</span>
              </Link>
              <Link to={`/companies/${encodeURIComponent(opcId)}/performance`} className="product-card-row">
                <strong><Icon name="gauge" /> {t("org.performance")}</strong>
                <span>{t("kpi.subtitle")}</span>
              </Link>
              <Link to={`/companies/${encodeURIComponent(opcId)}/reports`} className="product-card-row">
                <strong><Icon name="file-text" /> {t("org.reports")}</strong>
                <span>{t("report.subtitle")}</span>
              </Link>
              <Link to={`/companies/${encodeURIComponent(opcId)}/cost`} className="product-card-row">
                <strong><Icon name="database" /> {t("org.cost")}</strong>
                <span>{t("cost.subtitle")}</span>
              </Link>
            </div>
          </section>

          <section className="product-metrics-grid" aria-label={t("companies.title")}>
            <div className="product-metric">
              <div className="product-metric-value">{detail?.employee_count ?? 0}</div>
              <div className="product-metric-label">{t("company.metric_employees")}</div>
            </div>
            <div className="product-metric">
              <div className="product-metric-value">{departments.length}</div>
              <div className="product-metric-label">{t("company.departments")}</div>
            </div>
            <div className="product-metric">
              <div className="product-metric-value">{detail?.memory_count ?? 0}</div>
              <div className="product-metric-label">{t("company.memory")}</div>
            </div>
            <div className="product-metric">
              <div className="product-metric-value">{detail?.report_count ?? 0}</div>
              <div className="product-metric-label">{t("company.reports")}</div>
            </div>
          </section>

          <section className="product-grid-2">
            <div className="product-panel">
              <h2 className="product-section-title">{t("company.charter")}</h2>
              {charterLines.length ? (
                <div className="product-prose" style={{ whiteSpace: "pre-wrap" }}>
                  {charterLines.map((line, index) => (
                    <div key={`charter-${index}`}>{line || " "}</div>
                  ))}
                </div>
              ) : (
                <div className="product-empty">{t("company.charter_empty")}</div>
              )}
            </div>

            <div className="product-panel">
              <h2 className="product-section-title">{t("company.goals")}</h2>
              <div className="product-card-list">
                {goals.map((goal, index) => (
                  <div key={`goal-${index}`} className="product-card-row">
                    <strong>{shortText(goalText(goal) || t("company.goals"))}</strong>
                    {goalMeta(goal) ? <span>{shortText(goalMeta(goal), 70)}</span> : null}
                  </div>
                ))}
                {!goals.length && <div className="product-empty">{t("company.no_goals")}</div>}
              </div>
            </div>
          </section>

          <section className="product-grid-2">
            <div className="product-panel">
              <div className="product-panel-heading">
                <h2>{t("company.current_work")}</h2>
                <Link to="/work-orders">{t("company.view_all")}</Link>
              </div>
              <div className="product-list">
                {currentWork.map((order, index) => {
                  const status = stringField(order, "status");
                  const track = stringField(order, "track");
                  return (
                    <div key={`work-${index}`} className="product-list-row static">
                      <span className="product-row-main">
                        {shortText(stringField(order, "mission_intent") || t("company.current_work"))}
                      </span>
                      <span className={`product-pill ${taskStatusTone(status, track)}`}>{statusLabel(status)}</span>
                    </div>
                  );
                })}
                {!currentWork.length && <div className="product-empty">{t("employees.empty")}</div>}
              </div>
            </div>

            <div className="product-panel">
              <h2 className="product-section-title">{t("company.memory")}</h2>
              <div className="product-list">
                {recentMemory.map((memory, index) => (
                  <div key={`memory-${index}`} className="product-list-row static">
                    <span className="product-row-main">
                      {shortText(stringField(memory, "title") || stringField(memory, "content"))}
                    </span>
                    <span className="product-row-meta">{shortText(stringField(memory, "content"), 70)}</span>
                  </div>
                ))}
                {!recentMemory.length && <div className="product-empty">{t("company.no_memory")}</div>}
              </div>
            </div>
          </section>

          <section className="product-panel">
            <h2 className="product-section-title">{t("company.brief")}</h2>
            <p className="product-prose">{t("company.brief_hint")}</p>
            <div className="chip-row mt-3">
              <span className="mono-chip">
                {detail?.shared_files_count ?? 0} {t("company.shared_files_unit")}
              </span>
              {departments.length > 0 && <span className="mono-chip">{departments.join(" / ")}</span>}
              {detail?.dir ? <span className="mono-chip">{detail.dir}</span> : null}
            </div>
          </section>
        </>
      )}
    </div>
  );
}
