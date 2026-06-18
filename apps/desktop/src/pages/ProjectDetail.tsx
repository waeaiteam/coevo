import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  getCompanyProfile,
  getUserProfile,
  listConversations,
  listMemory,
  listWorkOrders,
} from "../api/client";
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

export default function ProjectDetail() {
  useLanguage();
  const params = useParams();
  const [company, setCompany] = useState<ProductRow | null>(null);
  const [user, setUser] = useState<ProductRow | null>(null);
  const [conversations, setConversations] = useState<ProductRow[]>([]);
  const [tasks, setTasks] = useState<ProductRow[]>([]);
  const [memories, setMemories] = useState<ProductRow[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void Promise.all([
      getCompanyProfile().catch(() => null),
      getUserProfile().catch(() => null),
      listConversations().catch(() => []),
      listWorkOrders().catch(() => []),
      listMemory({ scope: "company" }).catch(() => []),
    ]).then(([nextCompany, nextUser, nextConversations, nextTasks, nextMemories]) => {
      if (!alive) return;
      setCompany(nextCompany as ProductRow | null);
      setUser(nextUser as ProductRow | null);
      setConversations(Array.isArray(nextConversations) ? nextConversations as ProductRow[] : []);
      setTasks(Array.isArray(nextTasks) ? nextTasks as ProductRow[] : []);
      setMemories(Array.isArray(nextMemories) ? nextMemories as ProductRow[] : []);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, []);

  const projects = useMemo(
    () => deriveProjects({ companyProfile: company, userProfile: user, conversations, workOrders: tasks, memories }),
    [company, conversations, memories, tasks, user],
  );
  const requestedId = params.projectId ? decodeURIComponent(params.projectId) : "";
  // Only fall back to the first project when no specific id was requested; otherwise an
  // unknown id should surface "not found" rather than silently showing another project.
  const project = requestedId
    ? projects.find((item) => item.id === requestedId)
    : projects[0];
  const waiting = project?.tasks.filter((task) => stringField(task, "status") === "WaitingApproval").length || 0;
  const completed = project?.tasks.filter((task) => stringField(task, "status") === "Completed").length || 0;

  if (!project && !loading) {
    return (
      <div className="product-page">
        <section className="product-panel">
          <h1 className="product-title">{t("projects.not_found")}</h1>
          <Link to="/projects" className="product-link-button mt-3">{t("projects.back")}</Link>
        </section>
      </div>
    );
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div>
          <div className="product-kicker">{t("projects.detail")}</div>
          <h1 className="product-title">{project?.name || t("projects.title")}</h1>
          <p className="product-subtitle">{project?.description || t("projects.default_desc")}</p>
        </div>
        <div className="product-actions">
          <Link to="/" className="primary-button product-action">{t("projects.new_project_chat")}</Link>
          <Link to="/projects" className="product-link-button">{t("projects.back")}</Link>
        </div>
      </header>

      {loading && <div className="product-empty">{t("settings.loading")}</div>}

      {project && (
        <>
          <section className="product-metrics-grid" aria-label={t("projects.project_health")}>
            <div className="product-metric">
              <div className="product-metric-value">{project.tasks.length}</div>
              <div className="product-metric-label">{t("company.metric_tasks")}</div>
            </div>
            <div className="product-metric">
              <div className="product-metric-value">{completed}</div>
              <div className="product-metric-label">{t("workorders.metric_completed")}</div>
            </div>
            <div className="product-metric">
              <div className="product-metric-value">{waiting}</div>
              <div className="product-metric-label">{t("workorders.metric_waiting_approval")}</div>
            </div>
            <div className="product-metric">
              <div className="product-metric-value">{project.conversations.length}</div>
              <div className="product-metric-label">{t("company.metric_chats")}</div>
            </div>
          </section>

          <section className="product-grid-2">
            <div className="product-panel">
              <div className="product-panel-heading">
                <h2>{t("projects.conversations")}</h2>
                <Link to="/">{t("nav.new_chat")}</Link>
              </div>
              <div className="product-list">
                {project.conversations.map((conversation, index) => {
                  const id = stringField(conversation, "conversation_id") || `conversation-${index}`;
                  return (
                    <Link key={id} to={`/conversations/${encodeURIComponent(id)}`} className="product-list-row">
                      <span className="product-row-main">{shortText(stringField(conversation, "title") || t("chat.untitled"))}</span>
                      <span className="product-row-meta">{formatRelativeTime(Number(conversation.updated_at_ms || 0))}</span>
                    </Link>
                  );
                })}
                {!project.conversations.length && <div className="product-empty">{t("projects.no_conversations")}</div>}
              </div>
            </div>

            <div className="product-panel">
              <div className="product-panel-heading">
                <h2>{t("projects.tasks")}</h2>
                <Link to="/work-orders">{t("company.view_all")}</Link>
              </div>
              <div className="product-list">
                {project.tasks.map((task, index) => {
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
                {!project.tasks.length && <div className="product-empty">{t("projects.no_tasks")}</div>}
              </div>
            </div>
          </section>

          <section className="product-grid-2">
            <div className="product-panel">
              <h2 className="product-section-title">{t("projects.outputs")}</h2>
              <div className="product-card-list">
                {project.tasks.filter((task) => stringField(task, "status") === "Completed").slice(0, 6).map((task, index) => (
                  <Link key={stringField(task, "work_order_id") || index} to={`/tasks/${encodeURIComponent(stringField(task, "work_order_id"))}`} className="product-card-row">
                    <strong>{shortText(stringField(task, "mission_intent") || t("tasks.untitled"))}</strong>
                    <span>{t("workorders.status_completed")}</span>
                  </Link>
                ))}
                {completed === 0 && <div className="product-empty">{t("projects.no_outputs")}</div>}
              </div>
            </div>

            <div className="product-panel">
              <h2 className="product-section-title">{t("projects.memory")}</h2>
              <div className="product-card-list">
                {project.memories.map((memory, index) => (
                  <div key={stringField(memory, "memory_id") || index} className="product-card-row">
                    <strong>{shortText(stringField(memory, "title") || t("memory.title"))}</strong>
                    <span>{shortText(stringField(memory, "content"), 70)}</span>
                  </div>
                ))}
                {!project.memories.length && <div className="product-empty">{t("projects.no_memory")}</div>}
              </div>
            </div>
          </section>

          <section className="product-panel">
            <div className="product-panel-heading">
              <h2>{t("projects.timeline")}</h2>
              <Link to="/timeline">{t("nav.timeline")}</Link>
            </div>
            <div className="product-inline-timeline">
              {project.tasks.slice(0, 5).map((task, index) => (
                <div key={stringField(task, "work_order_id") || index} className="product-timeline-row">
                  <span>{formatRelativeTime(Number(task.updated_at_ms || task.created_at_ms || 0))}</span>
                  <strong>{shortText(stringField(task, "mission_intent") || t("tasks.untitled"))}</strong>
                  <em>{statusLabel(stringField(task, "status"))}</em>
                </div>
              ))}
              {!project.tasks.length && <div className="product-empty">{t("projects.no_timeline")}</div>}
            </div>
          </section>
        </>
      )}
    </div>
  );
}
