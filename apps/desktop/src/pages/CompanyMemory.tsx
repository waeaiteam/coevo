export default function CompanyMemory() {
  const memories = [
    {id:"1",scope:"Company",title:"OPC Mission",content:"Build AI governance for one-person companies",confidence:0.95,status:"Active",layer:"Fact"},
    {id:"2",scope:"Task",title:"Last Green Track result",content:"System health analysis completed in 43ms",confidence:0.8,status:"Active",layer:"Hypothesis"},
    {id:"3",scope:"Agent",title:"Critic objection pattern",content:"Frequent overconfidence in deployment recommendations",confidence:0.6,status:"Active",layer:"Suggestion"},
    {id:"4",scope:"Audit",title:"ADR-A archived",content:"Production write lease granted for emergency fix",confidence:1.0,status:"Active",layer:"Decision"},
  ];
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3"><span className="text-lg">🧠</span><h2 className="text-lg font-bold">Company Memory</h2></div>
      <div className="space-y-3">
        {memories.map((m)=>(
          <div key={m.id} className="card">
            <div className="flex items-center gap-2 mb-1">
              <span className="text-xs px-1.5 py-0.5 rounded" style={{background:"var(--bg-secondary)",color:"var(--accent)"}}>{m.scope}</span>
              <span className={`text-xs px-1.5 py-0.5 rounded ${m.layer==="Fact"?"track-green":m.layer==="Decision"?"track-yellow":"track-green"}`}>{m.layer}</span>
              <span className="text-xs" style={{color:"var(--text-muted)"}}>{m.status} | confidence: {m.confidence}</span>
            </div>
            <div className="text-sm font-semibold">{m.title}</div>
            <div className="text-xs mt-1" style={{color:"var(--text-secondary)"}}>{m.content}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
