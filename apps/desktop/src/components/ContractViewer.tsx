import { ContractResponse } from "../types";

export default function ContractViewer({ result }: { result: ContractResponse }) {
  const c = result.contract as Record<string, unknown>;

  return (
    <div className="bg-white rounded border p-4 space-y-3">
      <div className="flex justify-between items-center">
        <h3 className="font-bold">Compiled Contract</h3>
        <span className={`text-xs px-2 py-0.5 rounded ${result.ambiguity_score > 0.5 ? "bg-yellow-200" : "bg-green-200"}`}>
          Ambiguity: {result.ambiguity_score.toFixed(2)}
        </span>
      </div>
      <div className="text-xs font-mono text-gray-500 break-all">
        Hash: {result.contract_hash}
      </div>
      <div className="grid grid-cols-2 gap-2 text-xs">
        <Field label="Version" value={c.mcl_version as string} />
        <Field label="State" value={c.mcl_state as string} />
        <Field label="Institution Policy" value={(c.institution_policy_hash as string)?.slice(0, 16) + "..."} />
        <Field label="Risk Score" value={String((c.risk_tolerance_profile as Record<string, unknown>)?.max_risk_score)} />
      </div>
      {result.compile_warnings.length > 0 && (
        <div className="bg-yellow-50 border border-yellow-200 rounded p-2 text-xs text-yellow-700">
          Warnings: {result.compile_warnings.join(", ")}
        </div>
      )}
      <details className="text-xs">
        <summary className="cursor-pointer text-gray-500">Full JSON</summary>
        <pre className="mt-2 bg-gray-100 p-2 rounded overflow-x-auto">
          {JSON.stringify(c, null, 2)}
        </pre>
      </details>
    </div>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span className="text-gray-400">{label}:</span>{" "}
      <span className="font-mono">{value || "—"}</span>
    </div>
  );
}
