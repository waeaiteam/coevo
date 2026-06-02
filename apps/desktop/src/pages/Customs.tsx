import { useState } from "react";
import { proposeFact } from "../api/client";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";

export default function Customs() {
  useLanguage();
  const [key, setKey] = useState("");
  const [layer, setLayer] = useState("Hypothesis");
  const [result, setResult] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(false);

  async function handlePropose() {
    if (!key.trim()) return;
    setLoading(true);
    try {
      const response = await proposeFact({
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
      setResult(response);
    } catch (e: unknown) {
      setResult({ error: e instanceof Error ? e.message : String(e) });
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="product-page">
      <div className="feature-hero">
        <div className="feature-hero-icon">
          <Icon name="badge-check" />
        </div>
        <div>
          <h2>{t("customs.title")}</h2>
        </div>
      </div>
      <div className="card flex gap-3">
        <input
          className="flex-1 p-2 rounded-md text-sm font-mono border focus:outline-none focus:ring-1"
          placeholder={t("customs.key_placeholder")}
          value={key}
          onChange={(event) => setKey(event.target.value)}
          style={{ background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border-accent)" }}
        />
        <select
          className="p-2 rounded-md text-sm border focus:outline-none"
          value={layer}
          onChange={(event) => setLayer(event.target.value)}
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
          style={{ background: "var(--accent)", color: "#fff" }}
        >
          {loading ? t("customs.proposing") : t("customs.propose")}
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
