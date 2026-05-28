import { useGovernance } from "../hooks/useGovernance";

export default function GovernancePanel() {
  const { state } = useGovernance();
  const t = state.track;
  const trackColor = t === "green" ? "var(--green)" : t === "yellow" ? "var(--yellow)" : t === "red" ? "var(--red)" : "var(--accent)";
  const isReview = state.phase === "review";
  const isDone = state.phase === "done";
  const hasData = isReview || state.phase === "executing" || isDone;

  return (
    <aside className="w-72 border-l overflow-y-auto p-4 space-y-4" style={{ background: "#fff", borderColor: "var(--border-subtle)" }}>
      <div className="text-xs font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>
        Governance Status
      </div>

      {!hasData ? (
        <div className="text-xs py-8 text-center" style={{ color: "var(--text-muted)" }}>
          Send a mission to populate governance status
        </div>
      ) : (
        <>
          {/* Phase badge */}
          <div className="card">
            <div className="metric-label">Phase</div>
            <span className="text-xs font-semibold mt-1" style={{
              color: isReview ? "var(--accent)" : isDone ? "var(--green)" : "var(--yellow)"
            }}>
              {isReview ? "REVIEW — Awaiting human decision" : state.phase === "executing" ? "EXECUTING..." : "COMPLETED"}
            </span>
          </div>

          {/* Track */}
          <div className="card">
            <div className="metric-label">Track</div>
            <span className={`track-${t || "green"} mt-1`}>{(t || "green").toUpperCase()} TRACK</span>
          </div>

          {/* Hashes */}
          <div className="card space-y-2">
            <div><div className="metric-label">Contract Hash</div><div className="text-xs font-mono mt-0.5 truncate" style={{ color: "var(--text-secondary)" }}>{state.contractHash}</div></div>
            <div><div className="metric-label">Plan Hash</div><div className="text-xs font-mono mt-0.5 truncate" style={{ color: "var(--text-secondary)" }}>{state.planHash}</div></div>
          </div>

          {/* Approval Mode */}
          <div className="card">
            <div className="metric-label">Approval Mode</div>
            <div className="text-xs font-semibold mt-1" style={{ color: state.approvalMode === "EXPLICIT_APPROVAL" ? "var(--yellow)" : "var(--green)" }}>
              {state.approvalMode || "NEGATIVE_CONSENT"}
            </div>
          </div>

          {/* Action Modes */}
          <div className="card">
            <div className="metric-label">Allowed Actions</div>
            {state.actionModes.length > 0 ? state.actionModes.map((a) => (
              <div key={a} className="text-xs font-mono mt-0.5" style={{ color: "var(--accent)" }}>{a}</div>
            )) : <div className="text-xs mt-0.5" style={{ color: "var(--text-muted)" }}>—</div>}
          </div>

          {/* Agents */}
          <div className="card">
            <div className="metric-label">Selected Agents</div>
            {state.agents.map((a) => <div key={a} className="text-xs font-mono mt-1" style={{ color: "var(--accent)" }}>{a}</div>)}
          </div>

          {/* Risk */}
          <div className="card">
            <div className="metric-label">Risk Summary</div>
            <div className="text-xs mt-1" style={{ color: trackColor }}>
              {t === "green" ? "Low risk — auto-execute, no human approval" :
               t === "yellow" ? "Moderate risk — approval window required" :
               t === "red" ? "High risk — emergency lease, MFA required" :
               "—"}
            </div>
          </div>

          {isDone && (
            <div className="card">
              <div className="metric-label">Audit Trace</div>
              <div className="text-xs font-mono mt-1 truncate" style={{ color: "var(--text-muted)" }}>{state.traceparent}</div>
            </div>
          )}

          {/* Governance note */}
          <div className="p-3 rounded-lg text-xs leading-relaxed" style={{ background: "var(--accent-dim)", color: "var(--accent)" }}>
            coevo never creates unconstrained agents. Task Agent Instances are short-lived. Ephemeral Sub-Agents: Hypothesis/Suggestion only.
          </div>
        </>
      )}

      {/* Timeline */}
      <div className="card">
        <div className="metric-label mb-2">Governance Timeline</div>
        {[
          { step: "User intent received", done: hasData },
          { step: "MCL compiled", done: hasData },
          { step: "Agent Registry queried", done: hasData },
          { step: "Execution plan generated", done: hasData },
          { step: "RiskGate classified", done: hasData },
          { step: "CognitiveCustoms proposed", done: isDone },
          { step: "ADR-A archived", done: isDone },
        ].map(({ step, done }, i, arr) => (
          <div key={step} className="flex gap-2">
            <div className="flex flex-col items-center pt-0.5">
              <div className="w-2 h-2 rounded-full flex-shrink-0" style={{ background: done ? "var(--accent)" : "var(--border-accent)" }} />
              {i < arr.length - 1 && <div className="w-px flex-1 my-0.5" style={{ background: "var(--border-subtle)" }} />}
            </div>
            <div className="text-xs pb-2" style={{ color: done ? "var(--text-primary)" : "var(--text-muted)" }}>{step}</div>
          </div>
        ))}
      </div>
    </aside>
  );
}
