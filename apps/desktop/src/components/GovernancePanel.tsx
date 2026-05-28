import { useGovernance } from "../hooks/useGovernance";

export default function GovernancePanel() {
  const { state } = useGovernance();
  const trackColor = state?.track === "green" ? "var(--green)" : state?.track === "yellow" ? "var(--yellow)" : "var(--red)";

  return (
    <aside className="w-72 border-l overflow-y-auto p-4 space-y-4" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
      <div className="text-xs font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>
        Governance Status
      </div>

      {!state ? (
        <div className="text-xs py-8 text-center" style={{ color: "var(--text-muted)" }}>
          Send a mission to populate governance status
        </div>
      ) : (
        <>
          <div className="card">
            <div className="metric-label">Track</div>
            <span className={`track-${state.track} mt-1`}>{state.track.toUpperCase()} TRACK</span>
          </div>
          <div className="card space-y-2">
            <div><div className="metric-label">Contract Hash</div><div className="text-xs font-mono mt-0.5 truncate" style={{ color: "var(--text-secondary)" }}>{state.contractHash}</div></div>
            <div><div className="metric-label">Plan Hash</div><div className="text-xs font-mono mt-0.5 truncate" style={{ color: "var(--text-secondary)" }}>{state.planHash}</div></div>
          </div>
          <div className="card">
            <div className="metric-label">Selected Agents</div>
            {state.agents.map((a) => <div key={a} className="text-xs font-mono mt-1" style={{ color: "var(--accent)" }}>{a}</div>)}
          </div>
          <div className="card">
            <div className="metric-label">RiskGate Decision</div>
            <div className="text-xs font-semibold mt-1" style={{ color: trackColor }}>{state.riskDecision}</div>
          </div>
          <div className="card">
            <div className="metric-label">Approval Required</div>
            <div className="text-xs font-semibold mt-1" style={{ color: state.approvalRequired ? "var(--yellow)" : "var(--green)" }}>
              {state.approvalRequired ? "YES — pending human approval" : "No"}
            </div>
          </div>
          <div className="card">
            <div className="metric-label">Audit Trace</div>
            <div className="text-xs font-mono mt-1 truncate" style={{ color: "var(--text-muted)" }}>{state.traceparent}</div>
          </div>
          <div className="p-3 rounded-lg text-xs leading-relaxed" style={{ background: "var(--accent-dim)", color: "var(--accent)" }}>
            coevo never creates unconstrained agents. Task Agent Instances are short-lived. Ephemeral Sub-Agents: Hypothesis/Suggestion only.
          </div>
        </>
      )}

      <div className="card">
        <div className="metric-label mb-2">Governance Timeline</div>
        {[
          "User intent received",
          "MCL compiled",
          "Agent Registry queried",
          "Execution plan generated",
          "CognitiveCustoms proposed",
          "RiskGate evaluated",
          "ADR-A archived",
        ].map((step, i, arr) => (
          <div key={step} className="flex gap-2">
            <div className="flex flex-col items-center pt-0.5">
              <div className="w-2 h-2 rounded-full flex-shrink-0" style={{ background: state ? "var(--accent)" : "var(--border-accent)" }} />
              {i < arr.length - 1 && <div className="w-px flex-1 my-0.5" style={{ background: "var(--border-subtle)" }} />}
            </div>
            <div className="text-xs pb-2" style={{ color: state ? "var(--text-primary)" : "var(--text-muted)" }}>{step}</div>
          </div>
        ))}
      </div>
    </aside>
  );
}
