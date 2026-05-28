export default function ExternalExecutors() {
  const executors = [
    {id:"exec-hermes-01",name:"Hermes Runtime",type:"Hermes",risk:0.6,status:"Registered",sandbox:"Container"},
    {id:"exec-openclaw-01",name:"OpenClaw Runtime",type:"OpenClaw",risk:0.5,status:"Registered",sandbox:"Process"},
    {id:"exec-mcp-01",name:"MCP Tool Server",type:"MCP",risk:0.4,status:"Registered",sandbox:"None"},
    {id:"exec-302ai-01",name:"302AI Local",type:"302AI",risk:0.6,status:"Draft",sandbox:"VM"},
    {id:"exec-browser-01",name:"Browser Agent",type:"Browser",risk:0.5,status:"Registered",sandbox:"Container"},
    {id:"exec-docker-01",name:"Docker Sandbox",type:"Docker",risk:0.4,status:"Registered",sandbox:"Container"},
  ];
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3"><span className="text-lg">🔌</span><h2 className="text-lg font-bold">External Executors</h2></div>
      <div className="text-xs mb-2 p-3 rounded" style={{background:"var(--accent-dim)",color:"var(--accent)"}}>
        External executors are not free agents. They must be registered, risk-checked, and governed by MCL/RiskGate/ADR-A. Output cannot directly write Fact/Decision.
      </div>
      <div className="grid grid-cols-2 gap-2">
        {executors.map((e)=>(
          <div key={e.id} className="card">
            <div className="flex justify-between mb-1"><span className="text-sm font-semibold">{e.name}</span><span className="text-xs px-1.5 py-0.5 rounded" style={{background:e.status==="Registered"?"var(--green-dim)":"var(--yellow-dim)",color:e.status==="Registered"?"var(--green)":"var(--yellow)"}}>{e.status}</span></div>
            <div className="text-xs space-y-0.5" style={{color:"var(--text-muted)"}}>
              <div>Type: {e.type} | Sandbox: {e.sandbox} | Risk ceiling: {e.risk}</div>
              <div className="font-mono text-xs" style={{color:"var(--accent)"}}>{e.id}</div>
              <div className="flex gap-2 mt-2">
                <button className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>Health Check</button>
                <button className="px-2 py-1 text-xs rounded border" style={{borderColor:"var(--red)",color:"var(--red)"}}>Disable</button>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
