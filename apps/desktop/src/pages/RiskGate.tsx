export default function RiskGate() {
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg" style={{ color: "var(--yellow)" }}>⚠</span>
        <h2 className="text-lg font-bold tracking-tight" style={{ color: "var(--text-primary)" }}>Risk Gate</h2>
      </div>
      <div className="card">
        <div className="text-xs space-y-3">
          <div className="flex items-center gap-2 text-sm font-semibold" style={{ color: "var(--yellow)" }}>
            <span className="status-dot" style={{ background: "var(--yellow)" }} /> Rule-First, Number-Second
          </div>
          <div className="grid grid-cols-3 gap-3 mt-3">
            <div className="p-3 rounded" style={{ background: "var(--bg-primary)", border: "1px solid var(--border-subtle)" }}>
              <div className="text-xs mb-1" style={{ color: "var(--text-muted)" }}>Layer 1: OPA Filter</div>
              <div className="text-xs font-mono" style={{ color: "var(--text-secondary)" }}>Policy engine denies → immediate DENY</div>
            </div>
            <div className="p-3 rounded" style={{ background: "var(--bg-primary)", border: "1px solid var(--border-subtle)" }}>
              <div className="text-xs mb-1" style={{ color: "var(--text-muted)" }}>Layer 2: Veto Detection</div>
              <div className="text-xs font-mono" style={{ color: "var(--text-secondary)" }}>Privileged agent oppose → DENY</div>
            </div>
            <div className="p-3 rounded" style={{ background: "var(--bg-primary)", border: "1px solid var(--border-subtle)" }}>
              <div className="text-xs mb-1" style={{ color: "var(--text-muted)" }}>Layer 3: Confidence</div>
              <div className="text-xs font-mono" style={{ color: "var(--text-secondary)" }}>Available ≥ Required → ALLOW</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
