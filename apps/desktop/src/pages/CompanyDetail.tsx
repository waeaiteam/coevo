import type { ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import {
  getCompanyProfile,
  getUserProfile,
  listEmployees,
  listMemory,
  listWorkOrders,
} from "../api/client";
import { getLocalIdentity } from "../settings/identity";
import { t, useLanguage } from "../settings/i18n";
import {
  deriveProjects,
  listField,
  shortText,
  stringField,
  type ProductRow,
} from "../utils/productSurface";

function DetailBlock({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="product-panel">
      <h2 className="product-section-title">{title}</h2>
      {children}
    </section>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div className="product-field">
      <span>{label}</span>
      <strong>{value || "-"}</strong>
    </div>
  );
}

export default function CompanyDetail() {
  useLanguage();
  const identity = getLocalIdentity();
  const [company, setCompany] = useState<ProductRow | null>(null);
  const [user, setUser] = useState<ProductRow | null>(null);
  const [employees, setEmployees] = useState<ProductRow[]>([]);
  const [memories, setMemories] = useState<ProductRow[]>([]);
  const [tasks, setTasks] = useState<ProductRow[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void Promise.all([
      getCompanyProfile().catch(() => null),
      getUserProfile().catch(() => null),
      listEmployees().catch(() => []),
      listMemory({ scope: "company" }).catch(() => []),
      listWorkOrders().catch(() => []),
    ]).then(([nextCompany, nextUser, nextEmployees, nextMemories, nextTasks]) => {
      if (!alive) return;
      setCompany(nextCompany as ProductRow | null);
      setUser(nextUser as ProductRow | null);
      setEmployees(Array.isArray(nextEmployees) ? nextEmployees as ProductRow[] : []);
      setMemories(Array.isArray(nextMemories) ? nextMemories as ProductRow[] : []);
      setTasks(Array.isArray(nextTasks) ? nextTasks as ProductRow[] : []);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, []);

  const projects = useMemo(
    () => deriveProjects({ companyProfile: company, userProfile: user, workOrders: tasks, memories }),
    [company, memories, tasks, user],
  );
  const name = stringField(company || undefined, "name") || identity.opcName;
  const principles = listField(company || undefined, "operating_principles");
  const departments = new Set(employees.map((employee) => stringField(employee, "department")).filter(Boolean));
  const waiting = tasks.filter((task) => stringField(task, "status") === "WaitingApproval").length;

  return (
    <div className="product-page">
      <header className="product-header">
        <div>
          <div className="product-kicker">{t("company.details")}</div>
          <h1 className="product-title">{name}</h1>
          <p className="product-subtitle">{stringField(company || undefined, "mission") || t("company.mission_empty")}</p>
        </div>
        <div className="product-actions">
          <Link to="/company" className="product-link-button">{t("company.back_overview")}</Link>
          <Link to="/" className="primary-button product-action">{t("nav.new_chat")}</Link>
        </div>
      </header>

      {loading && <div className="product-empty">{t("settings.loading")}</div>}

      <section className="product-grid-2">
        <DetailBlock title={t("company.profile")}>
          <div className="product-field-grid">
            <Field label={t("company.name")} value={name} />
            <Field label={t("company.owner")} value={identity.userName} />
            <Field label={t("company.domain")} value={stringField(company || undefined, "current_strategy")} />
            <Field label={t("company.policy")} value={stringField(company || undefined, "policy_profile")} />
          </div>
          <p className="product-prose mt-3">{stringField(company || undefined, "mission") || t("company.mission_empty")}</p>
        </DetailBlock>

        <DetailBlock title={t("company.safety_title")}>
          <div className="product-safety-grid compact">
            <div>
              <strong>{t("company.low_risk")}</strong>
              <p>{t("company.low_risk_desc")}</p>
            </div>
            <div>
              <strong>{t("company.confirm_risk")}</strong>
              <p>{waiting} {t("company.waiting_count_suffix")}</p>
            </div>
            <div>
              <strong>{t("company.blocked_risk")}</strong>
              <p>{t("company.blocked_risk_desc")}</p>
            </div>
          </div>
        </DetailBlock>
      </section>

      <section className="product-grid-3">
        <DetailBlock title={t("company.operating_principles")}>
          <div className="product-card-list">
            {principles.map((principle, index) => (
              <div key={`${principle}-${index}`} className="product-card-row">
                <strong>{String(index + 1).padStart(2, "0")}</strong>
                <span>{principle}</span>
              </div>
            ))}
            {!principles.length && <div className="product-empty">{t("company.no_principles")}</div>}
          </div>
        </DetailBlock>

        <DetailBlock title={t("company.ai_team")}>
          <div className="product-card-list">
            {employees.slice(0, 8).map((employee, index) => (
              <Link key={stringField(employee, "agent_id") || index} to="/employees" className="product-card-row">
                <strong>{stringField(employee, "display_name") || stringField(employee, "agent_id")}</strong>
                <span>{stringField(employee, "department") || t("employees.department_custom")}</span>
              </Link>
            ))}
            {!employees.length && <div className="product-empty">{t("employees.empty")}</div>}
          </div>
        </DetailBlock>

        <DetailBlock title={t("nav.projects")}>
          <div className="product-card-list">
            {projects.slice(0, 8).map((project) => (
              <Link key={project.id} to={`/projects/${encodeURIComponent(project.id)}`} className="product-card-row">
                <strong>{project.name}</strong>
                <span>{project.tasks.length} {t("company.tasks_unit")}</span>
              </Link>
            ))}
          </div>
        </DetailBlock>
      </section>

      <section className="product-grid-2">
        <DetailBlock title={t("company.memory")}>
          <div className="product-list">
            {memories.slice(0, 8).map((memory, index) => (
              <div key={stringField(memory, "memory_id") || index} className="product-list-row static">
                <span className="product-row-main">{shortText(stringField(memory, "title") || t("memory.title"))}</span>
                <span className="product-row-meta">{shortText(stringField(memory, "content"), 60)}</span>
              </div>
            ))}
            {!memories.length && <div className="product-empty">{t("company.no_memory")}</div>}
          </div>
        </DetailBlock>

        <DetailBlock title={t("company.local_data")}>
          <div className="product-field-grid">
            <Field label={t("company.opc_id")} value={identity.opcId} />
            <Field label={t("company.user_id")} value={identity.userId} />
            <Field label={t("company.departments")} value={String(departments.size)} />
            <Field label={t("company.tasks")} value={String(tasks.length)} />
          </div>
          <details className="product-inline-details">
            <summary>{t("nav.advanced")}</summary>
            <div className="product-field-grid mt-3">
              <Field label="tenant_id" value={identity.tenantId} />
              <Field label="active_projects" value={projects.map((project) => project.name).join(", ")} />
              <Field label="memory_policy" value={JSON.stringify(company?.memory_policy || {})} />
              <Field label="budget_limits" value={JSON.stringify(user?.budget_limits || {})} />
            </div>
          </details>
        </DetailBlock>
      </section>
    </div>
  );
}
