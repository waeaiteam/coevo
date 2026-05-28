import { useState, useEffect } from "react";
import { listExecutors, registerExecutor, disableExecutor, executorHealth, executorDryRun, listWorkOrders } from "../api/client";

const SOURCE_TYPES = ["Hermes","OpenClaw","MCP","Local302AI","Browser","LocalProcess","Docker"];
export default function ExternalExecutors() {
  const [execs, setExecs] = useState<Record<string,unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [showReg, setShowReg] = useState(false);
  const [regForm, setRegForm] = useState<Record<string,string>>({executor_id:"",display_name:"",source_type:"OpenClaw",risk_ceiling:"0.5",sandbox_level:"None"});
  const [workOrders, setWorkOrders] = useState<Record<string,unknown>[]>([]);
  const [dryRunId, setDryRunId] = useState("");
  const [dryRunResult, setDryRunResult] = useState("");

  async function load() { setLoading(true); try { setExecs(await listExecutors()||[]); } catch { setExecs([]); } setLoading(false); }
  useEffect(()=>{load(); listWorkOrders().then(w=>setWorkOrders(w||[]));},[]);

  async function reg() {
    if (!regForm.display_name) return;
    try { await registerExecutor({...regForm,executor_id:regForm.executor_id||"exec-"+crypto.randomUUID(),capabilities:["mock"],required_credentials:[],permission_boundary:{max_risk_score:0.5,can_write_fact:false,can_write_decision:false,can_access_network:false,can_access_filesystem:false,can_call_external_executor:false,can_propose_skill:false},file_scope:[],network_scope:[],memory_scope:"Executor",supported_actions:["read"],health_check_url:"",audit_callback_url:"",status:"Registered",runtime_endpoint:"http://localhost:0",created_at_ms:Date.now(),updated_at_ms:Date.now()}); setShowReg(false); load(); }
    catch(e:unknown) { alert(String(e)); }
  }
  async function dryRun(eid:string,woid:string) {
    setDryRunResult(""); try { const r = await executorDryRun(eid,woid); setDryRunResult(JSON.stringify(r)); } catch(e:unknown) { setDryRunResult("Error: "+(e instanceof Error?e.message:String(e))); }
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3"><span className="text-lg">🔌</span><h2 className="text-lg font-bold">External Executors</h2></div>
        <button onClick={()=>setShowReg(!showReg)} className="px-3 py-1.5 text-xs rounded-md text-white" style={{background:"var(--accent)"}}>+ Register</button>
      </div>
      <div className="text-xs p-3 rounded" style={{background:"var(--accent-dim)",color:"var(--accent)"}}>External Executors are execution workers, not free agents. Every action governed by MCL, RiskGate and ADR-A.</div>
      {showReg && (
        <div className="card space-y-2">
          <input placeholder="Display Name" value={regForm.display_name||""} onChange={e=>setRegForm({...regForm,display_name:e.target.value})} className="input" />
          <select value={regForm.source_type||"OpenClaw"} onChange={e=>setRegForm({...regForm,source_type:e.target.value})} className="input">{SOURCE_TYPES.map(s=><option key={s}>{s}</option>)}</select>
          <input placeholder="Risk Ceiling" value={regForm.risk_ceiling||""} onChange={e=>setRegForm({...regForm,risk_ceiling:e.target.value})} className="input" />
          <div className="flex gap-2"><button onClick={reg} className="px-3 py-1.5 text-xs rounded-md text-white" style={{background:"var(--accent)"}}>Register</button><button onClick={()=>setShowReg(false)} className="px-3 py-1.5 text-xs rounded-md" style={{color:"var(--text-muted)"}}>Cancel</button></div>
        </div>
      )}
      {loading && <div className="text-xs" style={{color:"var(--text-muted)"}}>Loading...</div>}
      <div className="grid grid-cols-2 gap-2">
        {execs.map((e:Record<string,unknown>,i:number)=>(
          <div key={i} className="card">
            <div className="flex justify-between mb-1"><span className="text-sm font-semibold">{e.display_name as string}</span><span className="text-xs px-1.5 py-0.5 rounded" style={{background:e.status==="Registered"?"var(--green-dim)":"var(--yellow-dim)",color:e.status==="Registered"?"var(--green)":"var(--yellow)"}}>{e.status as string}</span></div>
            <div className="text-xs space-y-0.5" style={{color:"var(--text-muted)"}}>
              <div>Type: {e.source_type as string} | Sandbox: {e.sandbox_level as string} | Risk: {String(e.risk_ceiling)}</div>
              <div className="font-mono text-xs" style={{color:"var(--accent)"}}>{e.executor_id as string}</div>
              <div className="flex gap-2 mt-2 flex-wrap items-center">
                <button onClick={()=>executorHealth(e.executor_id as string)} className="text-xs px-2 py-1 rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>Health</button>
                <button onClick={()=>disableExecutor(e.executor_id as string).then(load)} className="text-xs px-2 py-1 rounded border" style={{borderColor:"var(--red)",color:"var(--red)"}}>Disable</button>
                <select value={dryRunId} onChange={ev=>setDryRunId(ev.target.value)} className="text-xs px-1 py-1 rounded border" style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}>
                  <option value="">Select WO</option>
                  {workOrders.map((w:Record<string,unknown>,j:number)=><option key={j} value={w.work_order_id as string}>{(w.mission_intent as string||"").slice(0,30)}</option>)}
                </select>
                <button onClick={()=>dryRun(e.executor_id as string,dryRunId)} disabled={!dryRunId} className="text-xs px-2 py-1 rounded border" style={{borderColor:"var(--yellow)",color:"var(--yellow)"}}>Dry-Run</button>
              </div>
            </div>
          </div>
        ))}
      </div>
      {dryRunResult && <div className="card"><pre className="text-xs" style={{color:"var(--text-secondary)"}}>{dryRunResult}</pre></div>}
    </div>
  );
}
