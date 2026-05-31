import { useEffect, useMemo, useState } from "react";
import {
  cancelWorkOrder,
  executeWorkOrder,
  getWorkOrderAuditExport,
  getWorkOrderTimeline,
  listWorkOrders,
  submitWorkOrderFeedback,
} from "../api/client";
import { t, useLanguage } from "../settings/i18n";

type WorkOrderRecord = Record<string, unknown>;

type RowResult = {
  label: string;
  payload: Record<string, unknown>;
};

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

export default function WorkOrders() {
  useLanguage();
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
      setOrders((await listWorkOrders()) || []);
    } catch {
      setOrders([]);
    }
    setLoading(false);
  }

  useEffect(() => {
    load();
  }, []);

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

  async function act(fn: () => Promise<unknown>, label: string) {
    setResult("");
    try {
      const r = await fn();
      setResult(`${label}: ${JSON.stringify(r)}`);
      load();
    } catch (e: unknown) {
      setResult(`${t("workorders.result_error")}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function submitFeedback(id: string, feedback: string) {
    setResult("");
    try {
      const r = await submitWorkOrderFeedback(id, feedback);
      setResult(`${t("workorders.feedback")}: ${JSON.stringify(r)}`);
      setFbWoId("");
      setFbText("");
      load();
    } catch (e: unknown) {
      setResult(`${t("workorders.result_error")}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function showTimeline(id: string, track: string) {
    setTimelineWoIds((prev) => ({ ...prev, [id]: track }));
    setTimelines((prev) => ({ ...prev, [id]: [] }));
    try {
      const items = await getWorkOrderTimeline(id);
      setTimelines((prev) => ({ ...prev, [id]: items }));
    } catch (e: unknown) {
      setResult(`${t("workorders.result_timeline_error")}: ${e instanceof Error ? e.message : String(e)}`);
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
      const label = track === "yellow" ? t("workorders.submit_approval") : rerun ? t("workorders.run_again") : t("workorders.execute");
      const payload = (await executeWorkOrder(id, rerun ? { rerun: true } : {})) as WorkOrderRecord;
      setRowResults((prev) => ({ ...prev, [id]: { label, payload } }));
      await load();
      await showTimeline(id, track);
    } catch (e: unknown) {
      setRowResults((prev) => ({
        ...prev,
        [id]: {
          label: t("workorders.result_error"),
          payload: { error: e instanceof Error ? e.message : String(e) },
        },
      }));
    } finally {
      setRunningIds((prev) => ({ ...prev, [id]: false }));
    }
  }

  const selectedIdValue = stringField(selected, "work_order_id");
  const selectedTrack = stringField(selected, "track");
  const selectedStatus = stringField(selected, "status");
  const selectedCompleted = selectedStatus === "Completed";
  const selectedRunning = Boolean(runningIds[selectedIdValue]) || selectedStatus === "Running";
  const selectedTimelineTrack = timelineWoIds[selectedIdValue];
  const selectedTimeline = timelines[selectedIdValue] || [];
  const selectedResult = rowResults[selectedIdValue];
  const currentFeedback = fbWoId === selectedIdValue ? fbText : "";

  return (
    <div className="space-y-5">
      <header className="flex flex-col gap-3 lg:flex-row lg:items-end lg:justify-between">
        <div>
          <div className="text-xs font-semibold uppercase" style={{ color: "var(--text-muted)" }}>OPC</div>
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
                const track = stringField(order, "track");
                const status = stringField(order, "status");
                const selectedRow = id === selectedIdValue;
                return (
                  <button
                    key={id}
                    type="button"
                    onClick={() => setSelectedId(id)}
                    className="w-full rounded-md border p-3 text-left transition"
                    style={{
                      borderColor: selectedRow ? "var(--accent)" : "var(--border-subtle)",
                      background: selectedRow ? "var(--surface-raised)" : "#fff",
                    }}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-semibold">{stringField(order, "mission_intent") || "Untitled task"}</div>
                        <div className="mt-1 font-mono text-[11px]" style={{ color: "var(--text-muted)" }}>{id}</div>
                      </div>
                      <div className="flex shrink-0 items-center gap-1">
                        <span className={`track-${track}`}>{track}</span>
                        <span className="rounded px-1.5 py-0.5 text-[11px]" style={statusTone(status)}>{status || "Planned"}</span>
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
                  <span className={`track-${selectedTrack}`}>{selectedTrack}</span>
                  <span className="rounded px-2 py-1 text-xs" style={statusTone(selectedStatus)}>{selectedStatus || "Planned"}</span>
                </div>
              </div>

              <div className="mt-4 grid gap-3 md:grid-cols-2">
                <InfoRow label={t("workorders.contract")} value={shortHash(stringField(selected, "contract_hash"))} mono />
                <InfoRow label={t("workorders.assigned_ai_employees")} value={listField(selected, "selected_agents")} />
                <InfoRow label={t("workorders.executors")} value={listField(selected, "selected_executors")} />
                <InfoRow label={t("workorders.skills")} value={listField(selected, "required_skills")} />
              </div>
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
                {!selectedCompleted && (
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
                {selectedCompleted && (
                  <>
                    <button onClick={() => showTimeline(selectedIdValue, selectedTrack)} className="rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("workorders.view_result")}</button>
                    <button onClick={() => executeRow(selectedIdValue, selectedTrack, true)} disabled={selectedRunning || selectedTrack === "red"} className="rounded border px-2 py-1 text-xs" style={{ borderColor: selectedTrack === "red" ? "var(--red)" : "var(--border-accent)", color: selectedTrack === "red" ? "var(--red)" : "var(--text-secondary)", opacity: selectedRunning || selectedTrack === "red" ? 0.5 : 1 }}>{selectedRunning ? t("workorders.running") : t("workorders.run_again")}</button>
                  </>
                )}
                <button onClick={() => showTimeline(selectedIdValue, selectedTrack)} className="rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("workorders.view_timeline")}</button>
                <button onClick={() => act(() => getWorkOrderAuditExport(selectedIdValue), t("workorders.export_audit"))} className="rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("workorders.export_audit")}</button>
                <button onClick={() => act(() => cancelWorkOrder(selectedIdValue), t("workorders.cancel"))} className="rounded border px-2 py-1 text-xs" style={{ borderColor: "var(--yellow)", color: "var(--yellow)" }}>{t("workorders.cancel")}</button>
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
                <span className="font-mono text-[11px]" style={{ color: "var(--text-muted)" }}>{selectedIdValue}</span>
              </div>
              <div className="mt-3 rounded border p-3" style={{ borderColor: "var(--border-subtle)", background: "#fff" }}>
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
                <div className="space-y-2">
                  {selectedTimeline.slice(0, 20).map((item, timelineIndex) => (
                    <div key={timelineIndex} className="flex gap-2 text-xs">
                      <span className="font-mono" style={{ color: "var(--accent)" }}>{String(item.type || item.title || "event")}</span>
                      <span style={{ color: "var(--text-secondary)" }}>{String(item.title || "")}</span>
                    </div>
                  ))}
                </div>
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
    <div className="rounded-md border px-3 py-2" style={{ borderColor: "var(--border-subtle)", background: "#fff" }}>
      <div className="text-lg font-semibold">{value}</div>
      <div className="text-[11px]" style={{ color: "var(--text-muted)" }}>{label}</div>
    </div>
  );
}

function InfoRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="rounded-md border p-3" style={{ borderColor: "var(--border-subtle)", background: "#fff" }}>
      <div className="text-[11px] uppercase" style={{ color: "var(--text-muted)" }}>{label}</div>
      <div className={`mt-1 text-xs ${mono ? "font-mono" : ""}`} style={{ color: "var(--text-secondary)" }}>{value}</div>
    </div>
  );
}

function ExecutionSummary({ result }: { result: RowResult }) {
  const payload = result.payload;
  const status = String(payload.status || "");
  const summary = String(payload.summary || payload.message || payload.error || "");
  const approvalId = String(payload.approval_id || "");
  const memoryIds = Array.isArray(payload.memory_ids) ? payload.memory_ids.map(String) : [];
  const runs = Array.isArray(payload.worker_runs) ? payload.worker_runs : [];
  const steps = Array.isArray(payload.worker_steps) ? payload.worker_steps : [];
  const toolCalls = Array.isArray(payload.tool_calls) ? payload.tool_calls : [];

  return (
    <div className="rounded border p-3 text-xs" style={{ borderColor: "var(--border-subtle)", background: "#fff", color: "var(--text-secondary)" }}>
      <div className="font-semibold" style={{ color: "var(--text-primary)" }}>{result.label}: {status || "ok"}</div>
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
