export default function ResolutionPanel() {
  return (
    <div className="bg-white border rounded p-4 text-sm space-y-3">
      <p className="text-gray-500">Process stance matrices and generate ADR-A records.</p>
      <div className="p-3 bg-gray-50 rounded">
        <div className="text-xs text-gray-400">ADR-A Fields</div>
        <div className="text-sm font-mono mt-1">
          decision_id, mcl_reference, proposer_agent, critic_objections, rejected_alternatives, responsibility_anchor
        </div>
      </div>
    </div>
  );
}
