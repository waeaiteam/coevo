export default function RiskDashboard() {
  return (
    <div className="bg-white border rounded p-4 text-sm space-y-3">
      <p className="text-gray-500">Risk evaluation is triggered automatically during Yellow and Red track execution.</p>
      <div className="grid grid-cols-2 gap-3">
        <div className="p-3 bg-gray-50 rounded">
          <div className="text-xs text-gray-400">Decision Tree</div>
          <div className="text-sm font-mono mt-1">OPA → Veto → Confidence</div>
        </div>
        <div className="p-3 bg-gray-50 rounded">
          <div className="text-xs text-gray-400">ActionRisk Formula</div>
          <div className="text-sm font-mono mt-1">w1·BR + w2·IR + w3·ES + w4·RV</div>
        </div>
      </div>
    </div>
  );
}
