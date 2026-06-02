import { useState } from "react";
import { compileContract } from "../api/client";
import Icon from "../components/Icon";
import { t, useLanguage } from "../settings/i18n";
import type { ContractResponse } from "../types";

export default function Contracts() {
  useLanguage();
  const [intent, setIntent] = useState("");
  const [result, setResult] = useState<ContractResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleCompile() {
    if (!intent.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const response = await compileContract(intent);
      setResult(response);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="product-page">
      <div className="feature-hero">
        <div className="feature-hero-icon">
          <Icon name="file-text" />
        </div>
        <div>
          <h2>{t("contracts.title")}</h2>
        </div>
      </div>

      <div className="card">
        <textarea
          className="w-full p-3 rounded-md text-sm font-mono border resize-none focus:outline-none focus:ring-1"
          rows={4}
          placeholder={t("contracts.intent_placeholder")}
          value={intent}
          onChange={(event) => setIntent(event.target.value)}
          style={{ background: "var(--bg-primary)", color: "var(--text-primary)", borderColor: "var(--border-accent)" }}
        />
        <button
          onClick={handleCompile}
          disabled={loading || !intent.trim()}
          className="mt-3 px-4 py-2 text-xs font-semibold rounded-md transition-all duration-150 disabled:opacity-30"
          style={{ background: "var(--accent)", color: "#fff" }}
        >
          {loading ? t("contracts.compiling") : t("contracts.compile")}
        </button>
      </div>

      {error && (
        <div className="card" style={{ borderColor: "var(--red)", background: "var(--red-dim)" }}>
          <div className="text-xs" style={{ color: "var(--red)" }}>{error}</div>
        </div>
      )}

      {result && (
        <div className="card">
          <div className="mb-3 flex items-center justify-between">
            <span className="text-xs font-semibold uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>{t("contracts.compiled")}</span>
            <span className="text-xs px-2 py-0.5 rounded" style={{
              background: result.ambiguity_score > 0.5 ? "var(--yellow-dim)" : "var(--green-dim)",
              color: result.ambiguity_score > 0.5 ? "var(--yellow)" : "var(--green)",
            }}>
              {t("contracts.ambiguity")}: {result.ambiguity_score.toFixed(2)}
            </span>
          </div>
          <div className="mb-2 break-all font-mono text-xs" style={{ color: "var(--text-muted)" }}>
            {t("contracts.hash")}: {result.contract_hash}
          </div>
          <details>
            <summary className="text-xs cursor-pointer" style={{ color: "var(--accent)" }}>{t("contracts.full_json")}</summary>
            <pre className="mt-2 p-3 rounded text-xs overflow-x-auto" style={{ background: "var(--bg-primary)", color: "var(--text-secondary)", borderColor: "var(--border-subtle)", border: "1px solid" }}>
              {JSON.stringify(result.contract, null, 2)}
            </pre>
          </details>
        </div>
      )}
    </div>
  );
}
