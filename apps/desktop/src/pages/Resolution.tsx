export default function Resolution() {
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg" style={{ color: "#ec4899" }}>⚖</span>
        <h2 className="text-lg font-bold tracking-tight" style={{ color: "var(--text-primary)" }}>Resolution Engine</h2>
      </div>
      <div className="card">
        <div className="text-xs space-y-3">
          <div className="text-sm font-semibold" style={{ color: "#ec4899" }}>ADR-A Records</div>
          <div className="grid grid-cols-2 gap-2">
            {["decision_id", "mcl_reference", "proposer_agent", "critic_objections", "rejected_alternatives", "responsibility_anchor", "risk_accepted"].map((f) => (
              <div key={f} className="p-2 rounded font-mono text-xs" style={{ background: "var(--bg-primary)", color: "var(--text-muted)", border: "1px solid var(--border-subtle)" }}>
                {f}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
