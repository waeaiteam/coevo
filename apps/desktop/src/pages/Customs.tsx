import { useState } from "react";
import { proposeFact } from "../api/client";

export default function Customs() {
  const [key, setKey] = useState("");
  const [layer, setLayer] = useState("Hypothesis");
  const [result, setResult] = useState<Record<string,unknown> | null>(null);
  const [loading, setLoading] = useState(false);

  async function handlePropose() {
    if (!key.trim()) return;
    setLoading(true);
    try {
      const res = await proposeFact({
        target_key: key,
        expected_version: 0,
        proposed_value: { data: "example" },
        cognitive_layer: layer,
        provenance_envelope: {
          source_agent_id: "desktop-agent",
          verification_tool_urn: "urn:mcp:tool:unit-test-runner",
          environmental_scope: { environment: "development", tenant_id: "desktop" },
          ttl_seconds: 3600,
          cryptographic_signature: "desktop-sig",
          verification_report: { passed: true },
          created_at: new Date().toISOString(),
        },
        dependency_entry_ids: [],
      });
      setResult(res);
    } catch (e: unknown) {
      setResult({ error: e instanceof Error ? e.message : String(e) } as Record<string,unknown>);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3">
        <span className="text-lg" style={{ color: "#8b5cf6" }}>◎</span>
        <h2 className="text-lg font-bold tracking-tight" style={{ color: "var(--text-primary)" }}>Cognitive Customs</h2>
      </div>
      <div className="card flex gap-3">
        <input
          className="flex-1 p-2 rounded-md text-sm font-mono border focus:outline-none focus:ring-1"
          placeholder="Blackboard key"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          style={{ background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border-accent)" }}
        />
        <select
          className="p-2 rounded-md text-sm border focus:outline-none"
          value={layer}
          onChange={(e) => setLayer(e.target.value)}
          style={{ background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border-accent)" }}
        >
          <option>Hypothesis</option>
          <option>Fact</option>
          <option>Suggestion</option>
          <option>Decision</option>
        </select>
        <button
          onClick={handlePropose}
          disabled={loading}
          className="px-4 py-2 text-xs font-semibold rounded-md transition-all duration-150 disabled:opacity-30"
          style={{ background: "#8b5cf6", color: "#fff" }}
        >
          {loading ? "···" : "Propose"}
        </button>
      </div>
      {result && (
        <div className="card">
          <pre className="text-xs overflow-x-auto" style={{ color: "var(--text-secondary)" }}>
            {JSON.stringify(result, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}
