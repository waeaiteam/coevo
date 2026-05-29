export default function SkillEvolutionCTA({ proposalId }: { proposalId?: unknown }) {
  if (!proposalId) return null;
  return (
    <div className="card space-y-1">
      <div className="text-sm font-semibold">Skill Evolution Proposal Created</div>
      <div className="text-xs" style={{color:"var(--text-muted)"}}>proposal_id: <span className="font-mono">{String(proposalId)}</span></div>
      <div className="text-xs" style={{color:"var(--text-muted)"}}>Review, verify, and approve in the Skills page. Approval changes future worker behavior but grants no new permissions.</div>
    </div>
  );
}
