import { useEffect, useState } from "react";
import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";
import { useEvaluationStore } from "../stores/evaluationStore";
import {
  evaluationManager,
  type CustomRpcEvaluatorConfig,
  type EvaluationJob,
} from "../evaluation/evaluators";

export default function Evaluations({ embedded = false }: { embedded?: boolean }) {
  useLanguage();
  const { evaluators, jobs, activeJobId, loadEvaluators, runEvaluation, setActiveJob } =
    useEvaluationStore();
  const [selectedEvaluator, setSelectedEvaluator] = useState<string>("");
  const [sampleInput, setSampleInput] = useState<string>(
    "You are a helpful assistant. Answer the user's question clearly."
  );
  const [running, setRunning] = useState(false);
  const [customRpcConfig, setCustomRpcConfig] = useState<CustomRpcEvaluatorConfig>({
    endpoint: "",
    auth_token: "",
  });
  const [customRpcSaved, setCustomRpcSaved] = useState(false);

  useEffect(() => {
    loadEvaluators();
  }, [loadEvaluators]);

  useEffect(() => {
    if (!selectedEvaluator && evaluators.length > 0) {
      setSelectedEvaluator(evaluators[0].id);
    }
  }, [evaluators, selectedEvaluator]);

  const selectedEvaluatorDef = evaluators.find((evaluator) => evaluator.id === selectedEvaluator);
  const selectedEvaluatorIsCustomRpc = selectedEvaluatorDef?.type === "custom_rpc";
  const customRpcReady = !selectedEvaluatorIsCustomRpc || customRpcConfig.endpoint.trim().length > 0;

  useEffect(() => {
    if (selectedEvaluatorIsCustomRpc) {
      setCustomRpcConfig(evaluationManager.getCustomRpcConfig());
      setCustomRpcSaved(false);
    }
  }, [selectedEvaluatorIsCustomRpc, selectedEvaluator]);

  function saveCustomRpcConfig() {
    evaluationManager.setCustomRpcConfig(customRpcConfig);
    setCustomRpcSaved(true);
  }

  async function handleRun() {
    if (!selectedEvaluator || running || !customRpcReady) return;
    if (selectedEvaluatorIsCustomRpc) {
      evaluationManager.setCustomRpcConfig(customRpcConfig);
    }
    setRunning(true);
    try {
      await runEvaluation(selectedEvaluator, "manual-target", sampleInput);
    } finally {
      setRunning(false);
    }
  }

  const jobList = Object.values(jobs).sort((a, b) => b.created_at - a.created_at);
  const activeJob = activeJobId ? jobs[activeJobId] : undefined;

  return (
    <div className={embedded ? "space-y-4" : "product-page"}>
      {!embedded && (
        <header className="product-header">
          <div className="min-w-0">
            <div className="product-kicker">{t("eval.kicker")}</div>
            <h1 className="product-title">{t("eval.title")}</h1>
          </div>
        </header>
      )}

      {!embedded && (
        <section className="feature-hero">
          <div className="feature-hero-icon"><Icon name="badge-check" /></div>
          <div>
            <h2>{t("eval.title")}</h2>
            <p>{t("eval.desc")}</p>
          </div>
        </section>
      )}

      <div className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("eval.run_title")}</h2>
          </div>
          <label className="block text-xs font-semibold mb-1" style={{ color: "var(--text-secondary)" }}>
            {t("eval.evaluator")}
          </label>
          <select
            className="select-control w-full mb-3"
            value={selectedEvaluator}
            onChange={(e) => setSelectedEvaluator(e.target.value)}
          >
            {evaluators.map((ev) => (
              <option key={ev.id} value={ev.id}>{ev.name} ({ev.type})</option>
            ))}
          </select>

          <label className="block text-xs font-semibold mb-1" style={{ color: "var(--text-secondary)" }}>
            {t("eval.input")}
          </label>
          <textarea
            className="composer-textarea w-full mb-3"
            style={{ minHeight: 120, border: "1px solid var(--border-subtle)", borderRadius: 8, padding: 10 }}
            value={sampleInput}
            onChange={(e) => setSampleInput(e.target.value)}
          />

          {selectedEvaluatorIsCustomRpc && (
            <div
              className="mb-3 rounded border p-3"
              style={{ borderColor: "var(--border-subtle)", background: "var(--bg-card)" }}
            >
              <label
                htmlFor="custom-rpc-endpoint"
                className="block text-xs font-semibold mb-1"
                style={{ color: "var(--text-secondary)" }}
              >
                {t("eval.custom_rpc_endpoint")}
              </label>
              <input
                id="custom-rpc-endpoint"
                type="url"
                className="text-input w-full mb-3"
                value={customRpcConfig.endpoint}
                onChange={(e) => {
                  setCustomRpcSaved(false);
                  setCustomRpcConfig((current) => ({ ...current, endpoint: e.target.value }));
                }}
              />

              <label
                htmlFor="custom-rpc-auth-token"
                className="block text-xs font-semibold mb-1"
                style={{ color: "var(--text-secondary)" }}
              >
                {t("eval.custom_rpc_auth_token")}
              </label>
              <input
                id="custom-rpc-auth-token"
                type="password"
                className="text-input w-full"
                value={customRpcConfig.auth_token}
                onChange={(e) => {
                  setCustomRpcSaved(false);
                  setCustomRpcConfig((current) => ({ ...current, auth_token: e.target.value }));
                }}
              />

              <div className="mt-2 text-xs" style={{ color: "var(--text-muted)" }}>
                {t("eval.custom_rpc_hint")}
              </div>

              <div className="mt-3 flex flex-wrap items-center gap-2">
                <button type="button" className="product-link-button" onClick={saveCustomRpcConfig}>
                  <Icon name="check" /> {t("eval.custom_rpc_save")}
                </button>
                {customRpcSaved && (
                  <span className="text-xs" style={{ color: "var(--green)" }}>
                    {t("eval.custom_rpc_saved")}
                  </span>
                )}
                {!customRpcReady && (
                  <span className="text-xs" style={{ color: "var(--yellow)" }}>
                    {t("eval.custom_rpc_required")}
                  </span>
                )}
              </div>
            </div>
          )}

          <button className="primary-button" disabled={running || !selectedEvaluator || !customRpcReady} onClick={handleRun}>
            {running ? (
              <span className="flex items-center gap-2">
                <Icon name="spinner" className="icon-spin" /> {t("eval.running")}
              </span>
            ) : (
              <span className="flex items-center gap-2">
                <Icon name="badge-check" /> {t("eval.run")}
              </span>
            )}
          </button>
        </div>

        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("eval.history")}</h2>
            <span>{jobList.length}</span>
          </div>
          {jobList.length === 0 ? (
            <div className="empty-state">
              <div className="empty-state-icon"><Icon name="clipboard" /></div>
              <p>{t("eval.empty")}</p>
            </div>
          ) : (
            <div className="product-list">
              {jobList.map((job) => (
                <button
                  key={job.job_id}
                  className="product-list-row"
                  onClick={() => setActiveJob(job.job_id)}
                  style={{ borderColor: job.job_id === activeJobId ? "var(--accent)" : undefined }}
                >
                  <span className="product-row-main">{job.evaluator_id.split("-")[0]}</span>
                  <JobStatusPill status={job.status} />
                </button>
              ))}
            </div>
          )}
        </div>
      </div>

      {activeJob && <JobResultPanel job={activeJob} />}
    </div>
  );
}

function JobStatusPill({ status }: { status: EvaluationJob["status"] }) {
  const tone =
    status === "completed" ? "green" : status === "failed" ? "red" : status === "running" ? "blue" : "yellow";
  return <span className={`product-pill ${tone}`}>{t(`eval.status_${status}`)}</span>;
}

function JobResultPanel({ job }: { job: EvaluationJob }) {
  const overall =
    job.results.length > 0
      ? Math.round(job.results.reduce((sum, r) => sum + r.score, 0) / job.results.length)
      : 0;

  return (
    <div className="product-panel">
      <div className="product-panel-heading">
        <h2>{t("eval.result_title")}</h2>
        {job.duration_ms != null && (
          <span className="mono-chip">{job.duration_ms}ms</span>
        )}
      </div>

      {job.status === "completed" && (
        <div className="mb-4">
          <div className="metric-value" style={{ color: overall >= 70 ? "var(--green)" : "var(--yellow)" }}>
            {overall}
          </div>
          <div className="metric-label">{t("eval.overall_score")}</div>
        </div>
      )}

      {job.error && (
        <div className="product-pill red mb-3">{job.error}</div>
      )}

      <div className="product-list">
        {job.results.map((r) => (
          <div key={r.metric_name} className="product-list-row static" style={{ flexDirection: "column", alignItems: "stretch", gap: 6 }}>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <span className="product-row-main">{r.metric_name}</span>
              <span className="flex items-center gap-2">
                <span className="mono-chip">{r.score}</span>
                <Icon
                  name={r.passed ? "check" : "x"}
                  style={{ color: r.passed ? "var(--green)" : "var(--red)" }}
                />
              </span>
            </div>
            {r.details && (
              <details style={{ marginTop: 2 }}>
                <summary style={{ cursor: "pointer", fontSize: 12, color: "var(--text-secondary)", fontWeight: 600 }}>
                  {t("eval.judge_reasoning")}
                </summary>
                <div style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 4, lineHeight: 1.5, whiteSpace: "pre-wrap" }}>
                  {r.details}
                </div>
              </details>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
