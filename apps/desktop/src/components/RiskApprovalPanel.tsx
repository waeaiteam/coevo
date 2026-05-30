export default function RiskApprovalPanel() {
  return (
    <div className="card">
      <div className="text-xs font-semibold uppercase tracking-widest mb-3" style={{ color: "var(--text-muted)" }}>
        Risk &amp; Approval
      </div>
      <div className="text-xs mb-3" style={{ color: "var(--text-muted)" }}>
        Static Alpha boundary summary
      </div>
      <div className="space-y-3">
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>Pending Human Approval</span>
          <span className="font-mono" style={{ color: "var(--text-secondary)" }}>
            -
          </span>
        </div>
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>Emergency Lease Active</span>
          <span className="font-mono" style={{ color: "var(--text-secondary)" }}>
            No
          </span>
        </div>
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>Latest Denied Action</span>
          <span className="font-mono truncate ml-2 max-w-32 text-right" style={{ color: "var(--red)" }}>
            Red Track hard block
          </span>
        </div>
        <div className="flex justify-between text-xs pt-2 border-t" style={{ borderColor: "var(--border-subtle)" }}>
          <span style={{ color: "var(--text-muted)" }}>Policy Version</span>
          <span className="font-mono" style={{ color: "var(--accent)" }}>Alpha</span>
        </div>
      </div>
    </div>
  );
}
