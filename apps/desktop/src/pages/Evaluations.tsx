import { useEffect, useState } from "react";
import { t, useLanguage } from "../settings/i18n";
import Icon from "../components/Icon";
import { useEvaluationStore } from "../stores/evaluationStore";
import type { EvaluationJob } from "../evaluation/evaluators";

export default function Evaluations() {
  useLanguage();
  const { evaluators, jobs, activeJobId, loadEvaluators, runEvaluation, setActiveJob } =
    useEvaluationStore();
  const [selectedEvaluator, setSelectedEvaluator] = useState<string>("");
  const [sampleInput, setSampleInput] = useState<string>(
    "You are a helpful assistant. Answer the user's question clearly."
  );
  const [running, setRunning] = useState(false);

  useEffect(() => {
    loadEvaluators();
  }, [loadEvaluators]);

  useEffect(() => {
    if (!selectedEvaluator && evaluators.length > 0) {
      setSelectedEvaluator(evaluators[0].id);
    }
  }, [evaluators, selectedEvaluator]);

  async function handleRun() {
    if (!selectedEvaluator || running) return;
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
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("eval.kicker")}</div>
          <h1 className="product-title">{t("eval.title")}</h1>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="badge-check" /></div>
        <div>
          <h2>{t("eval.title")}</h2>
          <p>{t("eval.desc")}</p>
        </div>
      </section>

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

          <button className="primary-button" disabled={running || !selectedEvaluator} onClick={handleRun}>
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
          <div key={r.metric_name} className="product-list-row static">
            <span className="product-row-main">{r.metric_name}</span>
            <span className="flex items-center gap-2">
              <span className="mono-chip">{r.score}</span>
              <Icon
                name={r.passed ? "check" : "x"}
                style={{ color: r.passed ? "var(--green)" : "var(--red)" }}
              />
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
