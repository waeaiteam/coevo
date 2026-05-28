export default function RiskApprovalPanel() {
  const mock = {
    pendingApprovals: 2,
    activeLeases: 0,
    lastDenied: "urn:coevo:action:production:write — 12m ago",
    policyVersion: "a3f2...c91b",
  };

  return (
    <div className="card">
      <div className="text-xs font-semibold uppercase tracking-widest mb-3" style={{ color: "var(--text-muted)" }}>
        Risk &amp; Approval
      </div>
      <div className="space-y-3">
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>Pending Human Approval</span>
          <span className="font-mono" style={{ color: mock.pendingApprovals > 0 ? "var(--yellow)" : "var(--text-secondary)" }}>
            {mock.pendingApprovals}
          </span>
        </div>
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>Emergency Lease Active</span>
          <span className="font-mono" style={{ color: mock.activeLeases > 0 ? "var(--red)" : "var(--text-secondary)" }}>
            {mock.activeLeases}
          </span>
        </div>
        <div className="flex justify-between text-xs">
          <span style={{ color: "var(--text-muted)" }}>Latest Denied Action</span>
          <span className="font-mono truncate ml-2 max-w-32 text-right" style={{ color: "var(--red)" }}>
            {mock.lastDenied}
          </span>
        </div>
        <div className="flex justify-between text-xs pt-2 border-t" style={{ borderColor: "var(--border-subtle)" }}>
          <span style={{ color: "var(--text-muted)" }}>Policy Version</span>
          <span className="font-mono" style={{ color: "var(--accent)" }}>{mock.policyVersion}</span>
        </div>
      </div>
    </div>
  );
}
