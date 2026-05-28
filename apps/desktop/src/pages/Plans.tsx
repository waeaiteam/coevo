export default function Plans() {
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg" style={{ color: "var(--blue)" }}>↗</span>
        <h2 className="text-lg font-bold tracking-tight" style={{ color: "var(--text-primary)" }}>Execution Plans</h2>
      </div>
      <div className="card">
        <div className="text-sm py-6 text-center" style={{ color: "var(--text-muted)" }}>
          Route a compiled contract to generate execution plans
        </div>
      </div>
    </div>
  );
}
