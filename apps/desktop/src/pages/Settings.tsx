import { useEffect, useState } from "react";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";
import NumberField from "../components/NumberField";
import PasswordField from "../components/PasswordField";
import SelectField from "../components/SelectField";
import SettingRow from "../components/SettingRow";
import SettingsLayout from "../components/SettingsLayout";
import SettingsSection from "../components/SettingsSection";
import TextField from "../components/TextField";
import ToggleField from "../components/ToggleField";
import { ensureWorkspaceDefaults } from "../api/bootstrap";
import { discoverModels, getApiBase, testModelConnection, updateModelConfig } from "../api/client";
import { getTauriInvoke } from "../api/tauri";
import { saveSettingsSnapshot, useSettings } from "../hooks/useSettings";
import { setLanguage, t, useLanguage } from "../settings/i18n";
import { chooseModelRoles, isKnownProvider, presetFor, providerOptions, type DiscoveredModel } from "../settings/modelPresets";
import { markModelProviderConfigured } from "../settings/onboarding";
import type { PolicyEngineType, ProviderType } from "../settings/types";

const CATEGORIES = ["general", "appearance", "model_provider", "agent_runtime", "governance", "risk_gate", "cognitive_customs", "policy_engine", "privacy", "developer", "data_management"];

const panels: Record<string, React.FC> = {
  general: GeneralPanel,
  appearance: AppearancePanel,
  model_provider: ModelProviderPanel,
  agent_runtime: AgentRuntimePanel,
  governance: GovernancePanel,
  risk_gate: RiskGatePanel,
  cognitive_customs: CognitiveCustomsPanel,
  policy_engine: PolicyEnginePanel,
  privacy: PrivacyPanel,
  developer: DeveloperPanel,
  data_management: DataManagementPanel,
};

export default function Settings() {
  return (
    <Routes>
      <Route index element={<Navigate to="general" replace />} />
      {CATEGORIES.map((cat) => <Route key={cat} path={cat} element={<SettingsCategory cat={cat} />} />)}
    </Routes>
  );
}

function SettingsCategory({ cat }: { cat: string }) {
  useLanguage();
  const Panel = panels[cat] || GeneralPanel;
  return <SettingsLayout section={cat as never} content={<Panel />} />;
}

function GeneralPanel() {
  const { settings, update } = useSettings();
  const g = settings.general;
  return (
    <SettingsSection title={t("settings.general")}>
      <SettingRow label={t("settings.default_home")} desc={t("settings.default_home_desc")}>
        <SelectField value={g.default_home} options={[{ value: "mission-chat", label: t("settings.home_mission_chat") }, { value: "dashboard", label: t("settings.home_opc") }]} onChange={(v) => update("general", { default_home: v as never })} />
      </SettingRow>
      <SettingRow label={t("settings.startup_behavior")} desc={t("settings.startup_behavior_desc")}>
        <SelectField value={g.startup_behavior} options={[{ value: "last-task", label: t("settings.open_last_task") }, { value: "new-task", label: t("settings.open_new_task") }]} onChange={(v) => update("general", { startup_behavior: v as never })} />
      </SettingRow>
    </SettingsSection>
  );
}

function AppearancePanel() {
  const { settings, update } = useSettings();
  const a = settings.appearance;
  return (
    <SettingsSection title={t("settings.appearance")}>
      <SettingRow label={t("settings.language")} htmlFor="appearance-language">
        <SelectField id="appearance-language" value={a.language} options={[{ value: "en", label: "English" }, { value: "zh", label: "中文" }]} onChange={(v) => { update("appearance", { language: v as never }); setLanguage(v as never); }} />
      </SettingRow>
    </SettingsSection>
  );
}

function ModelProviderPanel() {
  const { settings, update } = useSettings();
  const navigate = useNavigate();
  const m = settings.model_provider;
  const [testResult, setTestResult] = useState<"idle" | "ok" | "fail">("idle");
  const [testMsg, setTestMsg] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [models, setModels] = useState<DiscoveredModel[]>([]);
  const selectedProvider = isKnownProvider(m.provider) ? m.provider : "openai";
  const preset = presetFor(selectedProvider);
  const safeModel = (value: string, fallback: string) => {
    const trimmed = String(value || "").trim();
    return trimmed && !trimmed.toLowerCase().includes("mock") ? trimmed : fallback;
  };
  const defaultModel = safeModel(m.default_model, preset.defaultModel);
  const fastModel = safeModel(m.fast_model, preset.fastModel);
  const reasoningModel = safeModel(m.reasoning_model, preset.reasoningModel);
  const structuredModel = safeModel(m.structured_output_model, preset.structuredModel);
  const modelOptions = [
    ...models,
    { id: defaultModel },
    { id: fastModel },
    { id: reasoningModel },
    { id: structuredModel },
  ]
    .filter((item) => item.id && !item.id.toLowerCase().includes("mock"))
    .filter((item, index, all) => all.findIndex((candidate) => candidate.id === item.id) === index)
    .map((item) => ({ value: item.id, label: item.display_name || item.id }));

  function configFromCurrent(patch: Partial<typeof m> = {}) {
    const next = { ...m, ...patch };
    const p = presetFor(isKnownProvider(next.provider) ? next.provider : selectedProvider);
    return { provider_id: "desktop", kind: p.kind, base_url: next.base_url || p.baseUrl, api_key: next.api_key, default_model: safeModel(next.default_model, p.defaultModel), fast_model: safeModel(next.fast_model, p.fastModel), reasoning_model: safeModel(next.reasoning_model, p.reasoningModel), structured_output_model: safeModel(next.structured_output_model, p.structuredModel), max_tokens: next.max_tokens || p.maxTokens, temperature: next.temperature, timeout_ms: next.request_timeout_ms, max_cost_per_task_usd: next.max_cost_per_task_usd };
  }

  function changeProvider(value: string) {
    const nextProvider = value as ProviderType;
    const p = presetFor(nextProvider);
    update("model_provider", { provider: nextProvider, base_url: p.baseUrl, default_model: p.defaultModel, fast_model: p.fastModel, reasoning_model: p.reasoningModel, structured_output_model: p.structuredModel, max_tokens: p.maxTokens });
    setModels([]);
  }

  async function handleSaveAndTestConnection() {
    setTestResult("idle");
    setTestMsg("");
    try {
      const baseConfig = configFromCurrent();
      const r = await testModelConnection(baseConfig) as Record<string, unknown>;
      let discovered: DiscoveredModel[] = [];
      try {
        discovered = ((await discoverModels(baseConfig)) as { models?: DiscoveredModel[] }).models || [];
        setModels(discovered);
      } catch {}
      const roles = chooseModelRoles(discovered, preset);
      const finalPatch = { default_model: roles.default_model, fast_model: roles.fast_model, reasoning_model: roles.reasoning_model, structured_output_model: roles.structured_output_model, max_tokens: roles.max_tokens };
      update("model_provider", finalPatch);
      await updateModelConfig(configFromCurrent(finalPatch));
      try {
        await ensureWorkspaceDefaults();
      } catch (e: unknown) {
        throw new Error(`Workspace bootstrap failed: ${e instanceof Error ? e.message : String(e)}`);
      }
      saveSettingsSnapshot({ ...settings, model_provider: { ...settings.model_provider, ...finalPatch } });
      markModelProviderConfigured();
      setTestResult("ok");
      setTestMsg(`${r.model || "ok"} | ${r.latency_ms || ""}ms`);
    } catch (e: unknown) {
      setTestResult("fail");
      setTestMsg(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <SettingsSection title={t("settings.model_provider")}>
      <SettingRow label={t("settings.provider")} htmlFor="provider-select"><SelectField id="provider-select" value={selectedProvider} options={providerOptions()} onChange={changeProvider} /></SettingRow>
      <SettingRow label={t("settings.api_key")} desc={t("settings.api_key_warning")} htmlFor="provider-api-key"><PasswordField id="provider-api-key" value={m.api_key} onChange={(v) => update("model_provider", { api_key: v })} /></SettingRow>
      <SettingRow label={t("settings.default_model")} htmlFor="provider-default-model"><SelectField id="provider-default-model" value={defaultModel} options={modelOptions} onChange={(v) => update("model_provider", { default_model: v })} /></SettingRow>
      <SettingRow label={t("settings.fast_model")} htmlFor="provider-fast-model"><SelectField id="provider-fast-model" value={fastModel} options={modelOptions} onChange={(v) => update("model_provider", { fast_model: v })} /></SettingRow>
      <SettingRow label={t("settings.reasoning_model")} htmlFor="provider-reasoning-model"><SelectField id="provider-reasoning-model" value={reasoningModel} options={modelOptions} onChange={(v) => update("model_provider", { reasoning_model: v })} /></SettingRow>
      <SettingRow label={t("settings.structured_model")} htmlFor="provider-structured-model"><SelectField id="provider-structured-model" value={structuredModel} options={modelOptions} onChange={(v) => update("model_provider", { structured_output_model: v })} /></SettingRow>
      <SettingRow label={t("settings.connection")}>
        <div className="flex items-center gap-2 flex-wrap">
          <button onClick={handleSaveAndTestConnection} className="px-3 py-1.5 text-xs rounded-md border" style={{ borderColor: "var(--accent)", color: "var(--accent)" }}>{t("settings.test_discover")}</button>
          {testResult === "ok" && <span className="text-xs" style={{ color: "var(--green)" }}>{t("settings.saved_connected")}: {testMsg}</span>}
          {testResult === "fail" && <span className="text-xs" style={{ color: "var(--red)" }}>{t("settings.connection_failed")}: {testMsg}</span>}
          {testResult === "ok" && <button onClick={() => navigate("/")} className="px-3 py-1.5 text-xs rounded-md text-white" style={{ background: "var(--accent)" }}>{t("settings.continue_to_chat")}</button>}
        </div>
      </SettingRow>
      <SettingRow label={t("settings.advanced_toggle")}><button type="button" onClick={() => setAdvanced((v) => !v)} className="px-3 py-1.5 text-xs rounded-md border">{advanced ? t("settings.hide_advanced") : t("settings.advanced_toggle")}</button></SettingRow>
      {advanced && (
        <>
          <SettingRow label={t("settings.base_url")} htmlFor="provider-base-url"><TextField id="provider-base-url" monospace value={m.base_url || preset.baseUrl} onChange={(v) => update("model_provider", { base_url: v })} /></SettingRow>
          <SettingRow label={t("settings.max_tokens")} htmlFor="provider-max-tokens"><NumberField id="provider-max-tokens" value={m.max_tokens || preset.maxTokens} onChange={(v) => update("model_provider", { max_tokens: v })} min={1} max={1000000} step={1024} /></SettingRow>
          <SettingRow label={t("settings.max_cost_per_task")}><NumberField value={m.max_cost_per_task_usd} onChange={(v) => update("model_provider", { max_cost_per_task_usd: v })} min={0} max={100} step={0.1} /></SettingRow>
        </>
      )}
    </SettingsSection>
  );
}

function AgentRuntimePanel() { return <SettingsSection title={t("settings.agent_runtime")}><div /></SettingsSection>; }
function GovernancePanel() { return <SettingsSection title={t("settings.governance")}><div /></SettingsSection>; }
function RiskGatePanel() { return <SettingsSection title={t("settings.risk_gate")}><div /></SettingsSection>; }
function CognitiveCustomsPanel() { return <SettingsSection title={t("settings.cognitive_customs")}><div /></SettingsSection>; }
function PolicyEnginePanel() { const { settings, update } = useSettings(); const p = settings.policy_engine; const selectedPolicyEngine: PolicyEngineType = p.policy_engine === "custom" ? "custom" : "opa"; return <SettingsSection title={t("settings.policy_engine")}><SettingRow label={t("settings.policy_engine_choice")}><SelectField value={selectedPolicyEngine} options={[{ value: "opa", label: "OPA" }, { value: "custom", label: t("settings.policy_custom") }]} onChange={(v) => update("policy_engine", { policy_engine: v as PolicyEngineType })} /></SettingRow></SettingsSection>; }
function PrivacyPanel() { return <SettingsSection title={t("settings.privacy")}><div /></SettingsSection>; }
function DeveloperPanel() { return <SettingsSection title={t("settings.developer")}><SettingRow label={t("settings.reset_local_ui")}><button className="px-3 py-1.5 text-xs rounded-md border">{t("settings.reset_button")}</button></SettingRow></SettingsSection>; }

function DataManagementPanel() {
  useLanguage();
  const [coevoHome, setCoevoHome] = useState<string>(t("settings.loading"));
  useEffect(() => {
    (async () => {
      try {
        const invoke = getTauriInvoke();
        if (invoke) setCoevoHome(await invoke<string>("get_coevo_home"));
        else setCoevoHome(t("settings.web_mode_home"));
      } catch { setCoevoHome("~/.coevo"); }
    })();
  }, []);
  return (
    <SettingsSection title={t("settings.data_management")}>
      <SettingRow label="COEVO_HOME"><div className="text-xs font-mono">{coevoHome}</div></SettingRow>
      <SettingRow label={t("settings.api_base")}><div className="text-xs font-mono">{getApiBase()}</div></SettingRow>
    </SettingsSection>
  );
}
