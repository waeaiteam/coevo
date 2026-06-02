export default function ResolutionPanel() {
  return (
    <div className="card text-sm space-y-3">
      <p style={{ color: "var(--text-secondary)" }}>Process stance matrices and generate ADR-A records.</p>
      <div className="p-3 rounded" style={{ background: "var(--surface-raised)" }}>
        <div className="text-xs" style={{ color: "var(--text-muted)" }}>ADR-A Fields</div>
        <div className="text-sm font-mono mt-1">
          decision_id, mcl_reference, proposer_agent, critic_objections, rejected_alternatives, responsibility_anchor
        </div>
      </div>
    </div>
  );
}
