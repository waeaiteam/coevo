import { useEffect, useState } from "react";
import {
  cancelWorkOrder,
  executeWorkOrder,
  getWorkOrderAuditExport,
  getWorkOrderTimeline,
  listWorkOrders,
  submitWorkOrderFeedback,
} from "../api/client";

export default function WorkOrders() {
  const [orders, setOrders] = useState<Record<string,unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [result, setResult] = useState("");
  const [fbText, setFbText] = useState("");
  const [fbWoId, setFbWoId] = useState("");
  const [timelineWoId, setTimelineWoId] = useState("");
  const [timelineTrack, setTimelineTrack] = useState("");
  const [timeline, setTimeline] = useState<Record<string,unknown>[]>([]);

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
      setResult(`Error: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function showTimeline(id: string, track: string) {
    setTimelineWoId(id);
    setTimelineTrack(track);
    setTimeline([]);
    try { setTimeline(await getWorkOrderTimeline(id)); }
    catch(e:unknown) { setResult(`Timeline error: ${e instanceof Error ? e.message : String(e)}`); }
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg">WO</span>
        <h2 className="text-lg font-bold">Work Orders</h2>
      </div>

      {result && <div className="card"><pre className="text-xs whitespace-pre-wrap" style={{color:"var(--text-secondary)"}}>{result}</pre></div>}

      {timelineWoId && (
        <div className="card">
          <div className="text-xs font-semibold mb-2" style={{color:"var(--text-primary)"}}>Timeline: {timelineWoId}</div>
          {timeline.length === 0 && (
            <div className="text-xs" style={{color:"var(--text-muted)"}}>
              {timelineTrack === "red"
                ? "Red Track is blocked in Alpha. No execution timeline will be produced; the WorkOrder itself is the audit record."
                : "No timeline events yet. Execute the WorkOrder to create worker audit events."}
            </div>
          )}
          <div className="space-y-2">
            {timeline.slice(0, 20).map((item, i) => (
              <div key={i} className="text-xs flex gap-2">
                <span className="font-mono" style={{color:"var(--accent)"}}>{String(item.type || item.title || "event")}</span>
                <span style={{color:"var(--text-secondary)"}}>{String(item.title || "")}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {loading && <div className="text-xs" style={{color:"var(--text-muted)"}}>Loading...</div>}

      <div className="space-y-2">
        {orders.map((o:Record<string,unknown>, i:number) => {
          const id = String(o.work_order_id || "");
          const track = String(o.track || "");
          const status = String(o.status || "");
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
                <div>Contract: <span className="font-mono" style={{color:"var(--accent)"}}>{String(o.contract_hash || "").slice(0,14)}...</span></div>
                <div>Agents: {String(o.selected_agents)} | Executors: {String(o.selected_executors)} | Skills: {String(o.required_skills)}</div>
                {track === "red" && (
                  <div style={{color:"var(--red)"}}>Red Track is blocked in Alpha until production MFA, dual-sign, and lease verifier exist.</div>
                )}
                {track === "yellow" && (
                  <div style={{color:"var(--yellow)"}}>Yellow Track is not executed immediately. Submit it for approval, then execute after a valid approval receipt exists.</div>
                )}
                <div className="flex gap-2 mt-2 flex-wrap items-center">
                  <button
                    onClick={() => act(() => executeWorkOrder(id, {}), track === "yellow" ? "Submit for Approval" : "Execute")}
                    disabled={track === "red"}
                    className="px-2 py-1 text-xs rounded border"
                    style={{borderColor:track === "red" ? "var(--red)" : "var(--accent)", color:track === "red" ? "var(--red)" : "var(--accent)", opacity:track === "red" ? 0.5 : 1}}
                  >
                    {track === "yellow" ? "Submit for Approval" : "Execute"}{track === "red" ? " (Blocked)" : ""}
                  </button>
                  <button onClick={() => showTimeline(id, track)} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>View Timeline</button>
                  <button onClick={() => act(() => getWorkOrderAuditExport(id), "Audit Export")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>Export Audit</button>
                  <button onClick={() => act(() => cancelWorkOrder(id), "Cancel")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--yellow)",color:"var(--yellow)"}}>Cancel</button>
                  <input
                    placeholder="Feedback..."
                    value={fbWoId === id ? fbText : ""}
                    onChange={(e) => { setFbWoId(id); setFbText(e.target.value); }}
                    className="text-xs px-2 py-1 rounded border w-32"
                    style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}
                  />
                  <button onClick={() => act(() => submitWorkOrderFeedback(id, fbText), "Feedback")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--yellow)",color:"var(--yellow)"}}>Feedback</button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
