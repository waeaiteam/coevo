import { useEffect, useState } from "react";
import {
  cancelWorkOrder,
  executeWorkOrder,
  getWorkOrderAuditExport,
  getWorkOrderTimeline,
  listWorkOrders,
  submitWorkOrderFeedback,
} from "../api/client";
import { t, useLanguage } from "../settings/i18n";

type RowResult = {
  label: string;
  payload: Record<string, unknown>;
};

export default function WorkOrders() {
  useLanguage();
  const [orders, setOrders] = useState<Record<string,unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [result, setResult] = useState("");
  const [rowResults, setRowResults] = useState<Record<string, RowResult>>({});
  const [runningIds, setRunningIds] = useState<Record<string, boolean>>({});
  const [fbText, setFbText] = useState("");
  const [fbWoId, setFbWoId] = useState("");
  const [timelineWoIds, setTimelineWoIds] = useState<Record<string, string>>({});
  const [timelines, setTimelines] = useState<Record<string, Record<string,unknown>[]>>({});

  async function load() {
    setLoading(true);
    try { setOrders(await listWorkOrders() || []); }
    catch { setOrders([]); }
    setLoading(false);
  }

  useEffect(() => { load(); }, []);

  async function act(fn:()=>Promise<unknown>, label:string) {
    setResult("");
    try {
      const r = await fn();
      setResult(`${label}: ${JSON.stringify(r)}`);
      load();
    } catch(e:unknown) {
      setResult(`${t("workorders.result_error")}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function showTimeline(id: string, track: string) {
    setTimelineWoIds((prev) => ({ ...prev, [id]: track }));
    setTimelines((prev) => ({ ...prev, [id]: [] }));
    try {
      const items = await getWorkOrderTimeline(id);
      setTimelines((prev) => ({ ...prev, [id]: items }));
    }
    catch(e:unknown) { setResult(`${t("workorders.result_timeline_error")}: ${e instanceof Error ? e.message : String(e)}`); }
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
      const payload = await executeWorkOrder(id, rerun ? { rerun: true } : {}) as Record<string, unknown>;
      setRowResults((prev) => ({ ...prev, [id]: { label, payload } }));
      await load();
      await showTimeline(id, track);
    } catch(e:unknown) {
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

  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg">WO</span>
        <h2 className="text-lg font-bold">{t("workorders.title")}</h2>
      </div>

      {result && <div className="card"><pre className="text-xs whitespace-pre-wrap" style={{color:"var(--text-secondary)"}}>{result}</pre></div>}

      {loading && <div className="text-xs" style={{color:"var(--text-muted)"}}>{t("workorders.loading")}</div>}

      <div className="space-y-2">
        {orders.map((o:Record<string,unknown>, i:number) => {
          const id = String(o.work_order_id || "");
          const track = String(o.track || "");
          const status = String(o.status || "");
          const running = Boolean(runningIds[id]) || status === "Running";
          const completed = status === "Completed";
          const rowResult = rowResults[id];
          const timelineTrack = timelineWoIds[id];
          const timeline = timelines[id] || [];
          return (
            <div key={id || i} className="card">
              <div className="flex items-center justify-between mb-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-semibold">{String(o.mission_intent || "").slice(0, 70)}</span>
                  <span className={`track-${track}`}>{track}</span>
                  <span
                    className="text-xs px-1.5 py-0.5 rounded"
                    style={{
                      background: status === "Completed" ? "var(--green-dim)" : status === "Failed" ? "var(--red-dim)" : "var(--yellow-dim)",
                      color: status === "Completed" ? "var(--green)" : status === "Failed" ? "var(--red)" : "var(--yellow)",
                    }}
                  >
                    {status}
                  </span>
                </div>
              </div>
              <div className="text-xs space-y-0.5 mt-2" style={{color:"var(--text-muted)"}}>
                <div>{t("workorders.contract")}: <span className="font-mono" style={{color:"var(--accent)"}}>{String(o.contract_hash || "").slice(0,14)}...</span></div>
                <div>{t("workorders.agents")}: {String(o.selected_agents)} | {t("workorders.executors")}: {String(o.selected_executors)} | {t("workorders.skills")}: {String(o.required_skills)}</div>
                {track === "red" && (
                  <div style={{color:"var(--red)"}}>{t("workorders.red_block")}</div>
                )}
                {track === "yellow" && (
                  <div style={{color:"var(--yellow)"}}>{t("workorders.yellow_notice")}</div>
                )}
                <div className="flex gap-2 mt-2 flex-wrap items-center">
                  {!completed && (
                    <button
                      onClick={() => executeRow(id, track)}
                      disabled={track === "red" || running}
                      className="px-2 py-1 text-xs rounded border"
                      style={{borderColor:track === "red" ? "var(--red)" : "var(--accent)", color:track === "red" ? "var(--red)" : "var(--accent)", opacity:track === "red" || running ? 0.5 : 1}}
                    >
                      {running ? t("workorders.running") : track === "yellow" ? t("workorders.submit_approval") : track === "red" ? t("workorders.execute_blocked") : t("workorders.execute")}
                    </button>
                  )}
                  {completed && (
                    <>
                      <button onClick={() => showTimeline(id, track)} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>{t("workorders.view_result")}</button>
                      <button onClick={() => executeRow(id, track, true)} disabled={running || track === "red"} className="px-2 py-1 text-xs rounded border" style={{borderColor:track === "red" ? "var(--red)" : "var(--border-accent)",color:track === "red" ? "var(--red)" : "var(--text-secondary)", opacity:running || track === "red" ? 0.5 : 1}}>{running ? t("workorders.running") : t("workorders.run_again")}</button>
                    </>
                  )}
                  <button onClick={() => showTimeline(id, track)} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>{t("workorders.view_timeline")}</button>
                  <button onClick={() => act(() => getWorkOrderAuditExport(id), t("workorders.export_audit"))} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>{t("workorders.export_audit")}</button>
                  <button onClick={() => act(() => cancelWorkOrder(id), t("workorders.cancel"))} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--yellow)",color:"var(--yellow)"}}>{t("workorders.cancel")}</button>
                  <input
                    placeholder={t("workorders.feedback_placeholder")}
                    value={fbWoId === id ? fbText : ""}
                    onChange={(e) => { setFbWoId(id); setFbText(e.target.value); }}
                    className="text-xs px-2 py-1 rounded border w-32"
                    style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}
                  />
                  <button onClick={() => act(() => submitWorkOrderFeedback(id, fbText), t("workorders.feedback"))} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--yellow)",color:"var(--yellow)"}}>{t("workorders.feedback")}</button>
                </div>
                {rowResult && (
                  <ExecutionSummary result={rowResult} />
                )}
                {timelineTrack && (
                  <div className="mt-3 rounded border p-3" style={{ borderColor: "var(--border-subtle)", background: "#fff" }}>
                    <div className="text-xs font-semibold mb-2" style={{color:"var(--text-primary)"}}>{t("workorders.timeline")}: {id}</div>
                    {timeline.length === 0 && (
                      <div className="text-xs" style={{color:"var(--text-muted)"}}>
                        {timelineTrack === "red" ? t("workorders.red_no_timeline") : t("workorders.empty_timeline")}
                      </div>
                    )}
                    <div className="space-y-2">
                      {timeline.slice(0, 20).map((item, timelineIndex) => (
                        <div key={timelineIndex} className="text-xs flex gap-2">
                          <span className="font-mono" style={{color:"var(--accent)"}}>{String(item.type || item.title || "event")}</span>
                          <span style={{color:"var(--text-secondary)"}}>{String(item.title || "")}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
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
    <div className="mt-3 rounded border p-3 text-xs" style={{ borderColor: "var(--border-subtle)", background: "#fff", color: "var(--text-secondary)" }}>
      <div className="font-semibold" style={{ color: "var(--text-primary)" }}>{result.label}: {status || "ok"}</div>
      {summary && <div className="mt-1">{summary}</div>}
      <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
        {approvalId && <span>{t("workorders.summary_approval")}: <span className="font-mono">{approvalId}</span></span>}
        {runs.length > 0 && <span>{t("workorders.summary_runs")}: {runs.length}</span>}
        {steps.length > 0 && <span>{t("workorders.summary_steps")}: {steps.length}</span>}
        {toolCalls.length > 0 && <span>{t("workorders.summary_tools")}: {toolCalls.map((tc) => String((tc as Record<string, unknown>).tool_id || "tool")).join(", ")}</span>}
        {memoryIds.length > 0 && <span>{t("workorders.summary_memory")}: <span className="font-mono">{memoryIds.join(", ")}</span></span>}
      </div>
    </div>
  );
}
