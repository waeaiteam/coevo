import { useState } from "react";
import { proposeFact } from "../api/client";
import Icon from "../components/Icon";
import { getLocalIdentity } from "../settings/identity";
import { t, useLanguage } from "../settings/i18n";

export default function Customs() {
  useLanguage();
  const [key, setKey] = useState("");
  const [layer, setLayer] = useState("Hypothesis");
  const [valueText, setValueText] = useState("");
  const [result, setResult] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(false);

  async function handlePropose() {
    if (!key.trim() || loading) return;
    setLoading(true);
    // Use the real value the user typed: parse as JSON when possible, otherwise store
    // it as a plain string. No fabricated "example" payloads or fake signatures.
    let proposedValue: unknown = valueText.trim();
    try {
      if (valueText.trim()) proposedValue = JSON.parse(valueText);
    } catch {
      proposedValue = valueText.trim();
    }
    const identity = getLocalIdentity();
    try {
      const response = await proposeFact({
        target_key: key.trim(),
        expected_version: 0,
        proposed_value: proposedValue,
        cognitive_layer: layer,
        provenance_envelope: {
          source_agent_id: identity.userId,
          verification_tool_urn: "urn:coevo:desktop:manual-entry",
          environmental_scope: { environment: "desktop", tenant_id: identity.tenantId },
          ttl_seconds: 3600,
          // Local manual entry is not cryptographically attested; mark it honestly so
          // governance can treat it as a low-assurance, founder-authored proposal.
          cryptographic_signature: `local-manual:${identity.userId}`,
          verification_report: { passed: false, note: "manual desktop entry" },
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
      <div className="card flex flex-col gap-3">
        <input
          className="flex-1 p-2 rounded-md text-sm font-mono border focus:outline-none focus:ring-1"
          placeholder={t("customs.key_placeholder")}
          value={key}
          onChange={(event) => setKey(event.target.value)}
          style={{ background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border-accent)" }}
        />
        <textarea
          className="flex-1 p-2 rounded-md text-sm font-mono border focus:outline-none focus:ring-1"
          placeholder={t("customs.value_placeholder")}
          rows={4}
          value={valueText}
          onChange={(event) => setValueText(event.target.value)}
          style={{ background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border-accent)" }}
        />
        <div className="flex gap-3">
          <select
            className="p-2 rounded-md text-sm border focus:outline-none"
            value={layer}
            aria-label={t("customs.layer_label")}
            onChange={(event) => setLayer(event.target.value)}
            style={{ background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border-accent)" }}
          >
            <option value="Hypothesis">{t("customs.layer_hypothesis")}</option>
            <option value="Fact">{t("customs.layer_fact")}</option>
            <option value="Suggestion">{t("customs.layer_suggestion")}</option>
            <option value="Decision">{t("customs.layer_decision")}</option>
          </select>
          <button
            onClick={handlePropose}
            disabled={loading || !key.trim()}
            className="px-4 py-2 text-xs font-semibold rounded-md transition-all duration-150 disabled:opacity-30"
            style={{ background: "var(--accent)", color: "#fff" }}
          >
            {loading ? t("customs.proposing") : t("customs.propose")}
          </button>
        </div>
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
