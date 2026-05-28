import { useState } from "react";
import { compileContract } from "../api/client";
import type { ContractResponse } from "../types";

export default function Contracts() {
  const [intent, setIntent] = useState("");
  const [result, setResult] = useState<ContractResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleCompile() {
    if (!intent.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const res = await compileContract(intent);
      setResult(res);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg" style={{ color: "var(--accent)" }}>⊡</span>
        <h2 className="text-lg font-bold tracking-tight" style={{ color: "var(--text-primary)" }}>MCL Contracts</h2>
      </div>

      <div className="card">
        <textarea
          className="w-full p-3 rounded-md text-sm font-mono border resize-none focus:outline-none focus:ring-1"
          rows={4}
          placeholder="Enter user intent..."
          value={intent}
          onChange={(e) => setIntent(e.target.value)}
          style={{ background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border-accent)" }}
        />
        <button
          onClick={handleCompile}
          disabled={loading || !intent.trim()}
          className="mt-3 px-4 py-2 text-xs font-semibold rounded-md transition-all duration-150 disabled:opacity-30"
          style={{ background: "var(--accent)", color: "#fff" }}
        >
          {loading ? "Compiling..." : "Compile"}
        </button>
      </div>

      {error && (
        <div className="card" style={{ borderColor: "rgba(239,68,68,0.4)", background: "rgba(239,68,68,0.06)" }}>
          <div className="text-xs" style={{ color: "var(--red)" }}>{error}</div>
        </div>
      )}

      {result && (
        <div className="card">
          <div className="flex justify-between items-center mb-3">
            <span className="text-xs font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>Compiled Contract</span>
            <span className="text-xs px-2 py-0.5 rounded" style={{
              background: result.ambiguity_score > 0.5 ? "rgba(234,179,8,0.15)" : "rgba(34,197,94,0.15)",
              color: result.ambiguity_score > 0.5 ? "var(--yellow)" : "var(--green)",
            }}>
              Ambiguity: {result.ambiguity_score.toFixed(2)}
            </span>
          </div>
          <div className="text-xs font-mono mb-2 break-all" style={{ color: "var(--text-muted)" }}>
            Hash: {result.contract_hash}
          </div>
          <details>
            <summary className="text-xs cursor-pointer" style={{ color: "var(--accent)" }}>Full JSON</summary>
            <pre className="mt-2 p-3 rounded text-xs overflow-x-auto" style={{ background: "var(--bg-primary)", color: "var(--text-secondary)", borderColor: "var(--border-subtle)", border: "1px solid" }}>
              {JSON.stringify(result.contract, null, 2)}
            </pre>
          </details>
        </div>
      )}
    </div>
  );
}
