import { useState } from "react";
import Icon from "../components/Icon";
import { modelChat, runPlayground, type PlaygroundResult } from "../api/client";
import { getActiveOpcId } from "../api/companies";
import { t, useLanguage } from "../settings/i18n";

// 试跑场 — run one instruction across several models and compare side by side.
// Prefers the real contract endpoint (POST /companies/{opc}/playground/run); if the
// backend hasn't shipped it yet, falls back to the live model gateway (modelChat)
// one call per model, so the comparison is real today and switches over seamlessly.

const DEFAULT_MODELS = ["gpt-4o", "gpt-4o-mini"];

async function runViaModelGateway(
  systemPrompt: string,
  userInput: string,
  models: string[],
): Promise<PlaygroundResult[]> {
  return Promise.all(
    models.map(async (model): Promise<PlaygroundResult> => {
      const startedAt = Date.now();
      try {
        const res = (await modelChat({
          role: "MissionDraft",
          model,
          messages: [
            { role: "system", content: systemPrompt },
            { role: "user", content: userInput },
          ],
          temperature: 0.2,
          max_tokens: 600,
        })) as Record<string, unknown>;
        const usage = (res.usage || {}) as Record<string, unknown>;
        return {
          model: String(res.model || model),
          output: String(res.content || ""),
          input_tokens: Number(usage.input_tokens || usage.prompt_tokens || 0),
          output_tokens: Number(usage.output_tokens || usage.completion_tokens || 0),
          cost_usd: Number(res.cost_usd || 0),
          latency_ms: Date.now() - startedAt,
          error: null,
        };
      } catch (e) {
        return {
          model,
          output: "",
          input_tokens: 0,
          output_tokens: 0,
          cost_usd: 0,
          latency_ms: Date.now() - startedAt,
          error: e instanceof Error ? e.message : String(e),
        };
      }
    }),
  );
}

export default function EmployeePlayground({
  agentId,
  initialPrompt = "",
}: {
  agentId?: string;
  initialPrompt?: string;
}) {
  useLanguage();
  const [systemPrompt, setSystemPrompt] = useState(initialPrompt);
  const [userInput, setUserInput] = useState("");
  const [models, setModels] = useState<string[]>(DEFAULT_MODELS);
  const [results, setResults] = useState<PlaygroundResult[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState("");

  function updateModel(index: number, value: string) {
    setModels((prev) => prev.map((m, i) => (i === index ? value : m)));
  }

  function addModel() {
    setModels((prev) => [...prev, ""]);
  }

  function removeModel(index: number) {
    setModels((prev) => (prev.length <= 1 ? prev : prev.filter((_, i) => i !== index)));
  }

  async function run() {
    const activeModels = models.map((m) => m.trim()).filter(Boolean);
    if (running) return;
    if (activeModels.length === 0) {
      setError(t("play.no_models"));
      return;
    }
    setRunning(true);
    setError("");
    try {
      let next: PlaygroundResult[];
      try {
        const resp = await runPlayground(getActiveOpcId(), {
          agent_id: agentId,
          system_prompt: systemPrompt,
          user_input: userInput,
          models: activeModels,
          temperature: 0.2,
        });
        next = resp.results;
      } catch {
        // Backend endpoint not ready — compare via the live model gateway directly.
        next = await runViaModelGateway(systemPrompt, userInput, activeModels);
      }
      setResults(next);
    } finally {
      setRunning(false);
    }
  }

  return (
    <div className="space-y-4">
      {error && <div className="product-pill red">{error}</div>}

      <div className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("play.system_prompt")}</h2>
        </div>
        <textarea
          className="composer-textarea w-full"
          style={{ minHeight: 90, border: "1px solid var(--border-subtle)", borderRadius: 8, padding: 10 }}
          value={systemPrompt}
          placeholder={t("play.system_prompt_placeholder")}
          onChange={(e) => setSystemPrompt(e.target.value)}
        />

        <div className="product-panel-heading mt-4">
          <h2>{t("play.user_input")}</h2>
        </div>
        <textarea
          className="composer-textarea w-full"
          style={{ minHeight: 70, border: "1px solid var(--border-subtle)", borderRadius: 8, padding: 10 }}
          value={userInput}
          placeholder={t("play.user_input_placeholder")}
          onChange={(e) => setUserInput(e.target.value)}
        />

        <div className="product-panel-heading mt-4">
          <h2>{t("play.models")}</h2>
          <button type="button" className="product-link-button" onClick={addModel}>
            <Icon name="plus" /> {t("play.add_model")}
          </button>
        </div>
        <div className="space-y-2">
          {models.map((model, index) => (
            <div key={index} className="play-model-row">
              <input
                className="select-control play-model-input"
                value={model}
                placeholder="gpt-4o"
                onChange={(e) => updateModel(index, e.target.value)}
              />
              {models.length > 1 && (
                <button type="button" className="icon-button" onClick={() => removeModel(index)} aria-label={t("play.remove_model")}>
                  <Icon name="x" />
                </button>
              )}
            </div>
          ))}
        </div>

        <button className="primary-button mt-4" disabled={running} onClick={() => void run()}>
          {running ? <Icon name="spinner" className="icon-spin" /> : <Icon name="sparkles" />}{" "}
          {running ? t("play.running") : t("play.run")}
        </button>
      </div>

      {running && results.length === 0 && (
        <div className="play-skeleton" aria-label={t("play.running")}>
          <div className="play-skeleton-bar" style={{ width: "100%" }} />
          <div className="play-skeleton-bar" />
          <div className="play-skeleton-bar" />
        </div>
      )}

      {results.length === 0 && !running ? (
        <div className="empty-state">
          <div className="empty-state-icon"><Icon name="layers" /></div>
          <p>{t("play.empty")}</p>
        </div>
      ) : results.length > 0 ? (
        <div className="play-results">
          {results.map((result, index) => (
            <div key={`${result.model}-${index}`} className={`play-result ${result.error ? "error" : ""}`}>
              <div className="play-result-head">
                <span className="play-result-model">{result.model}</span>
                {result.error
                  ? <span className="product-pill red">{t("play.result_error")}</span>
                  : <span className="mono-chip">{result.latency_ms}ms</span>}
              </div>
              <div className="play-result-output">
                {result.error ? result.error : result.output}
              </div>
              {!result.error && (
                <div className="play-result-meta">
                  <span className="mono-chip">{result.input_tokens + result.output_tokens} {t("play.result_tokens")}</span>
                  <span className="mono-chip">${result.cost_usd.toFixed(4)} {t("play.result_cost")}</span>
                </div>
              )}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
