import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { getActiveOpcId } from "../api/companies";
import { getCompanyWorkOrderTimeline, listCompanyWorkOrders } from "../api/org";
import GovernanceTimeline, { type TimelineSpan } from "../components/GovernanceTimeline";
import { SimpleMarkdown } from "../components/SimpleMarkdown";
import { t, useLanguage } from "../settings/i18n";
import { extractWorkOrderResult } from "../utils/workOrderResult";
import {
  extractProjectNameFromText,
  shortText,
  stringField,
  taskStatusTone,
  type ProductRow,
} from "../utils/productSurface";

function parseJson(value: unknown) {
  if (typeof value !== "string" || !value.trim()) return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function timelineRowsToSpans(rows: ProductRow[]): TimelineSpan[] {
  return rows.map((row, index) => {
    const details = row.details && typeof row.details === "object" ? row.details as ProductRow : {};
    const payload = parseJson(details.payload_json) as ProductRow;
    const output = parseJson(details.output_json);
    const type = stringField(row, "type") || "Activity";
    const status = stringField(details, "status") || stringField(payload, "status");
    return {
      id: stringField(details, "event_id") || stringField(details, "step_id") || `${type}-${index}`,
      type,
      label: stringField(row, "title") || stringField(row, "label") || type,
      subtitle: stringField(row, "message") || undefined,
      trust: type.includes("Executor") ? "external" : "native",
      gate: {
        outcome: type === "ApprovalRequired" ? "need_approval" : status === "Failed" ? "blocked" : "allow",
        reason: stringField(payload, "reason") || stringField(payload, "error"),
      },
      output: output || payload || details,
    };
  });
}

function taskStatusLabel(status: string): string {
  if (status === "Completed") return t("workorders.status_completed");
  if (status === "Failed") return t("workorders.status_failed");
  if (status === "WaitingApproval") return t("workorders.status_waiting");
  if (status === "Running") return t("workorders.status_running");
  return t("workorders.status_ready");
}

function nextAction(status: string, track: string): string {
  if (track === "red") return t("tasks.next_blocked");
  if (status === "WaitingApproval" || track === "yellow") return t("tasks.next_confirm");
  if (status === "Completed") return t("tasks.next_result");
  if (status === "Failed") return t("tasks.next_retry");
  return t("tasks.next_execute");
}

function taskExplain(status: string, track: string): string {
  if (track === "red") return t("tasks.blocked_explain");
  if (status === "Completed") return t("tasks.completed_explain");
  if (track === "yellow" || status === "WaitingApproval") return t("tasks.confirm_explain");
  if (status === "Failed") return t("tasks.failed_explain");
  return t("tasks.ready_explain");
}

export default function TaskDetail() {
  useLanguage();
  const params = useParams();
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const [tasks, setTasks] = useState<ProductRow[]>([]);
  const [timeline, setTimeline] = useState<ProductRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [timelineLoading, setTimelineLoading] = useState(false);
  const requestedId = params.workOrderId ? decodeURIComponent(params.workOrderId) : "";

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void listCompanyWorkOrders(activeOpcId)
      .then((rows) => {
        if (alive) setTasks(Array.isArray(rows) ? rows as ProductRow[] : []);
      })
      .catch(() => {
        if (alive) setTasks([]);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [activeOpcId]);

  const task = useMemo(
    () => tasks.find((row) => stringField(row, "work_order_id") === requestedId) || tasks[0],
    [requestedId, tasks],
  );
  const taskId = stringField(task, "work_order_id");
  const status = stringField(task, "status");
  const track = stringField(task, "track");
  const project = extractProjectNameFromText(stringField(task, "mission_intent")) || t("projects.general_workspace");
  const spans = useMemo(() => timelineRowsToSpans(timeline), [timeline]);
  const result = useMemo(() => extractWorkOrderResult(timeline), [timeline]);

  useEffect(() => {
    if (!taskId) return;
    let alive = true;
    setTimelineLoading(true);
    void getCompanyWorkOrderTimeline(activeOpcId, taskId)
      .then((rows) => {
        if (alive) setTimeline(Array.isArray(rows) ? rows as ProductRow[] : []);
      })
      .catch(() => {
        if (alive) setTimeline([]);
      })
      .finally(() => {
        if (alive) setTimelineLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [activeOpcId, taskId]);

  if (!task && !loading) {
    return (
      <div className="product-page">
        <section className="product-panel">
          <h1 className="product-title">{t("tasks.not_found")}</h1>
          <Link to="/work-orders" className="product-link-button mt-3">{t("tasks.back")}</Link>
        </section>
      </div>
    );
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("tasks.detail")}</div>
          <h1 className="product-title">{shortText(stringField(task, "mission_intent") || t("tasks.untitled"), 110)}</h1>
          <p className="product-subtitle">{t("tasks.from_project")}: {project}</p>
        </div>
        <div className="product-actions">
          <Link to="/work-orders" className="product-link-button">{t("tasks.back")}</Link>
          <Link to="/" className="primary-button product-action">{t("nav.new_chat")}</Link>
        </div>
      </header>

      {loading && <div className="product-empty">{t("settings.loading")}</div>}

      {task && (
        <>
          <section className="task-hero-panel">
            <div>
              <span className={`product-pill ${taskStatusTone(status, track)}`}>{taskStatusLabel(status)}</span>
              <h2>{nextAction(status, track)}</h2>
              <p>{taskExplain(status, track)}</p>
            </div>
            <div className="task-hero-actions">
              {track === "red" ? (
                <span className="product-link-button disabled">{t("tasks.cannot_execute")}</span>
              ) : status === "WaitingApproval" || track === "yellow" ? (
                <Link to="/work-orders" className="primary-button product-action">{t("tasks.open_confirmation")}</Link>
              ) : status === "Completed" ? (
                <a href="#task-result" className="primary-button product-action">{t("workorders.view_result")}</a>
              ) : (
                <Link to="/work-orders" className="primary-button product-action">{t("mission.start_task")}</Link>
              )}
            </div>
          </section>

          <section className="product-grid-3">
            <div className="product-panel">
              <h2 className="product-section-title">{t("tasks.owner")}</h2>
              <p className="product-prose">{stringField(task, "selected_agents") || t("mission.default_employee")}</p>
            </div>
            <div className="product-panel">
              <h2 className="product-section-title">{t("nav.projects")}</h2>
              <p className="product-prose">{project}</p>
            </div>
            <div className="product-panel">
              <h2 className="product-section-title">{t("tasks.safety")}</h2>
              <p className="product-prose">{track === "red" ? t("company.blocked_risk") : track === "yellow" ? t("company.confirm_risk") : t("company.low_risk")}</p>
            </div>
          </section>

          <section id="task-result" className="product-panel">
            <div className="product-panel-heading">
              <h2>{t("tasks.result")}</h2>
              <span>{result.eventCount || timeline.length} {t("workorders.events")}</span>
            </div>
            <p className="product-prose">
              {status === "Completed"
                ? t("tasks.result_ready")
                : status === "WaitingApproval"
                  ? t("tasks.result_waiting")
                  : status === "Failed"
                    ? t("tasks.result_failed")
                    : t("tasks.result_pending")}
            </p>
            {result.finalText && (
              <div className="task-result-body">
                <SimpleMarkdown content={result.finalText} className="product-prose" />
              </div>
            )}
            {status === "Completed" && result.totalTokens > 0 && (
              <div className="task-result-metrics">
                <span className="mono-chip">{result.totalTokens.toLocaleString()} tokens</span>
                <span className="mono-chip">{t("stream.prompt_tokens")}: {result.promptTokens.toLocaleString()}</span>
                <span className="mono-chip">{t("stream.completion_tokens")}: {result.completionTokens.toLocaleString()}</span>
              </div>
            )}
          </section>

          <section className="product-panel timeline-panel">
            <GovernanceTimeline
              spans={spans}
              title={t("workorders.task_timeline")}
              emptyText={timelineLoading ? t("settings.loading") : t("workorders.empty_timeline")}
            />
          </section>

          <details className="product-advanced">
            <summary>{t("nav.advanced")}</summary>
            <div className="product-field-grid mt-3">
              <div className="product-field"><span>work_order_id</span><strong>{taskId}</strong></div>
              <div className="product-field"><span>conversation_id</span><strong>{stringField(task, "conversation_id") || "-"}</strong></div>
              <div className="product-field"><span>contract_hash</span><strong>{stringField(task, "contract_hash") || "-"}</strong></div>
              <div className="product-field"><span>plan_hash</span><strong>{stringField(task, "plan_hash") || "-"}</strong></div>
            </div>
          </details>
        </>
      )}
    </div>
  );
}
