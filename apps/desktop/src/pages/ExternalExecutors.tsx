import { useEffect, useState } from "react";
import { disableExecutor, executorHealth, listExecutors, registerExecutor, listWorkOrders, executorDryRun } from "../api/client";
import { useToast } from "../components/ToastProvider";
import { t, useLanguage } from "../settings/i18n";

const SOURCE_TYPES = ["Hermes", "OpenClaw", "MCP", "Local302AI", "Browser", "LocalProcess", "Docker"];

export default function ExternalExecutors() {
  useLanguage();
  const toast = useToast();
  const [execs, setExecs] = useState<Record<string, unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [showReg, setShowReg] = useState(false);
  const [regForm, setRegForm] = useState<Record<string, string>>({ executor_id: "", display_name: "", source_type: "OpenClaw", risk_ceiling: "0.5", sandbox_level: "None" });
  const [workOrders, setWorkOrders] = useState<Record<string, unknown>[]>([]);
  // dry-run target work order, isolated per executor card so selecting one card's
  // target does not change another's.
  const [dryRunIds, setDryRunIds] = useState<Record<string, string>>({});
  const [dryRunResult, setDryRunResult] = useState("");
  const [notice, setNotice] = useState("");

  async function load() {
    setLoading(true);
    try {
      setExecs(await listExecutors() || []);
    } catch {
      setExecs([]);
    }
    setLoading(false);
  }

  useEffect(() => {
    void load();
    void listWorkOrders().then((work) => setWorkOrders(work || []));
  }, []);

  async function reg() {
    if (!regForm.display_name) return;
    // Honor the risk ceiling the user typed (clamped to 0..1); fall back to 0.5.
    const parsedCeiling = Number(regForm.risk_ceiling);
    const riskCeiling = Number.isFinite(parsedCeiling) ? Math.min(1, Math.max(0, parsedCeiling)) : 0.5;
    try {
      await registerExecutor({
        ...regForm,
        executor_id: regForm.executor_id || "exec-" + crypto.randomUUID(),
        capabilities: ["read"],
        required_credentials: [],
        permission_boundary: {
          max_risk_score: riskCeiling,
          can_write_fact: false,
          can_write_decision: false,
          can_access_network: false,
          can_access_filesystem: false,
          can_call_external_executor: false,
          can_propose_skill: false,
        },
        file_scope: [],
        network_scope: [],
        memory_scope: "Executor",
        supported_actions: ["read"],
        health_check_url: "",
        audit_callback_url: "",
        status: "Registered",
        runtime_endpoint: "",
        risk_ceiling: riskCeiling,
        created_at_ms: Date.now(),
        updated_at_ms: Date.now(),
      });
      setShowReg(false);
      setNotice("");
      await load();
      toast.success(t("toast.executor_registered"));
    } catch (e: unknown) {
      setNotice(e instanceof Error ? e.message : String(e));
      toast.error(`${t("toast.executor_register_failed")}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function dryRun(executorId: string, workOrderId: string) {
    setDryRunResult("");
    setNotice("");
    try {
      const response = await executorDryRun(executorId, workOrderId);
      setDryRunResult(JSON.stringify(response));
      toast.success(t("toast.executor_dry_run_done"));
    } catch (e: unknown) {
      setNotice(`${t("executors.dry_run_failed")}: ` + (e instanceof Error ? e.message : String(e)));
      toast.error(`${t("toast.executor_action_failed")}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function runHealth(executorId: string) {
    try {
      await executorHealth(executorId);
      toast.success(t("toast.executor_health_ok"));
    } catch (e: unknown) {
      toast.error(`${t("toast.executor_action_failed")}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function runDisable(executorId: string) {
    try {
      await disableExecutor(executorId);
      await load();
      toast.info(t("toast.executor_disabled"));
    } catch (e: unknown) {
      toast.error(`${t("toast.executor_action_failed")}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-lg font-semibold" style={{ color: "var(--accent)" }}>E</span>
          <h2 className="text-lg font-bold">{t("executors.title")}</h2>
        </div>
        <button onClick={() => setShowReg(!showReg)} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>+ {t("executors.register_new")}</button>
      </div>
      <div className="text-xs p-3 rounded" style={{ background: "var(--accent-dim)", color: "var(--accent)" }}>{t("executors.desc")}</div>
      <div className="text-xs p-3 rounded" style={{ background: "var(--yellow-dim)", color: "var(--yellow)" }}>
        {t("executors.alpha_note")}
      </div>

      {showReg && (
        <div className="card space-y-2">
          <input placeholder={t("executors.display_name")} value={regForm.display_name || ""} onChange={(event) => setRegForm({ ...regForm, display_name: event.target.value })} className="input" />
          <select value={regForm.source_type || "OpenClaw"} onChange={(event) => setRegForm({ ...regForm, source_type: event.target.value })} className="input">{SOURCE_TYPES.map((source) => <option key={source}>{source}</option>)}</select>
          <input placeholder={t("executors.risk_ceiling")} value={regForm.risk_ceiling || ""} onChange={(event) => setRegForm({ ...regForm, risk_ceiling: event.target.value })} className="input" />
          <div className="text-xs" style={{ color: "var(--text-muted)" }}>{t("executors.alpha_note")}</div>
          <div className="flex gap-2">
            <button onClick={reg} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>{t("executors.register_new")}</button>
            <button onClick={() => setShowReg(false)} className="px-3 py-1.5 text-xs rounded-md" style={{ color: "var(--text-muted)" }}>{t("executors.cancel")}</button>
          </div>
        </div>
      )}

      {loading && <div className="text-xs" style={{ color: "var(--text-muted)" }}>{t("executors.loading")}</div>}
      {notice && <div className="text-xs p-3 rounded" style={{ background: "var(--red-dim)", color: "var(--red)" }}>{notice}</div>}
      <div className="grid gap-2 md:grid-cols-2">
        {execs.map((executor, index) => (
          <div key={String(executor.executor_id || index)} className="card">
            <div className="mb-1 flex justify-between">
              <span className="text-sm font-semibold">{executor.display_name as string}</span>
              <span className="text-xs px-1.5 py-0.5 rounded" style={{ background: executor.status === "Registered" ? "var(--green-dim)" : "var(--yellow-dim)", color: executor.status === "Registered" ? "var(--green)" : "var(--yellow)" }}>{executor.status as string}</span>
            </div>
            <div className="text-xs space-y-0.5" style={{ color: "var(--text-muted)" }}>
              <div>{t("executors.meta_type")}: {executor.source_type as string} | {t("executors.meta_sandbox")}: {executor.sandbox_level as string} | {t("executors.meta_risk")}: {String(executor.risk_ceiling)}</div>
              <div className="font-mono text-xs" style={{ color: "var(--accent)" }}>{executor.executor_id as string}</div>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <button onClick={() => runHealth(executor.executor_id as string)} className="text-xs px-2 py-1 rounded border" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("executors.health")}</button>
                <button onClick={() => runDisable(executor.executor_id as string)} className="text-xs px-2 py-1 rounded border" style={{ borderColor: "var(--red)", color: "var(--red)" }}>{t("executors.disable")}</button>
                <select value={dryRunIds[executor.executor_id as string] || ""} onChange={(event) => setDryRunIds((prev) => ({ ...prev, [executor.executor_id as string]: event.target.value }))} className="text-xs px-1 py-1 rounded border" style={{ borderColor: "var(--border-accent)", color: "var(--text-secondary)" }}>
                  <option value="">{t("executors.select_work_order")}</option>
                  {workOrders.map((workOrder, itemIndex) => <option key={itemIndex} value={workOrder.work_order_id as string}>{(workOrder.mission_intent as string || "").slice(0, 30)}</option>)}
                </select>
                <button onClick={() => dryRun(executor.executor_id as string, dryRunIds[executor.executor_id as string] || "")} disabled={!dryRunIds[executor.executor_id as string]} className="text-xs px-2 py-1 rounded border" style={{ borderColor: "var(--yellow)", color: "var(--yellow)" }}>{t("executors.dry_run")}</button>
              </div>
            </div>
          </div>
        ))}
      </div>
      {dryRunResult && <div className="card"><pre className="text-xs" style={{ color: "var(--text-secondary)" }}>{dryRunResult}</pre></div>}
    </div>
  );
}
