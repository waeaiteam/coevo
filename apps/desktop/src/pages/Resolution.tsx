import { useState } from "react";
import { resolveConflict } from "../api/client";
import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";
import { useToast } from "../components/ToastProvider";

type StanceForm = {
  agent_id: string;
  position: "SUPPORT" | "OPPOSE";
  weight: string;
  has_veto: boolean;
};

type ResolveResult = {
  verdict?: string;
  adr_id?: string | null;
  blocking_nodes?: string[];
  escalation?: string | null;
  error?: string;
};

const emptyStance = (): StanceForm => ({ agent_id: "", position: "SUPPORT", weight: "1", has_veto: false });

export default function Resolution() {
  useLanguage();
  const toast = useToast();
  const [issue, setIssue] = useState("");
  const [stances, setStances] = useState<StanceForm[]>([emptyStance(), { ...emptyStance(), position: "OPPOSE" }]);
  const [result, setResult] = useState<ResolveResult | null>(null);
  const [loading, setLoading] = useState(false);

  function updateStance(index: number, patch: Partial<StanceForm>) {
    setStances((prev) => prev.map((s, i) => (i === index ? { ...s, ...patch } : s)));
  }

  async function process() {
    const filled = stances.filter((s) => s.agent_id.trim());
    if (!issue.trim() || filled.length === 0 || loading) return;
    setLoading(true);
    setResult(null);
    try {
      const response = await resolveConflict({
        issue: issue.trim(),
        stances: filled.map((s) => ({
          agent_id: s.agent_id.trim(),
          position: s.position,
          weight: Number(s.weight) || 1,
          evidence_urns: [],
          has_veto: s.has_veto,
          compromise_proposal: null,
          round: 0,
        })),
      }) as ResolveResult;
      setResult(response);
      toast.success(t("resolution.resolved"));
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      setResult({ error: message });
      toast.error(`${t("resolution.failed")}: ${message}`);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("resolution.title")}</div>
          <h1 className="product-title">{t("resolution.title")}</h1>
          <p className="product-subtitle">{t("resolution.subtitle")}</p>
        </div>
      </header>

      <section className="product-panel">
        <label className="product-field-label" htmlFor="resolution-issue">{t("resolution.issue")}</label>
        <textarea
          id="resolution-issue"
          className="composer-textarea"
          style={{ border: "1px solid var(--border-subtle)", borderRadius: 8, minHeight: 64 }}
          value={issue}
          placeholder={t("resolution.issue_placeholder")}
          onChange={(event) => setIssue(event.target.value)}
        />

        <div className="product-panel-heading" style={{ marginTop: 16 }}>
          <h2>{t("resolution.stances")}</h2>
          <button type="button" className="product-link-button" onClick={() => setStances((prev) => [...prev, emptyStance()])}>
            <Icon name="plus" /> {t("resolution.add_stance")}
          </button>
        </div>
        <div className="product-list">
          {stances.map((stance, index) => (
            <div key={index} className="product-card-row" style={{ flexDirection: "row", flexWrap: "wrap", gap: 8, alignItems: "center" }}>
              <input
                className="select-control"
                style={{ flex: 1, minWidth: 140 }}
                placeholder={t("resolution.agent_id")}
                aria-label={t("resolution.agent_id")}
                value={stance.agent_id}
                onChange={(event) => updateStance(index, { agent_id: event.target.value })}
              />
              <select className="select-control" aria-label={t("resolution.position")} value={stance.position} onChange={(event) => updateStance(index, { position: event.target.value as StanceForm["position"] })}>
                <option value="SUPPORT">{t("resolution.support")}</option>
                <option value="OPPOSE">{t("resolution.oppose")}</option>
              </select>
              <input
                className="select-control"
                style={{ width: 80 }}
                type="number"
                min="0"
                max="1"
                step="0.1"
                aria-label={t("resolution.weight")}
                value={stance.weight}
                onChange={(event) => updateStance(index, { weight: event.target.value })}
              />
              <label className="mono-chip" style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                <input type="checkbox" checked={stance.has_veto} onChange={(event) => updateStance(index, { has_veto: event.target.checked })} />
                {t("resolution.veto")}
              </label>
              {stances.length > 1 && (
                <button type="button" className="icon-button" aria-label={t("resolution.remove_stance")} onClick={() => setStances((prev) => prev.filter((_, i) => i !== index))}>
                  <Icon name="x" />
                </button>
              )}
            </div>
          ))}
        </div>

        <div className="product-actions" style={{ marginTop: 16, justifyContent: "flex-start" }}>
          <button type="button" className="primary-button product-action" disabled={loading || !issue.trim()} onClick={process}>
            {loading ? t("resolution.processing") : t("resolution.process")}
          </button>
        </div>
      </section>

      {result && (
        <section className="product-panel">
          <div className="product-panel-heading"><h2>{t("resolution.result")}</h2></div>
          {result.error ? (
            <div className="product-pill red">{result.error}</div>
          ) : (
            <div className="product-list">
              <div className="product-list-row static"><span className="product-row-main">{t("resolution.verdict")}</span><span className="product-row-meta">{result.verdict}</span></div>
              {result.adr_id && <div className="product-list-row static"><span className="product-row-main">{t("resolution.adr")}</span><span className="product-row-meta">{result.adr_id}</span></div>}
              {result.blocking_nodes && result.blocking_nodes.length > 0 && (
                <div className="product-list-row static"><span className="product-row-main">{t("resolution.blocking")}</span><span className="product-row-meta">{result.blocking_nodes.join(", ")}</span></div>
              )}
              {result.escalation && <div className="product-list-row static"><span className="product-row-main">{t("resolution.escalation")}</span><span className="product-row-meta">{result.escalation}</span></div>}
            </div>
          )}
        </section>
      )}
    </div>
  );
}
