import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
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
  type ProductRow,
} from "../utils/productSurface";

function projectStatusLabel(status: string) {
  if (status === "waiting") return t("projects.status_waiting");
  if (status === "done") return t("projects.status_done");
  return t("projects.status_active");
}

export default function Projects() {
  useLanguage();
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

  return (
    <div className="product-page">
      <header className="product-header">
        <div>
          <div className="product-kicker">{t("nav.projects")}</div>
          <h1 className="product-title">{t("projects.title")}</h1>
          <p className="product-subtitle">{t("projects.subtitle")}</p>
        </div>
        <div className="product-actions">
          <Link to="/" className="primary-button product-action">{t("projects.new_project_chat")}</Link>
          <Link to="/company" className="product-link-button">{t("nav.my_company")}</Link>
        </div>
      </header>

      {loading && <div className="product-empty">{t("settings.loading")}</div>}

      <section className="project-grid">
        {projects.map((project) => (
          <Link key={project.id} to={`/projects/${encodeURIComponent(project.id)}`} className="project-card">
            <div className="project-card-head">
              <h2>{project.name}</h2>
              <span className={`product-pill ${project.status === "waiting" ? "yellow" : project.status === "done" ? "green" : "blue"}`}>
                {projectStatusLabel(project.status)}
              </span>
            </div>
            <p>{project.description || t("projects.default_desc")}</p>
            <div className="project-card-meta">
              <span>{project.tasks.length} {t("company.tasks_unit")}</span>
              <span>{project.conversations.length} {t("company.chats_unit")}</span>
              <span>{project.memories.length} {t("projects.memories_unit")}</span>
            </div>
            <div className="project-card-footer">
              <span>{project.folder || t("projects.no_folder")}</span>
              <span>{formatRelativeTime(project.updatedAtMs)}</span>
            </div>
          </Link>
        ))}
      </section>

      {!loading && projects.length === 0 && (
        <section className="product-panel">
          <h2 className="product-section-title">{t("projects.empty_title")}</h2>
          <p className="product-prose">{t("projects.empty_desc")}</p>
        </section>
      )}

      <section className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("projects.recent_project_signals")}</h2>
          <Link to="/timeline">{t("nav.timeline")}</Link>
        </div>
        <div className="product-list">
          {tasks.slice(0, 6).map((task, index) => (
            <Link key={String(task.work_order_id || index)} to={`/tasks/${encodeURIComponent(String(task.work_order_id || ""))}`} className="product-list-row">
              <span className="product-row-main">{shortText(task.mission_intent || t("tasks.untitled"))}</span>
              <span className="product-row-meta">{String(task.status || "")}</span>
            </Link>
          ))}
          {!tasks.length && <div className="product-empty">{t("company.no_tasks")}</div>}
        </div>
      </section>
    </div>
  );
}
