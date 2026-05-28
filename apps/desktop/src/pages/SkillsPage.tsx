import { useState, useEffect } from "react";
import { listSkills, seedSkills, listSkillProposals, verifySkillProposal, approveSkillProposal, rejectSkillProposal, rollbackSkill } from "../api/client";

export default function SkillsPage() {
  const [skills, setSkills] = useState<Record<string,unknown>[]>([]);
  const [proposals, setProposals] = useState<Record<string,unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [result, setResult] = useState("");

  async function load() { setLoading(true); try { setSkills(await listSkills()||[]); setProposals(await listSkillProposals()||[]); } catch { setSkills([]); setProposals([]); } setLoading(false); }
  useEffect(()=>{load();},[]);

  async function action(fn:()=>Promise<unknown>,label:string) { setResult(""); try { const r = await fn(); setResult(`${label}: ${JSON.stringify(r)}`); load(); } catch(e:unknown) { setResult(`Error: ${e instanceof Error?e.message:String(e)}`); } }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3"><span className="text-lg">⚡</span><h2 className="text-lg font-bold">Skills</h2></div>
        <button onClick={()=>action(()=>seedSkills(),"Seed")} className="px-3 py-1.5 text-xs rounded-md text-white" style={{background:"var(--accent)"}}>Seed Skills</button>
      </div>
      <div className="text-xs p-3 rounded" style={{background:"var(--accent-dim)",color:"var(--accent)"}}>Skills are versioned, testable, rollback-capable. Cannot auto-escalate permissions.</div>
      {result && <div className="card"><pre className="text-xs" style={{color:"var(--text-secondary)"}}>{result}</pre></div>}
      {loading && <div className="text-xs" style={{color:"var(--text-muted)"}}>Loading...</div>}

      <h3 className="text-sm font-semibold">Skill Packages</h3>
      <div className="grid grid-cols-2 gap-2">
        {skills.map((s:Record<string,unknown>,i:number)=>(
          <div key={i} className="card">
            <div className="flex justify-between mb-1"><span className="text-sm font-semibold">{s.name as string}</span><span className="text-xs px-1.5 py-0.5 rounded" style={{background:s.status==="Active"?"var(--green-dim)":"var(--yellow-dim)",color:s.status==="Active"?"var(--green)":"var(--yellow)"}}>{s.status as string}</span></div>
            <div className="text-xs space-y-0.5" style={{color:"var(--text-muted)"}}>
              <div>v{s.version as string} | {s.department as string} | owner: {s.owner_agent_id as string} | risk: {String(s.risk_ceiling)}</div>
              <div className="flex gap-2 mt-2">
                <button onClick={()=>action(()=>rollbackSkill(s.skill_id as string,s.version as string),"Rollback")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--red)",color:"var(--red)"}}>Rollback</button>
              </div>
            </div>
          </div>
        ))}
      </div>

      <h3 className="text-sm font-semibold">Evolution Proposals</h3>
      <div className="grid grid-cols-1 gap-2">
        {proposals.map((p:Record<string,unknown>,i:number)=>(
          <div key={i} className="card">
            <div className="flex justify-between mb-1"><span className="text-sm font-semibold">{p.proposal_id as string}</span><span className="text-xs px-1.5 py-0.5 rounded" style={{background:p.status==="Applied"?"var(--green-dim)":"var(--yellow-dim)",color:p.status==="Applied"?"var(--green)":"var(--yellow)"}}>{p.status as string}</span></div>
            <div className="text-xs space-y-0.5" style={{color:"var(--text-muted)"}}>
              <div>Target: {p.target_skill_id as string} | Type: {p.proposal_type as string} | Risk: {p.risk_assessment as string}</div>
              <div>Diagnosis: {p.diagnosis as string}</div>
              <div className="flex gap-2 mt-2">
                <button onClick={()=>action(()=>verifySkillProposal(p.proposal_id as string),"Verify")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>Verify</button>
                <button onClick={()=>action(()=>approveSkillProposal(p.proposal_id as string),"Approve")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--green)",color:"var(--green)"}}>Approve</button>
                <button onClick={()=>action(()=>rejectSkillProposal(p.proposal_id as string),"Reject")} className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--red)",color:"var(--red)"}}>Reject</button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
