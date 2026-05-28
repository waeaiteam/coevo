import { DemoResponse } from "../types";

export default function TrackMonitor({ result }: { result: DemoResponse }) {
  return (
    <div className="bg-white border rounded p-4 space-y-3 text-sm">
      <div className="flex items-center gap-2">
        <span className={result.track === "green" ? "track-green" : result.track === "yellow" ? "track-yellow" : "track-red"}>
          {result.track.toUpperCase()}
        </span>
        <span className="text-gray-400">completed in {result.elapsed_ms}ms</span>
      </div>
      <div className="grid grid-cols-2 gap-2 text-xs">
        <div><span className="text-gray-400">Contract:</span> <span className="font-mono">{result.contract_hash.slice(0, 16)}...</span></div>
        <div><span className="text-gray-400">Plan:</span> <span className="font-mono">{result.plan_hash.slice(0, 16)}...</span></div>
        <div><span className="text-gray-400">Trace:</span> <span className="font-mono">{result.traceparent.slice(0, 24)}...</span></div>
        <div><span className="text-gray-400">Ambiguity:</span> {result.ambiguity_score != null ? result.ambiguity_score.toFixed(2) : "N/A"}</div>
      </div>
      {result.warnings.length > 0 && (
        <div className="bg-yellow-50 border border-yellow-200 rounded p-2 text-xs text-yellow-700">
          Warnings: {result.warnings.join(", ")}
        </div>
      )}
      <div>
        <div className="text-xs font-medium text-gray-500 mb-1">Blackboard Entries</div>
        {result.entries_created.map((e, i) => (
          <div key={i} className="font-mono text-xs text-gray-600">{e}</div>
        ))}
      </div>
    </div>
  );
}
