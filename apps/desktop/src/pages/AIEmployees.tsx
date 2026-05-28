import { useState, useEffect } from "react";
import { listEmployees, seedEmployees } from "../api/client";

const DEPTS = ["FounderOffice","Product","Engineering","Research","Governance","SRE","Growth","Finance"];
export default function AIEmployees() {
  const [emps, setEmps] = useState<Record<string,unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [seeding, setSeeding] = useState(false);
  const [seedResult, setSeedResult] = useState("");

  async function load() { setLoading(true); try { const e = await listEmployees(); setEmps(e||[]); } catch { setEmps([]); } setLoading(false); }
  useEffect(()=>{load();},[]);

  async function seed() {
    setSeeding(true); setSeedResult("");
    try { const r = await seedEmployees() as Record<string,unknown>; setSeedResult(`Inserted: ${r.inserted}, Total: ${r.total}`); load(); }
    catch(e:unknown) { setSeedResult("Error: "+(e instanceof Error?e.message:String(e))); }
    setSeeding(false);
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3"><span className="text-lg">👥</span><h2 className="text-lg font-bold">AI Employees</h2></div>
        <div className="flex items-center gap-2">
          {seedResult && <span className="text-xs" style={{color:"var(--green)"}}>{seedResult}</span>}
          <button onClick={seed} disabled={seeding} className="px-3 py-1.5 text-xs rounded-md text-white" style={{background:"var(--accent)"}}>
            {seeding?"Seeding...":emps.length===0?"Seed 10 AI Employees":"Re-seed"}
          </button>
        </div>
      </div>
      <div className="text-xs p-3 rounded" style={{background:"var(--accent-dim)",color:"var(--accent)"}}>
        AI Employees are governed workers, not unconstrained sub-agents.
      </div>
      {loading && <div className="text-xs" style={{color:"var(--text-muted)"}}>Loading...</div>}
      {!loading && emps.length===0 && <div className="text-xs" style={{color:"var(--text-muted)"}}>No AI Employees. Click "Seed 10 AI Employees" to initialize.</div>}
      {DEPTS.map(d=>{
        const deptEmps = emps.filter(e=>e.department===d);
        if (deptEmps.length===0) return null;
        return (<div key={d}>
          <div className="text-xs font-semibold mb-2 uppercase tracking-wider" style={{color:"var(--text-muted)"}}>{d}</div>
          <div className="grid grid-cols-2 gap-2">
            {deptEmps.map((e:Record<string,unknown>,i:number)=>(
              <div key={i} className="card">
                <div className="flex justify-between items-start mb-1">
                  <span className="text-sm font-semibold">{e.display_name as string}</span>
                  <span className="text-xs px-1.5 py-0.5 rounded" style={{background:e.lifecycle_status==="Active"?"var(--green-dim)":"var(--yellow-dim)",color:e.lifecycle_status==="Active"?"var(--green)":"var(--yellow)"}}>{e.lifecycle_status as string}</span>
                </div>
                <div className="text-xs space-y-0.5" style={{color:"var(--text-muted)"}}>
                  <div>Role: {e.role as string}</div>
                  <div>Risk ceiling: {String(e.risk_ceiling)}</div>
                  <div>Layers: {String(e.allowed_cognitive_layers)}</div>
                  <div className="font-mono text-xs" style={{color:"var(--accent)"}}>{e.agent_id as string}</div>
                </div>
              </div>
            ))}
          </div>
        </div>);
      })}
    </div>
  );
}
