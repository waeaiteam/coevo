import { useState } from "react";
import { compileContract, ContractResponse } from "../api/client";
import ContractViewer from "../components/ContractViewer";

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
    <div>
      <h2 className="text-2xl font-bold mb-6">MCL Contracts</h2>
      <div className="mb-6">
        <textarea
          className="w-full p-3 border rounded font-mono text-sm"
          rows={4}
          placeholder="Enter user intent (e.g., 'Read system health metrics in development...')"
          value={intent}
          onChange={(e) => setIntent(e.target.value)}
        />
        <button
          onClick={handleCompile}
          disabled={loading || !intent.trim()}
          className="mt-2 px-4 py-2 bg-green-600 text-white rounded hover:bg-green-700 disabled:opacity-50"
        >
          {loading ? "Compiling..." : "Compile"}
        </button>
      </div>

      {error && (
        <div className="p-4 bg-red-50 border border-red-200 rounded text-red-700 text-sm mb-4">
          {error}
        </div>
      )}

      {result && <ContractViewer result={result} />}
    </div>
  );
}
