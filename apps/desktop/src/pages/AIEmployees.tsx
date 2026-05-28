export default function AIEmployees() {
  const depts = ["FounderOffice","Product","Engineering","Research","Governance","SRE","Growth","Finance"];
  const agents = [
    {id:"agent-founder-01",name:"Founder Assistant",dept:"FounderOffice",risk:0.3,status:"Active",layers:"Hypothesis, Suggestion",skills:["mission-drafting"]},
    {id:"agent-pm-01",name:"Product Manager",dept:"Product",risk:0.3,status:"Active",layers:"Suggestion",skills:["requirement-analysis"]},
    {id:"agent-engineer-01",name:"Engineer",dept:"Engineering",risk:0.4,status:"Active",layers:"Hypothesis, Suggestion",skills:["code-analysis"]},
    {id:"agent-critic-01",name:"Critic",dept:"Governance",risk:0.5,status:"Active",layers:"Suggestion",skills:["critique","blocker-detection"]},
    {id:"agent-risk-01",name:"Risk & Compliance",dept:"Governance",risk:0.5,status:"Active",layers:"Suggestion",skills:["policy-check","risk-review"]},
    {id:"agent-sre-01",name:"SRE Diagnostic",dept:"SRE",risk:0.4,status:"Active",layers:"Hypothesis, Suggestion",skills:["log-analysis"]},
    {id:"agent-synth-01",name:"Synthesizer",dept:"FounderOffice",risk:0.3,status:"Active",layers:"Suggestion",skills:["report-generation"]},
    {id:"agent-research-01",name:"Research Agent",dept:"Research",risk:0.4,status:"Active",layers:"Hypothesis, Suggestion",skills:["paper-search"]},
    {id:"agent-growth-01",name:"Growth Agent",dept:"Growth",risk:0.3,status:"Active",layers:"Draft",skills:["content-generation"]},
    {id:"agent-finance-01",name:"Finance Agent",dept:"Finance",risk:0.4,status:"Active",layers:"Suggestion",skills:["budget-analysis"]},
  ];
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3"><span className="text-lg">👥</span><h2 className="text-lg font-bold">AI Employees</h2></div>
      {depts.map((d)=>(
        <div key={d}>
          <div className="text-xs font-semibold mb-2 uppercase tracking-wider" style={{color:"var(--text-muted)"}}>{d}</div>
          <div className="grid grid-cols-2 gap-2">
            {agents.filter((a)=>a.dept===d).map((a)=>(
              <div key={a.id} className="card">
                <div className="flex justify-between items-start mb-1">
                  <span className="text-sm font-semibold">{a.name}</span>
                  <span className="text-xs px-1.5 py-0.5 rounded" style={{background:"var(--green-dim)",color:"var(--green)"}}>{a.status}</span>
                </div>
                <div className="text-xs space-y-0.5" style={{color:"var(--text-muted)"}}>
                  <div>Risk ceiling: {a.risk}</div>
                  <div>Layers: {a.layers}</div>
                  <div>Skills: {a.skills.join(", ")}</div>
                  <div className="font-mono text-xs" style={{color:"var(--accent)"}}>{a.id}</div>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
