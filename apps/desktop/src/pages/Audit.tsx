export default function Audit() {
  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg" style={{ color: "var(--accent)" }}>☰</span>
        <h2 className="text-lg font-bold">Audit Log</h2>
      </div>
      <div className="card">
        <div className="text-sm py-6 text-center" style={{ color: "var(--text-muted)" }}>
          Structured audit events are logged to SQLite and accessible via the audit repository.
        </div>
        <div className="grid grid-cols-3 gap-2 text-xs">
          {["contract.compiled", "plan.created", "fact.proposed", "fact.promoted", "risk.evaluated", "lease.granted", "adr.generated", "human.overridden", "contract.closed"].map((e) => (
            <div key={e} className="p-2 rounded font-mono" style={{ background: "var(--bg-secondary)", color: "var(--text-muted)", border: "1px solid var(--border-subtle)" }}>
              {e}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
