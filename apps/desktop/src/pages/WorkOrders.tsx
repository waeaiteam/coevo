export default function WorkOrders() {
  const orders = [
    {id:"wo-001",intent:"Analyze system health",track:"green",status:"Completed",contract:"e9fef199...",agents:["agent-synth-01"],executors:[],skills:["log-analysis"]},
    {id:"wo-002",intent:"Send staging notification",track:"yellow",status:"WaitingApproval",contract:"a1b2c3...",agents:["agent-pm-01","agent-critic-01"],executors:[],skills:["report-generation"]},
    {id:"wo-003",intent:"Emergency production fix",track:"red",status:"Running",contract:"d4e5f6...",agents:["agent-sre-01","agent-risk-01"],executors:["exec-hermes-01"],skills:["log-analysis","policy-check"]},
  ];
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3"><span className="text-lg">📋</span><h2 className="text-lg font-bold">Work Orders</h2></div>
      <div className="space-y-2">
        {orders.map((o)=>(
          <div key={o.id} className="card">
            <div className="flex items-center justify-between mb-1">
              <div className="flex items-center gap-2">
                <span className="text-sm font-semibold">{o.intent}</span>
                <span className={`track-${o.track}`}>{o.track}</span>
                <span className="text-xs px-1.5 py-0.5 rounded" style={{background:o.status==="Completed"?"var(--green-dim)":"var(--yellow-dim)",color:o.status==="Completed"?"var(--green)":"var(--yellow)"}}>{o.status}</span>
              </div>
            </div>
            <div className="text-xs space-y-0.5 mt-2" style={{color:"var(--text-muted)"}}>
              <div>Contract: <span className="font-mono" style={{color:"var(--accent)"}}>{o.contract}</span></div>
              <div>Agents: {o.agents.join(", ")} | Executors: {o.executors.length>0?o.executors.join(", "):"none"} | Skills: {o.skills.join(", ")}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
