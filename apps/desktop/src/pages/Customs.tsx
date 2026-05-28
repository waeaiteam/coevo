import { useState } from "react";
import { proposeFact } from "../api/client";
import CognitiveBoard from "../components/CognitiveBoard";

export default function Customs() {
  const [key, setKey] = useState("");
  const [layer, setLayer] = useState("Hypothesis");
  const [result, setResult] = useState<Record<string, unknown> | null>(null);

  async function handlePropose() {
    if (!key.trim()) return;
    try {
      const res = await proposeFact({
        target_key: key,
        expected_version: 1,
        proposed_value: { data: "example", note: "from desktop" },
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
      setResult({ error: e instanceof Error ? e.message : String(e) });
    }
  }

  return (
    <div>
      <h2 className="text-2xl font-bold mb-6">Cognitive Customs</h2>
      <div className="flex gap-4 mb-6">
        <input
          className="flex-1 p-2 border rounded text-sm"
          placeholder="Blackboard key"
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <select
          className="p-2 border rounded text-sm"
          value={layer}
          onChange={(e) => setLayer(e.target.value)}
        >
          <option>Hypothesis</option>
          <option>Fact</option>
          <option>Suggestion</option>
          <option>Decision</option>
        </select>
        <button
          onClick={handlePropose}
          className="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 text-sm"
        >
          Propose
        </button>
      </div>
      {result && <CognitiveBoard result={result} />}
    </div>
  );
}
