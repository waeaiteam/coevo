import { ContractResponse } from "../types";

export default function ContractViewer({ result }: { result: ContractResponse }) {
  const c = result.contract as Record<string, unknown>;

  return (
    <div className="card space-y-3">
      <div className="flex justify-between items-center">
        <h3 className="font-bold">Compiled Contract</h3>
        <span
          className="text-xs px-2 py-0.5 rounded"
          style={
            result.ambiguity_score > 0.5
              ? { background: "var(--yellow-dim)", color: "var(--yellow)" }
              : { background: "var(--green-dim)", color: "var(--green)" }
          }
        >
          Ambiguity: {result.ambiguity_score.toFixed(2)}
        </span>
      </div>
      <div className="text-xs font-mono break-all" style={{ color: "var(--text-secondary)" }}>
        Hash: {result.contract_hash}
      </div>
      <div className="grid grid-cols-2 gap-2 text-xs">
        <Field label="Version" value={c.mcl_version as string} />
        <Field label="State" value={c.mcl_state as string} />
        <Field label="Institution Policy" value={(c.institution_policy_hash as string)?.slice(0, 16) + "..."} />
        <Field label="Risk Score" value={String((c.risk_tolerance_profile as Record<string, unknown>)?.max_risk_score)} />
      </div>
      {result.compile_warnings.length > 0 && (
        <div className="border rounded p-2 text-xs" style={{ background: "var(--yellow-dim)", borderColor: "var(--yellow)", color: "var(--yellow)" }}>
          Warnings: {result.compile_warnings.join(", ")}
        </div>
      )}
      <details className="text-xs">
        <summary className="cursor-pointer" style={{ color: "var(--text-secondary)" }}>Full JSON</summary>
        <pre className="mt-2 p-2 rounded overflow-x-auto" style={{ background: "var(--surface-raised)" }}>
          {JSON.stringify(c, null, 2)}
        </pre>
      </details>
    </div>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span style={{ color: "var(--text-muted)" }}>{label}:</span>{" "}
      <span className="font-mono">{value || "—"}</span>
    </div>
  );
}
