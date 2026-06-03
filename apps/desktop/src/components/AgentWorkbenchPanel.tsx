import { useCallback, useEffect, useState } from "react";
import {
  deleteEmployee,
  listPromptVersions,
  updateEmployee,
  updateEmployeePrompt,
  type PromptVersion,
} from "../api/client";
import Icon from "./Icon";
import { t, useLanguage } from "../settings/i18n";

type Employee = Record<string, unknown>;

function num(v: unknown, fallback = 0): number {
  const n = Number(v);
  return Number.isFinite(n) ? n : fallback;
}

export default function AgentWorkbenchPanel({
  employee,
  onChanged,
  onDeleted,
}: {
  employee: Employee;
  onChanged: () => void;
  onDeleted: () => void;
}) {
  useLanguage();
  const agentId = String(employee.agent_id || "");

  const [systemPrompt, setSystemPrompt] = useState(String(employee.system_prompt || ""));
  const [riskCeiling, setRiskCeiling] = useState(num(employee.risk_ceiling, 0.3));
  const [lifecycle, setLifecycle] = useState(String(employee.lifecycle_status || "active"));
  const [savingPrompt, setSavingPrompt] = useState(false);
  const [savingConfig, setSavingConfig] = useState(false);
  const [promptSaved, setPromptSaved] = useState(false);
  const [versions, setVersions] = useState<PromptVersion[]>([]);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState("");

  // Reset local state when a different employee is selected.
  useEffect(() => {
    setSystemPrompt(String(employee.system_prompt || ""));
    setRiskCeiling(num(employee.risk_ceiling, 0.3));
    setLifecycle(String(employee.lifecycle_status || "active"));
    setConfirmDelete(false);
    setError("");
    setPromptSaved(false);
  }, [employee]);

  const loadVersions = useCallback(async () => {
    if (!agentId) return;
    try {
      setVersions(await listPromptVersions(agentId));
    } catch {
      setVersions([]);
    }
  }, [agentId]);

  useEffect(() => {
    loadVersions();
  }, [loadVersions]);

  async function savePrompt() {
    if (savingPrompt) return;
    setSavingPrompt(true);
    setError("");
    try {
      await updateEmployeePrompt(agentId, systemPrompt, t("workbench.change_via_workbench"));
      setPromptSaved(true);
      setTimeout(() => setPromptSaved(false), 2000);
      await loadVersions();
      onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSavingPrompt(false);
    }
  }

  async function saveConfig() {
    if (savingConfig) return;
    setSavingConfig(true);
    setError("");
    try {
      const updated = {
        ...employee,
        risk_ceiling: riskCeiling,
        lifecycle_status: lifecycle,
      };
      await updateEmployee(agentId, updated);
      onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSavingConfig(false);
    }
  }

  async function doDelete() {
    setDeleting(true);
    setError("");
    try {
      await deleteEmployee(agentId);
      onDeleted();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setDeleting(false);
    }
  }

  function restoreVersion(v: PromptVersion) {
    setSystemPrompt(v.content);
  }

  return (
    <div className="space-y-4">
      {error && <div className="product-pill red">{error}</div>}

      {/* System prompt editor */}
      <div className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("workbench.system_prompt")}</h2>
          {promptSaved && <span className="product-pill green">{t("workbench.saved")}</span>}
        </div>
        <p className="product-prose mb-2">{t("workbench.system_prompt_hint")}</p>
        <textarea
          className="composer-textarea w-full"
          style={{ minHeight: 140, border: "1px solid var(--border-subtle)", borderRadius: 8, padding: 10 }}
          value={systemPrompt}
          placeholder={t("workbench.system_prompt_placeholder")}
          onChange={(e) => setSystemPrompt(e.target.value)}
        />
        <div className="flex items-center gap-2 mt-3">
          <button className="primary-button" disabled={savingPrompt} onClick={savePrompt}>
            {savingPrompt ? <Icon name="spinner" className="icon-spin" /> : <Icon name="check" />} {t("workbench.save_prompt")}
          </button>
        </div>
      </div>

      {/* Prompt version history */}
      <div className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("workbench.version_history")}</h2>
          <span>{versions.length}</span>
        </div>
        {versions.length === 0 ? (
          <div className="empty-state">
            <div className="empty-state-icon"><Icon name="history" /></div>
            <p>{t("workbench.no_versions")}</p>
          </div>
        ) : (
          <div className="product-list">
            {versions.map((v) => (
              <div key={v.version_id} className="product-list-row static" style={{ flexWrap: "wrap", gap: 8 }}>
                <span className="product-row-main" style={{ flex: "1 1 200px" }}>
                  <span className="mono-chip">v{v.version_number}</span>{" "}
                  {v.change_summary || t("workbench.no_summary")}
                  {v.status === "PUBLISHED" && <span className="product-pill green" style={{ marginLeft: 6 }}>{t("workbench.current")}</span>}
                </span>
                <button className="product-link-button" onClick={() => restoreVersion(v)}>
                  <Icon name="history" /> {t("workbench.restore")}
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Runtime config */}
      <div className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("workbench.config")}</h2>
        </div>
        <div className="product-grid-2">
          <label className="block">
            <span className="metric-label">{t("workbench.risk_ceiling")}</span>
            <input
              type="number" min={0} max={1} step={0.1}
              className="select-control w-full mt-1"
              value={riskCeiling}
              onChange={(e) => setRiskCeiling(num(e.target.value, 0.3))}
            />
          </label>
          <label className="block">
            <span className="metric-label">{t("workbench.lifecycle")}</span>
            <select className="select-control w-full mt-1" value={lifecycle} onChange={(e) => setLifecycle(e.target.value)}>
              <option value="active">{t("workbench.status_active")}</option>
              <option value="suspended">{t("workbench.status_suspended")}</option>
              <option value="draft">{t("workbench.status_draft")}</option>
            </select>
          </label>
        </div>
        <button className="primary-button mt-3" disabled={savingConfig} onClick={saveConfig}>
          {savingConfig ? <Icon name="spinner" className="icon-spin" /> : <Icon name="sliders" />} {t("workbench.save_config")}
        </button>
      </div>

      {/* Danger zone: delete */}
      <div className="product-panel" style={{ borderColor: "var(--red)" }}>
        <div className="product-panel-heading">
          <h2 style={{ color: "var(--red)" }}>{t("workbench.danger_zone")}</h2>
        </div>
        {!confirmDelete ? (
          <button className="product-link-button" style={{ color: "var(--red)", borderColor: "var(--red)" }} onClick={() => setConfirmDelete(true)}>
            <Icon name="x" /> {t("workbench.delete")}
          </button>
        ) : (
          <div className="flex items-center gap-2 flex-wrap">
            <span className="product-prose">{t("workbench.delete_confirm")}</span>
            <button className="primary-button" style={{ background: "var(--red)", borderColor: "var(--red)" }} disabled={deleting} onClick={doDelete}>
              {deleting ? <Icon name="spinner" className="icon-spin" /> : <Icon name="x" />} {t("workbench.delete_yes")}
            </button>
            <button className="product-link-button" onClick={() => setConfirmDelete(false)}>{t("workbench.cancel")}</button>
          </div>
        )}
      </div>
    </div>
  );
}
