import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import {
  getCompanyProfile,
  listConversations,
  listEmployees,
  listMemory,
  listWorkOrders,
} from "../api/client";
import AdvancedConsole from "../components/AdvancedConsole";
import { getLocalIdentity } from "../settings/identity";
import { t, useLanguage } from "../settings/i18n";
import {
  deriveProjects,
  formatRelativeTime,
  shortText,
  stringField,
  taskStatusTone,
  type ProductRow,
} from "../utils/productSurface";

function statusLabel(status: string): string {
  if (status === "Completed") return t("workorders.status_completed");
  if (status === "Failed") return t("workorders.status_failed");
  if (status === "WaitingApproval") return t("workorders.status_waiting");
  if (status === "Running") return t("workorders.status_running");
  return t("workorders.status_ready");
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="product-metric">
      <div className="product-metric-value">{value}</div>
      <div className="product-metric-label">{label}</div>
    </div>
  );
}

function EmptyLine({ children }: { children: string }) {
  return <div className="product-empty">{children}</div>;
}

export default function MyCompany() {
  useLanguage();
  const identity = getLocalIdentity();
  const [company, setCompany] = useState<ProductRow | null>(null);
  const [employees, setEmployees] = useState<ProductRow[]>([]);
  const [memories, setMemories] = useState<ProductRow[]>([]);
  const [tasks, setTasks] = useState<ProductRow[]>([]);
  const [conversations, setConversations] = useState<ProductRow[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void Promise.all([
      getCompanyProfile().catch(() => null),
      listEmployees().catch(() => []),
      listMemory({ scope: "company" }).catch(() => []),
      listWorkOrders().catch(() => []),
      listConversations().catch(() => []),
    ]).then(([profile, nextEmployees, nextMemories, nextTasks, nextConversations]) => {
      if (!alive) return;
      setCompany(profile as ProductRow | null);
      setEmployees(Array.isArray(nextEmployees) ? nextEmployees as ProductRow[] : []);
      setMemories(Array.isArray(nextMemories) ? nextMemories as ProductRow[] : []);
      setTasks(Array.isArray(nextTasks) ? nextTasks as ProductRow[] : []);
      setConversations(Array.isArray(nextConversations) ? nextConversations as ProductRow[] : []);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, []);

  const projects = useMemo(
    () => deriveProjects({ companyProfile: company, conversations, workOrders: tasks, memories }),
    [company, conversations, memories, tasks],
  );
  const activeEmployees = employees.filter((employee) => String(employee.lifecycle_status || "").toLowerCase() === "active");
  const waitingTasks = tasks.filter((task) => stringField(task, "status") === "WaitingApproval");
  const runningTasks = tasks.filter((task) => ["Planned", "Running"].includes(stringField(task, "status")));
  const companyName = stringField(company || undefined, "name") || identity.opcName;
  const mission = stringField(company || undefined, "mission") || t("company.mission_empty");
  const strategy = stringField(company || undefined, "current_strategy") || t("company.strategy_empty");

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("nav.my_company")}</div>
          <h1 className="product-title">{companyName}</h1>
          <p className="product-subtitle">{mission}</p>
        </div>
        <div className="product-actions">
          <Link to="/" className="primary-button product-action">{t("nav.new_chat")}</Link>
          <Link to="/company/details" className="product-link-button">{t("company.open_details")}</Link>
        </div>
      </header>

      <section className="product-metrics-grid" aria-label={t("company.health")}>
        <Metric label={t("company.metric_employees")} value={String(activeEmployees.length)} />
        <Metric label={t("company.metric_projects")} value={String(projects.length)} />
        <Metric label={t("company.metric_tasks")} value={String(tasks.length)} />
        <Metric label={t("company.metric_confirmations")} value={String(waitingTasks.length)} />
      </section>

      <section className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("company.current_work")}</h2>
            <Link to="/work-orders">{t("company.view_all")}</Link>
          </div>
          <div className="product-list">
            {tasks.slice(0, 5).map((task, index) => {
              const id = stringField(task, "work_order_id") || `task-${index}`;
              const status = stringField(task, "status");
              const track = stringField(task, "track");
              return (
                <Link key={id} to={`/tasks/${encodeURIComponent(id)}`} className="product-list-row">
                  <span className="product-row-main">{shortText(stringField(task, "mission_intent") || t("tasks.untitled"))}</span>
                  <span className={`product-pill ${taskStatusTone(status, track)}`}>{statusLabel(status)}</span>
                </Link>
              );
            })}
            {!tasks.length && <EmptyLine>{loading ? t("settings.loading") : t("company.no_tasks")}</EmptyLine>}
          </div>
        </div>

        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("company.recent_chats")}</h2>
            <Link to="/">{t("nav.new_chat")}</Link>
          </div>
          <div className="product-list">
            {conversations.slice(0, 5).map((conversation, index) => {
              const id = stringField(conversation, "conversation_id") || `conversation-${index}`;
              return (
                <Link key={id} to={`/conversations/${encodeURIComponent(id)}`} className="product-list-row">
                  <span className="product-row-main">{shortText(stringField(conversation, "title") || t("chat.untitled"))}</span>
                  <span className="product-row-meta">{formatRelativeTime(Number(conversation.updated_at_ms || 0))}</span>
                </Link>
              );
            })}
            {!conversations.length && <EmptyLine>{loading ? t("settings.loading") : t("company.no_chats")}</EmptyLine>}
          </div>
        </div>
      </section>

      <section className="product-grid-3">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("nav.projects")}</h2>
            <Link to="/projects">{t("company.view_all")}</Link>
          </div>
          <div className="product-card-list">
            {projects.slice(0, 4).map((project) => (
              <Link key={project.id} to={`/projects/${encodeURIComponent(project.id)}`} className="product-card-row">
                <strong>{project.name}</strong>
                <span>{project.tasks.length} {t("company.tasks_unit")} · {project.conversations.length} {t("company.chats_unit")}</span>
              </Link>
            ))}
          </div>
        </div>

        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("company.ai_team")}</h2>
            <Link to="/employees">{t("company.manage")}</Link>
          </div>
          <div className="product-card-list">
            {activeEmployees.slice(0, 5).map((employee, index) => (
              <div key={stringField(employee, "agent_id") || index} className="product-card-row">
                <strong>{stringField(employee, "display_name") || stringField(employee, "agent_id")}</strong>
                <span>{stringField(employee, "department") || t("employees.department_custom")}</span>
              </div>
            ))}
            {!activeEmployees.length && <EmptyLine>{t("employees.empty")}</EmptyLine>}
          </div>
        </div>

        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("company.memory")}</h2>
            <Link to="/memory">{t("company.open")}</Link>
          </div>
          <div className="product-card-list">
            {memories.slice(0, 5).map((memory, index) => (
              <div key={stringField(memory, "memory_id") || index} className="product-card-row">
                <strong>{shortText(stringField(memory, "title") || t("memory.title"))}</strong>
                <span>{shortText(stringField(memory, "content"), 54)}</span>
              </div>
            ))}
            {!memories.length && <EmptyLine>{t("company.no_memory")}</EmptyLine>}
          </div>
        </div>
      </section>

      <section className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("company.safety_title")}</h2>
          <span>{waitingTasks.length > 0 ? t("company.confirmation_needed") : t("company.safety_normal")}</span>
        </div>
        <div className="product-safety-grid">
          <div>
            <strong>{t("company.low_risk")}</strong>
            <p>{t("company.low_risk_desc")}</p>
          </div>
          <div>
            <strong>{t("company.confirm_risk")}</strong>
            <p>{t("company.confirm_risk_desc")}</p>
          </div>
          <div>
            <strong>{t("company.blocked_risk")}</strong>
            <p>{t("company.blocked_risk_desc")}</p>
          </div>
        </div>
      </section>

      <section className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("company.strategy")}</h2>
          <Link to="/company/details">{t("company.open_details")}</Link>
        </div>
        <p className="product-prose">{strategy}</p>
        <div className="mt-3 text-xs muted">
          {runningTasks.length} {t("company.active_tasks_hint")}
        </div>
      </section>

      <details className="product-advanced">
        <summary>{t("nav.advanced")}</summary>
        <div className="mt-3">
          <AdvancedConsole />
        </div>
      </details>
    </div>
  );
}
