import { useState, useEffect } from "react";
import { listWorkOrders, executeWorkOrder, cancelWorkOrder, submitWorkOrderFeedback } from "../api/client";

export default function WorkOrders() {
  const [orders, setOrders] = useState<Record<string,unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [result, setResult] = useState("");
  const [fbText, setFbText] = useState("");
  const [fbWoId, setFbWoId] = useState("");

  async function load() { setLoading(true); try { setOrders(await listWorkOrders()||[]); } catch { setOrders([]); } setLoading(false); }
  useEffect(()=>{load();},[]);

  async function act(fn:()=>Promise<unknown>,label:string) { setResult(""); try { const r=await fn(); setResult(`${label}: ${JSON.stringify(r)}`); load(); } catch(e:unknown) { setResult(`Error: ${e instanceof Error?e.message:String(e)}`); } }

  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3"><span className="text-lg">📋</span><h2 className="text-lg font-bold">Work Orders</h2></div>
      {result && <div className="card"><pre className="text-xs" style={{color:"var(--text-secondary)"}}>{result}</pre></div>}
      {loading && <div className="text-xs" style={{color:"var(--text-muted)"}}>Loading...</div>}
      <div className="space-y-2">
        {orders.map((o:Record<string,unknown>,i:number)=>(
          <div key={i} className="card">
            <div className="flex items-center justify-between mb-1">
              <div className="flex items-center gap-2">
                <span className="text-sm font-semibold">{(o.mission_intent as string||"").slice(0,50)}</span>
                <span className={`track-${o.track}`}>{o.track as string}</span>
                <span className="text-xs px-1.5 py-0.5 rounded" style={{background:o.status==="Completed"?"var(--green-dim)":o.status==="Failed"?"var(--red-dim)":"var(--yellow-dim)",color:o.status==="Completed"?"var(--green)":o.status==="Failed"?"var(--red)":"var(--yellow)"}}>{o.status as string}</span>
              </div>
            </div>
            <div className="text-xs space-y-0.5 mt-2" style={{color:"var(--text-muted)"}}>
              <div>Contract: <span className="font-mono" style={{color:"var(--accent)"}}>{(o.contract_hash as string||"").slice(0,14)}...</span></div>
              <div>Agents: {String(o.selected_agents)} | Executors: {String(o.selected_executors)} | Skills: {String(o.required_skills)}</div>
              <div className="flex gap-2 mt-2 flex-wrap items-center">
                <button onClick={()=>act(()=>executeWorkOrder(o.work_order_id as string, o.track==="red"?{caller_identity_proof:"mock",monitoring_signature:"mock",diagnostic_signature:"mock",lease_id:"mock"}:{}),"Execute")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>Execute</button>
                <button onClick={()=>act(()=>cancelWorkOrder(o.work_order_id as string),"Cancel")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--yellow)",color:"var(--yellow)"}}>Cancel</button>
                <input placeholder="Feedback..." value={fbWoId===o.work_order_id?fbText:""} onChange={e=>{setFbWoId(o.work_order_id as string);setFbText(e.target.value)}} className="text-xs px-2 py-1 rounded border w-32" style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}} />
                <button onClick={()=>act(()=>submitWorkOrderFeedback(o.work_order_id as string,fbText),"Feedback")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--yellow)",color:"var(--yellow)"}}>Feedback</button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
