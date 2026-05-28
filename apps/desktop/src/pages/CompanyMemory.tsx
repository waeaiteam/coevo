import { useState, useEffect } from "react";
import { listMemory, createMemory, markMemoryStale, revokeMemory } from "../api/client";

const SCOPES = ["User","Company","Agent","Task","Skill","Executor","Audit"];
export default function CompanyMemory() {
  const [memories, setMemories] = useState<Record<string,unknown>[]>([]);
  const [scope, setScope] = useState("");
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newContent, setNewContent] = useState("");
  const [newScope, setNewScope] = useState("Company");
  const [error, setError] = useState("");

  async function load(s?:string) {
    setLoading(true);
    try { const m = await listMemory(s?{scope:s}:undefined); setMemories(m as Record<string,unknown>[]||[]); } catch { setMemories([]); }
    setLoading(false);
  }
  useEffect(()=>{load();},[]);

  async function create() {
    if (!newTitle.trim()) return; setError("");
    try {
      await createMemory({memory_id:crypto.randomUUID(),scope:newScope,owner_id:"default-founder",title:newTitle,content:newContent,tags:[],source:"desktop",provenance:"",confidence:0.5,ttl_seconds:86400,created_at_ms:Date.now(),updated_at_ms:Date.now(),access_policy:"",status:"Active",cognitive_layer:"Hypothesis",linked_contract_hash:null,linked_plan_hash:null,linked_adr_id:null});
      setShowCreate(false); setNewTitle(""); setNewContent(""); load(scope||undefined);
    } catch(e:unknown) { setError(e instanceof Error?e.message:String(e)); }
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3"><span className="text-lg">🧠</span><h2 className="text-lg font-bold">Company Memory</h2></div>
        <button onClick={()=>setShowCreate(!showCreate)} className="px-3 py-1.5 text-xs rounded-md text-white" style={{background:"var(--accent)"}}>+ New Memory</button>
      </div>
      <div className="flex gap-2 flex-wrap">
        <button onClick={()=>{setScope("");load();}} className={`px-3 py-1 text-xs rounded-md border ${!scope?"font-bold":""}`} style={{borderColor:"var(--border-accent)",color:!scope?"var(--accent)":"var(--text-secondary)"}}>All</button>
        {SCOPES.map(s=><button key={s} onClick={()=>{setScope(s);load(s);}} className={`px-3 py-1 text-xs rounded-md border ${scope===s?"font-bold":""}`} style={{borderColor:"var(--border-accent)",color:scope===s?"var(--accent)":"var(--text-secondary)"}}>{s}</button>)}
      </div>
      {showCreate && (
        <div className="card space-y-2">
          <select value={newScope} onChange={e=>setNewScope(e.target.value)} className="input"><option>Company</option><option>Task</option><option>Agent</option><option>User</option></select>
          <input placeholder="Title" value={newTitle} onChange={e=>setNewTitle(e.target.value)} className="input" />
          <textarea placeholder="Content" value={newContent} onChange={e=>setNewContent(e.target.value)} className="input" rows={3} />
          <div className="flex gap-2">
            <button onClick={create} className="px-3 py-1.5 text-xs rounded-md text-white" style={{background:"var(--accent)"}}>Create</button>
            <button onClick={()=>setShowCreate(false)} className="px-3 py-1.5 text-xs rounded-md" style={{color:"var(--text-muted)"}}>Cancel</button>
          </div>
          {error && <div className="text-xs" style={{color:"var(--red)"}}>{error}</div>}
        </div>
      )}
      {loading && <div className="text-xs" style={{color:"var(--text-muted)"}}>Loading...</div>}
      <div className="space-y-2">
        {memories.map((m:Record<string,unknown>,i:number)=>(
          <div key={i} className="card">
            <div className="flex items-center gap-2 mb-1">
              <span className="text-xs px-1.5 py-0.5 rounded" style={{background:"var(--bg-secondary)",color:"var(--accent)"}}>{m.scope as string}</span>
              <span className="text-xs px-1.5 py-0.5 rounded" style={{background:m.cognitive_layer==="Fact"?"var(--green-dim)":"var(--bg-secondary)",color:m.cognitive_layer==="Fact"?"var(--green)":"var(--text-muted)"}}>{m.cognitive_layer as string}</span>
              <span className="text-xs" style={{color:m.status==="Active"?"var(--green)":"var(--text-muted)"}}>{m.status as string}</span>
              <span className="text-xs" style={{color:"var(--text-muted)"}}>confidence: {String(m.confidence)}</span>
            </div>
            <div className="text-sm font-semibold">{m.title as string}</div>
            <div className="text-xs mt-1" style={{color:"var(--text-secondary)"}}>{m.content as string}</div>
            <div className="flex gap-2 mt-2">
              <button onClick={async()=>{await markMemoryStale(m.memory_id as string);load(scope||undefined);}} className="text-xs" style={{color:"var(--yellow)"}}>Stale</button>
              <button onClick={async()=>{await revokeMemory(m.memory_id as string);load(scope||undefined);}} className="text-xs" style={{color:"var(--red)"}}>Revoke</button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
