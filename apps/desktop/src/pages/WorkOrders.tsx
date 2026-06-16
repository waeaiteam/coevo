import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import {
} from "../api/client";
import { getActiveOpcId } from "../api/companies";
import {
  cancelCompanyWorkOrder,
  decideCompanyWorkOrderApproval,
  executeCompanyWorkOrder,
  getCompanyWorkOrderAuditExport,
  getCompanyWorkOrderTimeline,
  listCompanyWorkOrders,
  submitCompanyWorkOrderFeedback,
} from "../api/org";
import GovernanceTimeline, { type TimelineSpan } from "../components/GovernanceTimeline";
import { useToast } from "../components/ToastProvider";
import { t, useLanguage } from "../settings/i18n";

type WorkOrderRecord = Record<string, unknown>;

type RowResult = {
  label: string;
  payload: Record<string, unknown>;
};

function friendlyStatus(status: string): string {
  if (status === "Completed") return t("workorders.status_completed");
  if (status === "Failed") return t("workorders.status_failed");
  if (status === "Cancelled") return t("workorders.status_cancelled");
  if (status === "WaitingApproval") return t("workorders.status_waiting");
  if (status === "Running") return t("workorders.status_running");
  return t("workorders.status_ready");
}

function summarizeExecutionError(input: unknown): string {
  const raw = String(input || "").trim();
  if (!raw) return "Model is unavailable right now. Please check model settings and try again.";
  if (raw === "ok" || raw === "\"ok\"") return "Model returned an invalid response. Please retry or change model.";
  if (raw.includes("MODEL_ROUTE_UNAVAILABLE") || raw.includes("Provider unreachable")) {
    return "Model execution is unavailable right now. Please check model settings and try again.";
  }
  if (raw.includes("JSON schema violation") || raw.includes("EOF while parsing")) {
    return "Model returned an invalid structured response. Please switch model or retry.";
  }
  return raw;
}

function displayAssignee(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed || trimmed === "-") return t("mission.default_employee");
  if (trimmed.startsWith("agent-")) return t("mission.default_employee");
  return trimmed;
}

function parseJson(value: unknown) {
  if (typeof value !== "string" || !value.trim()) return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

function timelineToSpans(items: WorkOrderRecord[]): TimelineSpan[] {
  return items.map((item, index) => {
    const details = (item.details || {}) as Record<string, unknown>;
    const input = parseJson(details.input_json);
    const output = parseJson(details.output_json);
    const payload = parseJson(details.payload_json) as Record<string, unknown>;
    const outputRecord = output && typeof output === "object" ? output as Record<string, unknown> : {};
    const eventType = String(item.type || "event");
    const eventStatus = String(details.status || payload?.status || "");
    const hasFailure = eventType === "LifecycleError" || eventStatus === "Failed" || Boolean(payload?.error);
    const gate = outputRecord.gate && typeof outputRecord.gate === "object"
      ? outputRecord.gate as TimelineSpan["gate"]
      : item.type === "ApprovalRequired"
        ? {
            outcome: "need_approval",
            reason: String(payload?.reason || details.reason || t("timeline.approval_desc")),
            action_digest: String(payload?.action_digest || details.action_digest || payload?.approval_id || details.approval_id || ""),
          }
        : payload?.reason
          ? { outcome: item.type === "WorkerBlocked" || hasFailure ? "blocked" : "allow", reason: String(payload.reason), action_digest: String(payload.action_digest || "") }
          : { outcome: hasFailure ? "blocked" : "allow", reason: hasFailure ? summarizeExecutionError(payload?.error) : undefined };
    const started = Number(details.started_at_ms || item.time_ms || 0);
    const ended = Number(details.ended_at_ms || started);
    return {
      id: String(details.step_id || details.event_id || `${eventType}-${index}`),
      type: eventType,
      label: String(item.title || item.label || item.message || item.type || "步骤"),
      round: Number((outputRecord.round ?? (payload as Record<string, unknown>)?.round ?? 0) || 0),
      durationMs: Math.max(0, ended - started),
      tokens: Number((outputRecord.usage_total as Record<string, unknown> | undefined)?.total_tokens || (outputRecord.usage as Record<string, unknown> | undefined)?.total_tokens || 0),
      costUsd: Number(outputRecord.cost_total_usd || outputRecord.estimated_cost_usd || 0),
      trust: eventType.includes("Executor") ? "external" : "native",
      gate,
      overlays: gate?.outcome === "deny" ? ["deny"] : gate?.outcome === "need_approval" ? ["need_approval"] : gate?.outcome === "blocked" && !hasFailure ? ["sandbox_blocked"] : [],
      thought: String(outputRecord.thought || ""),
      proposal: outputRecord.proposal,
      confidence: Number(outputRecord.confidence || 0),
      usage: outputRecord.usage || outputRecord.usage_total,
      input,
      output: output || payload || item.details,
    };
  });
}

function stringField(row: WorkOrderRecord | undefined, key: string): string {
  const value = row?.[key];
  return value == null ? "" : String(value);
}

function listField(row: WorkOrderRecord | undefined, key: string): string {
  const value = row?.[key];
  if (Array.isArray(value)) return value.map(String).join(", ") || "-";
  if (value == null || value === "") return "-";
  return String(value);
}

function shortHash(value: string): string {
  return value ? `${value.slice(0, 14)}...` : "-";
}

function statusTone(status: string) {
  if (status === "Completed") return { background: "var(--green-dim)", color: "var(--green)" };
  if (status === "Failed") return { background: "var(--red-dim)", color: "var(--red)" };
  return { background: "var(--yellow-dim)", color: "var(--yellow)" };
}

function nextActionLabel(status: string, track: string, running: boolean): string {
  if (running || status === "Running") return t("workorders.next_running");
  if (track === "red") return t("workorders.next_blocked");
  if (status === "Cancelled") return t("workorders.next_cancelled");
  if (status === "Completed") return t("workorders.next_view_result");
  if (status === "Failed") return t("workorders.next_retry");
  if (status === "WaitingApproval") return t("workorders.next_waiting");
  if (track === "yellow") return t("workorders.next_submit_approval");
  return t("workorders.next_execute");
}

export default function WorkOrders() {
  useLanguage();
  const toast = useToast();
  const activeOpcId = useMemo(() => getActiveOpcId(), []);
  const [orders, setOrders] = useState<WorkOrderRecord[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [loading, setLoading] = useState(true);
  const [result, setResult] = useState("");
  const [rowResults, setRowResults] = useState<Record<string, RowResult>>({});
  const [runningIds, setRunningIds] = useState<Record<string, boolean>>({});
  const [fbText, setFbText] = useState("");
  const [fbWoId, setFbWoId] = useState("");
  const [timelineWoIds, setTimelineWoIds] = useState<Record<string, string>>({});
  const [timelines, setTimelines] = useState<Record<string, WorkOrderRecord[]>>({});

  async function load() {
    setLoading(true);
    try {
      setOrders((await listCompanyWorkOrders(activeOpcId)) || []);
    } catch {
      setOrders([]);
    }
    setLoading(false);
  }

  useEffect(() => {
    void load();
  }, [activeOpcId]);

  useEffect(() => {
    if (orders.length === 0) {
      setSelectedId("");
      return;
    }
    const hasSelected = orders.some((order) => String(order.work_order_id || "") === selectedId);
    if (!hasSelected) setSelectedId(String(orders[0].work_order_id || ""));
  }, [orders, selectedId]);

  const metrics = useMemo(() => {
    const waiting = orders.filter((order) => String(order.status || "") === "WaitingApproval").length;
    const red = orders.filter((order) => String(order.track || "") === "red").length;
    const completed = orders.filter((order) => String(order.status || "") === "Completed").length;
    return { total: orders.length, waiting, red, completed };
  }, [orders]);

  const selected = useMemo(() => {
    return orders.find((order) => String(order.work_order_id || "") === selectedId) || orders[0];
  }, [orders, selectedId]);

  async function act(fn: () => Promise<unknown>, label: string): Promise<boolean> {
    setResult("");
    try {
      const r = await fn();
      setResult(`${label}: ${JSON.stringify(r)}`);
      load();
      return true;
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      setResult(`${t("workorders.result_error")}: ${message}`);
      toast.error(`${t("toast.workorder_action_failed")}: ${message}`);
      return false;
    }
  }

  async function submitFeedback(id: string, feedback: string) {
    setResult("");
    try {
      const r = await submitCompanyWorkOrderFeedback(activeOpcId, id, feedback);
      setResult(`${t("workorders.feedback")}: ${JSON.stringify(r)}`);
      setFbWoId("");
      setFbText("");
      void load();
      toast.success(t("toast.workorder_feedback_sent"));
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      setResult(`${t("workorders.result_error")}: ${message}`);
      toast.error(`${t("toast.workorder_action_failed")}: ${message}`);
    }
  }

  async function showTimeline(id: string, track: string) {
    setTimelineWoIds((prev) => ({ ...prev, [id]: track }));
    setTimelines((prev) => ({ ...prev, [id]: [] }));
    try {
      const items = await getCompanyWorkOrderTimeline(activeOpcId, id);
      setTimelines((prev) => ({ ...prev, [id]: items }));
    } catch (e: unknown) {
      setResult(`${t("workorders.result_timeline_error")}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function decideApproval(id: string, decision: "approve" | "reject", comment: string) {
    const approvalId =
      String(rowResults[id]?.payload?.approval_id || "") || findApprovalId(timelines[id] || []);
    if (!approvalId) {
      setResult(`${t("workorders.result_error")}: ${t("workorders.approval_missing")}`);
      toast.error(`${t("toast.workorder_action_failed")}: ${t("workorders.approval_missing")}`);
      return;
    }
    try {
      const payload = await decideCompanyWorkOrderApproval(activeOpcId, id, {
        approval_id: approvalId,
        decision,
        comment,
      });
      setRowResults((prev) => ({
        ...prev,
        [id]: {
          label: decision === "approve" ? t("workorders.approved") : t("workorders.rejected"),
          payload: payload as Record<string, unknown>,
        },
      }));
      await load();
      await showTimeline(id, selectedTrack);
      toast.success(t("toast.approval_recorded"));
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      setResult(`${t("workorders.result_error")}: ${message}`);
      toast.error(`${t("toast.workorder_action_failed")}: ${message}`);
    }
  }

  async function executeRow(id: string, track: string, rerun = false) {
    setRunningIds((prev) => ({ ...prev, [id]: true }));
    setRowResults((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
    try {
      const label = rerun ? t("workorders.run_again") : track === "yellow" ? t("workorders.submit_approval") : t("workorders.execute");
      const payload = (await executeCompanyWorkOrder(
        activeOpcId,
        id,
        rerun ? { rerun: true } : {},
      )) as WorkOrderRecord;
      if (String(payload.summary || "").includes("WorkerHarness")) {
        payload.summary = t("workorders.completed_summary");
      }
      setRowResults((prev) => ({ ...prev, [id]: { label, payload } }));
      await load();
      await showTimeline(id, track);
    } catch (e: unknown) {
      const message = summarizeExecutionError(e instanceof Error ? e.message : String(e));
      setOrders((prev) =>
        prev.map((order) =>
          String(order.work_order_id || "") === id ? { ...order, status: "Failed" } : order,
        ),
      );
      setRowResults((prev) => ({
        ...prev,
        [id]: {
          label: t("workorders.result_error"),
          payload: { error: message },
        },
      }));
      await load();
      toast.error(`${t("toast.workorder_action_failed")}: ${message}`);
    } finally {
      setRunningIds((prev) => ({ ...prev, [id]: false }));
    }
  }

  const selectedIdValue = stringField(selected, "work_order_id");
  const selectedTrack = stringField(selected, "track");
  const selectedStatus = stringField(selected, "status");
  const selectedCompleted = selectedStatus === "Completed";
  const selectedFailed = selectedStatus === "Failed";
  const selectedCancelled = selectedStatus === "Cancelled";
  const selectedReadyToExecute = !selectedCompleted && !selectedFailed && !selectedCancelled && selectedStatus !== "Running";
  const selectedCanRerun = (selectedCompleted || selectedFailed) && selectedTrack !== "red";
  const selectedRerunBlocked = (selectedCompleted || selectedFailed) && selectedTrack === "red";
  const selectedRunning = Boolean(runningIds[selectedIdValue]) || selectedStatus === "Running";
  const selectedTimelineTrack = timelineWoIds[selectedIdValue];
  const selectedTimeline = timelines[selectedIdValue] || [];
  const selectedTimelineSpans = useMemo(() => timelineToSpans(selectedTimeline), [selectedTimeline]);
  const selectedResult = rowResults[selectedIdValue];
  const currentFeedback = fbWoId === selectedIdValue ? fbText : "";
  const selectedNextAction = nextActionLabel(selectedStatus, selectedTrack, selectedRunning);

  return (
    <div className="space-y-5">
      <header className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
        <div>
          <div className="text-xs font-semibold uppercase" style={{ color: "var(--text-muted)" }}>{t("nav.tasks")}</div>
          <h2 className="mt-1 text-xl font-bold">{t("workorders.task_center")}</h2>
          <p className="mt-1 max-w-2xl text-sm" style={{ color: "var(--text-secondary)" }}>
            {t("workorders.task_center_desc")}
          </p>
        </div>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Metric label={`${metrics.total} ${t("workorders.metric_total_tasks")}`} value={String(metrics.total)} />
          <Metric label={`${metrics.waiting} ${t("workorders.metric_waiting_approval")}`} value={String(metrics.waiting)} />
          <Metric label={`${metrics.red} ${t("workorders.metric_red_blocked")}`} value={String(metrics.red)} />
          <Metric label={`${metrics.completed} ${t("workorders.metric_completed")}`} value={String(metrics.completed)} />
        </div>
      </header>

      {result && (
        <div className="card">
          <pre className="text-xs whitespace-pre-wrap" style={{ color: "var(--text-secondary)" }}>{result}</pre>
        </div>
      )}

      {loading && <div className="text-xs" style={{ color: "var(--text-muted)" }}>{t("workorders.loading")}</div>}

      {!loading && orders.length === 0 && (
        <div className="card">
          <div className="text-sm font-semibold">{t("workorders.no_tasks")}</div>
          <p className="mt-1 text-xs" style={{ color: "var(--text-secondary)" }}>
            {t("workorders.no_tasks_desc")}
          </p>
        </div>
      )}

      {orders.length > 0 && (
        <div className="grid gap-4 xl:grid-cols-[0.85fr_1.15fr]">
          <section className="card space-y-3">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold">{t("workorders.task_list")}</h3>
              <span className="text-xs" style={{ color: "var(--text-muted)" }}>{orders.length}</span>
            </div>
            <div className="space-y-2">
              {orders.map((order, index) => {
                const id = stringField(order, "work_order_id") || String(index);
                const status = stringField(order, "status");
                const selectedRow = id === selectedIdValue;
                const ownerLabel = displayAssignee((listField(order, "selected_agents").split(",")[0] || "").trim());
                return (
                  <button
                    key={id}
                    type="button"
                    onClick={() => setSelectedId(id)}
                    className="w-full rounded-md border p-3 text-left transition"
                    style={{
                      borderColor: selectedRow ? "var(--accent)" : "var(--border-subtle)",
                      background: selectedRow ? "var(--surface-raised)" : "var(--bg-card)",
                    }}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-semibold">{stringField(order, "mission_intent") || "Untitled task"}</div>
                        <div className="mt-1 text-[11px]" style={{ color: "var(--text-muted)" }}>{ownerLabel}</div>
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        <span className="rounded px-1.5 py-0.5 text-[11px]" style={statusTone(status)}>{friendlyStatus(status)}</span>
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          </section>

          <section className="space-y-4">
            <div className="card">
              <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                <div>
                  <h3 className="text-sm font-semibold">{t("workorders.selected_details")}</h3>
                  <p className="mt-1 text-base font-semibold">{stringField(selected, "mission_intent") || "Untitled task"}</p>
                </div>
                <div className="flex items-center gap-2">
                  <span className="rounded px-2 py-1 text-xs" style={statusTone(selectedStatus)}>{friendlyStatus(selectedStatus)}</span>
                </div>
              </div>

              <div className="mt-4 grid gap-3 md:grid-cols-3">
                <InfoRow label={t("workorders.assigned_ai_employees")} value={displayAssignee(listField(selected, "selected_agents").split(",")[0] || "")} />
                <InfoRow label={t("workorders.timeline")} value={selectedTimeline.length > 0 ? `${selectedTimeline.length} ${t("workorders.events")}` : t("workorders.no_events")} />
                <InfoRow label={t("workorders.next_action")} value={selectedNextAction} />
              </div>
              <details className="mt-3 rounded-md border p-3" style={{ borderColor: "var(--border-subtle)" }}>
                <summary className="cursor-pointer text-xs font-semibold">{t("app.alpha_console")}</summary>
                <div className="mt-3 grid gap-3 md:grid-cols-2">
                  <InfoRow label={t("workorders.assigned_ai_employees")} value={listField(selected, "selected_agents")} />
                  <InfoRow label={t("workorders.contract")} value={shortHash(stringField(selected, "contract_hash"))} mono />
                  <InfoRow label={t("workorders.executors")} value={listField(selected, "selected_executors")} />
                  <InfoRow label={t("workorders.skills")} value={listField(selected, "required_skills")} />
                  <InfoRow label={t("workorders.internal_task_id")} value={selectedIdValue || "-"} mono />
                </div>
              </details>
            </div>

            <div className="card space-y-3">
              <div>
                <h3 className="text-sm font-semibold">{t("workorders.approval_audit")}</h3>
                <p className="mt-1 text-xs" style={{ color: "var(--text-secondary)" }}>
                  {t("workorders.approval_audit_desc")}
                </p>
              </div>

              {selectedTrack === "red" && (
                <div className="rounded-md border p-3 text-xs" style={{ borderColor: "var(--red)", color: "var(--red)" }}>
                  {t("workorders.red_block")}
                </div>
              )}
              {selectedTrack === "yellow" && (
                <div className="rounded-md border p-3 text-xs" style={{ borderColor: "var(--yellow)", color: "var(--yellow)" }}>
                  <span className="font-semibold">{t("workorders.yellow_approval_required")}</span>
                  <span> - {t("workorders.yellow_notice")}</span>
                </div>
              )}

              <div className="flex flex-wrap items-center gap-2">
                {selectedReadyToExecute && (
                  <button
                    onClick={() => executeRow(selectedIdValue, selectedTrack)}
                    disabled={selectedTrack === "red" || selectedRunning}
                    className="rounded border px-2 py-1 text-xs"
                    style={{
                      borderColor: selectedTrack === "red" ? "var(--red)" : "var(--accent)",
                      color: selectedTrack === "red" ? "var(--red)" : "var(--accent)",
                      opacity: selectedTrack === "red" || selectedRunning ? 0.5 : 1,
                    }}
                  >
                    {selectedRunning ? t("workorders.running") : selectedTrack === "yellow" ? t("workorders.submit_approval") : selectedTrack === "red" ? t("workorders.execute_blocked") : t("workorders.execute")}
                  </button>
                )}
                {(selectedCompleted || selectedFailed) && (
                  <>
                    {selectedCompleted && (
                      <Link to={`/tasks/${encodeURIComponent(selectedIdValue)}`} className="rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)", textDecoration: "none" }}>{t("workorders.view_result")}</Link>
                    )}
                    <button onClick={() => executeRow(selectedIdValue, selectedTrack, true)} disabled={selectedRunning || !selectedCanRerun} className="rounded border px-2 py-1 text-xs" style={{ borderColor: selectedRerunBlocked ? "var(--red)" : "var(--border-accent)", color: selectedRerunBlocked ? "var(--red)" : "var(--text-secondary)", opacity: selectedRunning || !selectedCanRerun ? 0.5 : 1 }}>{selectedRunning ? t("workorders.running") : t("workorders.run_again")}</button>
                  </>
                )}
                <button onClick={() => showTimeline(selectedIdValue, selectedTrack)} className="rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("workorders.view_timeline")}</button>
                <button onClick={() => { void act(() => getCompanyWorkOrderAuditExport(activeOpcId, selectedIdValue), t("workorders.export_audit")).then((ok) => { if (ok) toast.success(t("toast.workorder_audit_exported")); }); }} className="rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("workorders.export_audit")}</button>
                <button onClick={() => { void act(() => cancelCompanyWorkOrder(activeOpcId, selectedIdValue), t("workorders.cancel")).then((ok) => { if (ok) toast.info(t("toast.workorder_cancelled")); }); }} className="rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--yellow)", color: "var(--yellow)" }}>{t("workorders.cancel")}</button>
                <input
                  placeholder={t("workorders.feedback_placeholder")}
                  value={currentFeedback}
                  onChange={(e) => {
                    setFbWoId(selectedIdValue);
                    setFbText(e.target.value);
                  }}
                  className="w-36 rounded border px-2 py-1 text-xs"
                  style={{ borderColor: "var(--border-accent)", color: "var(--text-secondary)" }}
                />
                <button
                  onClick={() => {
                    if (!currentFeedback.trim()) return;
                    submitFeedback(selectedIdValue, currentFeedback);
                  }}
                  disabled={!currentFeedback.trim()}
                  className="rounded border px-2 py-1 text-xs"
                  style={{ borderColor: "var(--yellow)", color: "var(--yellow)", opacity: currentFeedback.trim() ? 1 : 0.5 }}
                >
                  {t("workorders.feedback")}
                </button>
              </div>

              {selectedResult && <ExecutionSummary result={selectedResult} />}
            </div>

            <div className="card">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold">{t("workorders.task_timeline")}</h3>
                <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>{friendlyStatus(selectedStatus)}</span>
              </div>
              <div className="mt-3 rounded border p-3" style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}>
                {!selectedTimelineTrack && (
                  <div className="text-xs" style={{ color: "var(--text-muted)" }}>
                    {t("workorders.timeline_hint")}
                  </div>
                )}
                {selectedTimelineTrack && selectedTimeline.length === 0 && (
                  <div className="text-xs" style={{ color: "var(--text-muted)" }}>
                    {selectedTimelineTrack === "red" ? t("workorders.red_no_timeline") : t("workorders.empty_timeline")}
                  </div>
                )}
                {selectedTimeline.length > 0 && (
                  <div className="h-[520px]">
                    <GovernanceTimeline
                      spans={selectedTimelineSpans}
                      title={t("workorders.task_timeline")}
                      onApprove={(_, comment) => decideApproval(selectedIdValue, "approve", comment)}
                      onReject={(_, comment) => decideApproval(selectedIdValue, "reject", comment)}
                    />
                  </div>
                )}
              </div>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border px-3 py-2" style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}>
      <div className="text-lg font-semibold">{value}</div>
      <div className="text-[11px]" style={{ color: "var(--text-muted)" }}>{label}</div>
    </div>
  );
}

function InfoRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="rounded-md border p-3" style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}>
      <div className="text-[11px] uppercase" style={{ color: "var(--text-muted)" }}>{label}</div>
      <div className={`mt-1 text-xs ${mono ? "font-mono" : ""}`} style={{ color: "var(--text-secondary)" }}>{value}</div>
    </div>
  );
}

function ExecutionSummary({ result }: { result: RowResult }) {
  const payload = result.payload;
  const status = String(payload.status || "");
  const rawError = String(payload.error || "").trim();
  const hasError = rawError.length > 0;
  const summary = summarizeExecutionError(String(payload.summary || payload.message || payload.error || ""));
  const approvalId = String(payload.approval_id || "");
  const memoryIds = Array.isArray(payload.memory_ids) ? payload.memory_ids.map(String) : [];
  const runs = Array.isArray(payload.worker_runs) ? payload.worker_runs : [];
  const steps = Array.isArray(payload.worker_steps) ? payload.worker_steps : [];
  const toolCalls = Array.isArray(payload.tool_calls) ? payload.tool_calls : [];

  return (
    <div className="rounded border p-3 text-xs" style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)", color: "var(--text-secondary)" }}>
      <div className="font-semibold" style={{ color: "var(--text-primary)" }}>
        {hasError ? t("workorders.execution_failed") : result.label}: {hasError ? t("workorders.failed") : friendlyStatus(status)}
      </div>
      {summary && <div className="mt-1">{summary}</div>}
      <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
        {approvalId && <span>{t("workorders.summary_approval")}: <span className="font-mono">{approvalId}</span></span>}
        {runs.length > 0 && <span>{t("workorders.summary_runs")}: {runs.length}</span>}
        {steps.length > 0 && <span>{t("workorders.summary_steps")}: {steps.length}</span>}
        {toolCalls.length > 0 && <span>{t("workorders.summary_tools")}: {toolCalls.map((tc) => String((tc as WorkOrderRecord).tool_id || "tool")).join(", ")}</span>}
        {memoryIds.length > 0 && <span>{t("workorders.summary_memory")}: <span className="font-mono">{memoryIds.join(", ")}</span></span>}
      </div>
    </div>
  );
}

function findApprovalId(timeline: WorkOrderRecord[]): string {
  for (const item of timeline) {
    const details = (item.details || {}) as Record<string, unknown>;
    const payload = parseJson(details.payload_json) as Record<string, unknown>;
    const approvalId = String(payload.approval_id || payload.approval_receipt || details.approval_id || "").trim();
    if (approvalId) return approvalId;
  }
  return "";
}
