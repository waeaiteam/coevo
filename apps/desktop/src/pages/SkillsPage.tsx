export default function Skills() {
  const skills = [
    {id:"skill-mission-draft",name:"Mission Drafting",version:"1.2.0",owner:"agent-founder-01",risk:0.3,status:"Active"},
    {id:"skill-code-analysis",name:"Code Analysis",version:"1.0.0",owner:"agent-engineer-01",risk:0.4,status:"Active"},
    {id:"skill-critique",name:"Critique",version:"1.1.0",owner:"agent-critic-01",risk:0.5,status:"Active"},
    {id:"skill-policy-check",name:"Policy Check",version:"1.0.0",owner:"agent-risk-01",risk:0.5,status:"Active"},
    {id:"skill-log-analysis",name:"Log Analysis",version:"1.0.0",owner:"agent-sre-01",risk:0.4,status:"Active"},
    {id:"skill-report-gen",name:"Report Generation",version:"1.0.0",owner:"agent-synth-01",risk:0.3,status:"Active"},
  ];
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3"><span className="text-lg">⚡</span><h2 className="text-lg font-bold">Skills</h2></div>
      <div className="text-xs mb-2 p-3 rounded" style={{background:"var(--accent-dim)",color:"var(--accent)"}}>
        Skills are versioned, testable, verifiable, rollback-capable. They cannot auto-escalate permissions. All skill evolution governed by MCL/RiskGate/ADR-A.
      </div>
      <div className="grid grid-cols-2 gap-2">
        {skills.map((s)=>(
          <div key={s.id} className="card">
            <div className="flex justify-between mb-1"><span className="text-sm font-semibold">{s.name}</span><span className="text-xs px-1.5 py-0.5 rounded" style={{background:s.status==="Active"?"var(--green-dim)":"var(--yellow-dim)",color:s.status==="Active"?"var(--green)":"var(--yellow)"}}>{s.status}</span></div>
            <div className="text-xs space-y-0.5" style={{color:"var(--text-muted)"}}>
              <div>v{s.version} | Risk: {s.risk} | Owner: {s.owner}</div>
              <div className="flex gap-2 mt-2">
                <button className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>Approve</button>
                <button className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--red)",color:"var(--red)"}}>Rollback</button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
