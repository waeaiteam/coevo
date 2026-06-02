export default function RiskDashboard() {
  return (
    <div className="card text-sm space-y-3">
      <p style={{ color: "var(--text-secondary)" }}>Risk evaluation is triggered automatically during Yellow and Red track execution.</p>
      <div className="grid grid-cols-2 gap-3">
        <div className="p-3 rounded" style={{ background: "var(--surface-raised)" }}>
          <div className="text-xs" style={{ color: "var(--text-muted)" }}>Decision Tree</div>
          <div className="text-sm font-mono mt-1">OPA → Veto → Confidence</div>
        </div>
        <div className="p-3 rounded" style={{ background: "var(--surface-raised)" }}>
          <div className="text-xs" style={{ color: "var(--text-muted)" }}>ActionRisk Formula</div>
          <div className="text-sm font-mono mt-1">w1·BR + w2·IR + w3·ES + w4·RV</div>
        </div>
      </div>
    </div>
  );
}
