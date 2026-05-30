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
import { t, setLanguage } from "../settings/i18n";
import { chooseModelRoles, isKnownProvider, providerOptions, presetFor, type DiscoveredModel } from "../settings/modelPresets";
import { markModelProviderConfigured } from "../settings/onboarding";
import type { PolicyEngineType, ProviderType } from "../settings/types";

const CATEGORIES = ["general","appearance","model_provider","agent_runtime","governance","risk_gate","cognitive_customs","policy_engine","privacy","developer","data_management"];

export default function Settings() {
  return (
    <Routes>
      <Route index element={<Navigate to="general" replace />} />
      {CATEGORIES.map((cat) => (
        <Route key={cat} path={cat} element={<SettingsCategory cat={cat as keyof typeof panels} />} />
      ))}
    </Routes>
  );
}

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

function SettingsCategory({ cat }: { cat: string }) {
  const Panel = panels[cat] || GeneralPanel;
  return <SettingsLayout section={cat as never} content={<Panel />} />;
}

function GeneralPanel() {
  const { settings, update } = useSettings();
  const g = settings.general;
  return (
    <SettingsSection title={t("settings.general")}>
      <SettingRow label={t("settings.default_home")} desc={t("settings.default_home_desc")}>
        <SelectField value={g.default_home} options={[
          {value:"mission-chat",label:"Mission Chat"},{value:"dashboard",label:"Dashboard"}
        ]} onChange={(v)=>update("general",{default_home:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.startup_behavior")} desc={t("settings.startup_behavior_desc")}>
        <SelectField value={g.startup_behavior} options={[
          {value:"last-task",label:t("settings.open_last_task")},{value:"new-task",label:t("settings.open_new_task")}
        ]} onChange={(v)=>update("general",{startup_behavior:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.default_mission_mode")}>
        <SelectField value={g.default_mission_mode} options={[
          {value:"auto",label:t("settings.auto")},{value:"readonly",label:t("settings.readonly")},{value:"collaborative",label:t("settings.collaborative")},{value:"high-risk",label:t("settings.high_risk")}
        ]} onChange={(v)=>update("general",{default_mission_mode:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.autosave_drafts")}>
        <ToggleField checked={g.autosave_drafts} onChange={(v)=>update("general",{autosave_drafts:v})} />
      </SettingRow>
      <SettingRow label={t("settings.time_format")}>
        <SelectField value={g.time_format} options={[{value:"12h",label:"12h"},{value:"24h",label:"24h"}]} onChange={(v)=>update("general",{time_format:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.region")}>
        <TextField value={g.region} onChange={(v)=>update("general",{region:v})} />
      </SettingRow>
    </SettingsSection>
  );
}

function AppearancePanel() {
  const { settings, update } = useSettings();
  const a = settings.appearance;
  return (
    <SettingsSection title={t("settings.appearance")}>
      <SettingRow label={t("settings.language")}>
        <SelectField value={a.language} options={[
          {value:"en",label:"English"},{value:"zh",label:"Chinese"}
        ]} onChange={(v)=>{update("appearance",{language:v as never});setLanguage(v as never);}} />
      </SettingRow>
      <SettingRow label={t("settings.theme")}>
        <SelectField value={a.theme} options={[
          {value:"light",label:"Light"},{value:"dark",label:"Dark"},{value:"system",label:"System"}
        ]} onChange={(v)=>update("appearance",{theme:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.font_size")}>
        <SelectField value={a.font_size} options={[
          {value:"small",label:"Small"},{value:"normal",label:"Normal"},{value:"large",label:"Large"},{value:"extra-large",label:"Extra Large"}
        ]} onChange={(v)=>update("appearance",{font_size:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.density")}>
        <SelectField value={a.density} options={[
          {value:"comfortable",label:"Comfortable"},{value:"compact",label:"Compact"}
        ]} onChange={(v)=>update("appearance",{density:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.sidebar_mode")}>
        <SelectField value={a.sidebar_mode} options={[
          {value:"icon+text",label:"Icon + Text"},{value:"icon-only",label:"Icon Only"}
        ]} onChange={(v)=>update("appearance",{sidebar_mode:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.reduce_motion")}>
        <ToggleField checked={a.reduce_motion} onChange={(v)=>update("appearance",{reduce_motion:v})} />
      </SettingRow>
      <SettingRow label={t("settings.high_contrast")}>
        <ToggleField checked={a.high_contrast} onChange={(v)=>update("appearance",{high_contrast:v})} />
      </SettingRow>
    </SettingsSection>
  );
}

function ModelProviderPanel() {
  const { settings, update } = useSettings();
  const navigate = useNavigate();
  const m = settings.model_provider;
  const [testResult, setTestResult] = useState<"idle"|"ok"|"fail">("idle");
  const [testMsg, setTestMsg] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [models, setModels] = useState<DiscoveredModel[]>([]);
  const selectedProvider = isKnownProvider(m.provider) ? m.provider : "openai";
  const preset = presetFor(selectedProvider);
  const effectiveModels = isKnownProvider(m.provider) ? m : {
    ...m,
    default_model: preset.defaultModel,
    fast_model: preset.fastModel,
    reasoning_model: preset.reasoningModel,
    structured_output_model: preset.structuredModel,
    max_tokens: preset.maxTokens,
  };
  const modelOptions = (models.length ? models : [
    { id: effectiveModels.default_model || preset.defaultModel, display_name: effectiveModels.default_model || preset.defaultModel },
    { id: effectiveModels.fast_model || preset.fastModel, display_name: effectiveModels.fast_model || preset.fastModel },
    { id: effectiveModels.reasoning_model || preset.reasoningModel, display_name: effectiveModels.reasoning_model || preset.reasoningModel },
    { id: effectiveModels.structured_output_model || preset.structuredModel, display_name: effectiveModels.structured_output_model || preset.structuredModel },
  ]).filter((item, index, arr) => item.id && arr.findIndex((x) => x.id === item.id) === index)
    .map((item) => ({ value: item.id, label: item.display_name || item.id }));

  function configFromCurrent(patch: Partial<typeof m> = {}) {
    const next = { ...m, ...patch };
    const p = presetFor(next.provider);
    return {
      provider_id: "desktop",
      kind: p.kind,
      base_url: next.base_url || p.baseUrl,
      api_key: next.api_key,
      default_model: next.default_model || p.defaultModel,
      fast_model: next.fast_model || p.fastModel,
      reasoning_model: next.reasoning_model || p.reasoningModel,
      structured_output_model: next.structured_output_model || p.structuredModel,
      max_tokens: next.max_tokens || p.maxTokens,
      temperature: next.temperature,
      timeout_ms: next.request_timeout_ms,
      max_cost_per_task_usd: next.max_cost_per_task_usd,
    };
  }

  function changeProvider(value: string) {
    const nextProvider = value as ProviderType;
    const nextPreset = presetFor(nextProvider);
    update("model_provider", {
      provider: nextProvider,
      base_url: nextPreset.baseUrl,
      default_model: nextPreset.defaultModel,
      fast_model: nextPreset.fastModel,
      reasoning_model: nextPreset.reasoningModel,
      structured_output_model: nextPreset.structuredModel,
      max_tokens: nextPreset.maxTokens,
    });
    setModels([]);
  }

  async function handleSaveAndTestConnection() {
    setTestResult("idle");
    setTestMsg("");
    const baseConfig = configFromCurrent();
    try {
      const r = await testModelConnection(baseConfig) as Record<string,unknown>;
      let discovered: DiscoveredModel[] = [];
      let discoveryNote = "";
      try {
        const discovery = await discoverModels(baseConfig) as { models?: DiscoveredModel[] };
        discovered = discovery.models || [];
        setModels(discovered);
      } catch (e: unknown) {
        discoveryNote = ` Model discovery unavailable; using recommended defaults. ${e instanceof Error ? e.message : String(e)}`;
      }
      const roles = chooseModelRoles(discovered, preset);
      const finalPatch = {
        default_model: roles.default_model,
        fast_model: roles.fast_model,
        reasoning_model: roles.reasoning_model,
        structured_output_model: roles.structured_output_model,
        max_tokens: roles.max_tokens,
      };
      update("model_provider", finalPatch);
      const finalConfig = configFromCurrent(finalPatch);
      await updateModelConfig(finalConfig);
      try {
        await ensureWorkspaceDefaults();
      } catch (e: unknown) {
        throw new Error(`Workspace bootstrap failed after model connection succeeded: ${e instanceof Error ? e.message : String(e)}`);
      }
      saveSettingsSnapshot({ ...settings, model_provider: { ...settings.model_provider, ...finalPatch } });
      markModelProviderConfigured();
      setTestResult("ok");
      setTestMsg(`${r.model || "ok"} | ${r.latency_ms}ms | ${r.provider_kind || ""}.${discoveryNote}`);
    } catch(e: unknown) {
      setTestResult("fail");
      setTestMsg(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <SettingsSection title={t("settings.model_provider")}>
      <SettingRow label={t("settings.provider")}>
        <SelectField value={selectedProvider} options={providerOptions()} onChange={changeProvider} />
      </SettingRow>
      <SettingRow label={t("settings.api_key")} desc={t("settings.api_key_warning")}>
        <PasswordField value={m.api_key} onChange={(v)=>update("model_provider",{api_key:v})} />
      </SettingRow>
      <SettingRow label={t("settings.default_model")}>
        <SelectField value={effectiveModels.default_model || preset.defaultModel} options={modelOptions} onChange={(v)=>update("model_provider",{default_model:v})} />
      </SettingRow>
      <SettingRow label={t("settings.fast_model")}>
        <SelectField value={effectiveModels.fast_model || preset.fastModel} options={modelOptions} onChange={(v)=>update("model_provider",{fast_model:v})} />
      </SettingRow>
      <SettingRow label={t("settings.reasoning_model")}>
        <SelectField value={effectiveModels.reasoning_model || preset.reasoningModel} options={modelOptions} onChange={(v)=>update("model_provider",{reasoning_model:v})} />
      </SettingRow>
      <SettingRow label="Structured Output Model">
        <SelectField value={effectiveModels.structured_output_model || preset.structuredModel} options={modelOptions} onChange={(v)=>update("model_provider",{structured_output_model:v})} />
      </SettingRow>
      <SettingRow label="Connection">
        <div className="flex items-center gap-2 flex-wrap">
          <button
            onClick={handleSaveAndTestConnection}
            className="px-3 py-1.5 text-xs rounded-md border transition-colors"
            style={{borderColor:"var(--accent)",color:"var(--accent)"}}
          >
            Connect
          </button>
          {testResult==="ok" && <span className="text-xs" style={{color:"var(--green)"}}>Saved and connected: {testMsg}</span>}
          {testResult==="fail" && <span className="text-xs" style={{color:"var(--red)"}}>Connection failed: {testMsg}</span>}
          {testResult==="ok" && (
            <button
              onClick={() => navigate("/")}
              className="px-3 py-1.5 text-xs rounded-md text-white transition-colors"
              style={{background:"var(--accent)"}}
            >
              Continue to Mission Chat
            </button>
          )}
        </div>
      </SettingRow>
      <SettingRow label="Advanced">
        <button
          type="button"
          onClick={() => setAdvanced((v) => !v)}
          className="px-3 py-1.5 text-xs rounded-md border"
          style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}
        >
          {advanced ? "Hide Advanced" : "Show Advanced"}
        </button>
      </SettingRow>
      {advanced && (
        <>
          <SettingRow label={t("settings.base_url")}>
            <TextField monospace value={m.base_url || preset.baseUrl} onChange={(v)=>update("model_provider",{base_url:v})} />
          </SettingRow>
          <SettingRow label={t("settings.max_tokens")}>
            <NumberField value={m.max_tokens} onChange={(v)=>update("model_provider",{max_tokens:v})} min={1} max={128000} />
          </SettingRow>
          <SettingRow label={t("settings.temperature")}>
            <NumberField value={m.temperature} onChange={(v)=>update("model_provider",{temperature:v})} min={0} max={2} step={0.1} />
          </SettingRow>
          <SettingRow label={t("settings.request_timeout_ms")}>
            <NumberField value={m.request_timeout_ms} onChange={(v)=>update("model_provider",{request_timeout_ms:v})} min={1000} max={120000} />
          </SettingRow>
          <SettingRow label="Max Cost/Task (USD)">
            <NumberField value={m.max_cost_per_task_usd} onChange={(v)=>update("model_provider",{max_cost_per_task_usd:v})} min={0} max={100} step={0.1} />
          </SettingRow>
        </>
      )}
    </SettingsSection>
  );
}

function AgentRuntimePanel() {
  const { settings, update } = useSettings();
  const a = settings.agent_runtime;
  return (
    <SettingsSection title={t("settings.agent_runtime")} desc="coevo will never create unconstrained agents. It selects from Agent Registry. Task Agent Instances are short-lived. Ephemeral Sub-Agents default to low privilege - Hypothesis/Suggestion only.">
      <SettingRow label="Default Agent Registry">
        <TextField monospace value={a.default_agent_registry} onChange={(v)=>update("agent_runtime",{default_agent_registry:v})} />
      </SettingRow>
      <SettingRow label="Allow Task Agent Instance">
        <ToggleField checked={a.allow_task_agent_instance} onChange={(v)=>update("agent_runtime",{allow_task_agent_instance:v})} />
      </SettingRow>
      <SettingRow label="Allow Ephemeral Sub-Agent">
        <ToggleField checked={a.allow_ephemeral_sub_agent} onChange={(v)=>update("agent_runtime",{allow_ephemeral_sub_agent:v})} />
      </SettingRow>
      <SettingRow label="Ephemeral Agent TTL (min)">
        <NumberField value={a.ephemeral_agent_ttl_minutes} onChange={(v)=>update("agent_runtime",{ephemeral_agent_ttl_minutes:v})} min={1} max={1440} />
      </SettingRow>
      <SettingRow label="Max Agents Per Task">
        <NumberField value={a.max_agents_per_task} onChange={(v)=>update("agent_runtime",{max_agents_per_task:v})} min={1} max={20} />
      </SettingRow>
      <SettingRow label="Max Hops">
        <NumberField value={a.max_hops} onChange={(v)=>update("agent_runtime",{max_hops:v})} min={1} max={20} />
      </SettingRow>
      <SettingRow label="Allow Cross-Org A2A">
        <ToggleField checked={a.allow_cross_org_a2a} onChange={(v)=>update("agent_runtime",{allow_cross_org_a2a:v})} />
      </SettingRow>
      <SettingRow label="Default Tool Scope">
        <TextField monospace value={a.default_tool_scope} onChange={(v)=>update("agent_runtime",{default_tool_scope:v})} />
      </SettingRow>
    </SettingsSection>
  );
}

function GovernancePanel() {
  const { settings, update } = useSettings();
  const g = settings.governance;
  return (
    <SettingsSection title={t("settings.governance")}>
      <SettingRow label="Auto-Confirm Green Track">
        <ToggleField checked={g.auto_confirm_green_track} onChange={(v)=>update("governance",{auto_confirm_green_track:v})} />
      </SettingRow>
      <SettingRow label="Yellow Approval Mode">
        <SelectField value={g.yellow_approval_mode} options={[
          {value:"negative_consent",label:"Negative Consent"},{value:"explicit_approval",label:"Explicit Approval"}
        ]} onChange={(v)=>update("governance",{yellow_approval_mode:v as never})} />
      </SettingRow>
      <SettingRow label="Negative Consent Timeout (s)">
        <NumberField value={g.negative_consent_timeout_seconds} onChange={(v)=>update("governance",{negative_consent_timeout_seconds:v})} min={30} max={3600} />
      </SettingRow>
      <SettingRow label="Red Requires MFA">
        <ToggleField checked={g.red_requires_mfa} onChange={(v)=>update("governance",{red_requires_mfa:v})} />
      </SettingRow>
      <SettingRow label="Human Override Enabled">
        <ToggleField checked={g.human_override_enabled} onChange={(v)=>update("governance",{human_override_enabled:v})} />
      </SettingRow>
      <SettingRow label="One-Vote Blocker Roles">
        <TextField value={g.one_vote_blocker_roles} onChange={(v)=>update("governance",{one_vote_blocker_roles:v})} />
      </SettingRow>
      <SettingRow label="Responsibility Anchor Required">
        <ToggleField checked={g.responsibility_anchor_required} onChange={(v)=>update("governance",{responsibility_anchor_required:v})} />
      </SettingRow>
    </SettingsSection>
  );
}

function RiskGatePanel() {
  const { settings, update } = useSettings();
  const r = settings.risk_gate;
  return (
    <SettingsSection title={t("settings.risk_gate")}>
      <SettingRow label="Green Threshold"><NumberField value={r.green_threshold} onChange={(v)=>update("risk_gate",{green_threshold:v})} min={0} max={1} step={0.1} /></SettingRow>
      <SettingRow label="Yellow Threshold"><NumberField value={r.yellow_threshold} onChange={(v)=>update("risk_gate",{yellow_threshold:v})} min={0} max={1} step={0.1} /></SettingRow>
      <SettingRow label="Red Threshold"><NumberField value={r.red_threshold} onChange={(v)=>update("risk_gate",{red_threshold:v})} min={0} max={1} step={0.1} /></SettingRow>
      <SettingRow label="ActionRisk w1 (Blast Radius)"><NumberField value={r.action_risk_weight_blast_radius} onChange={(v)=>update("risk_gate",{action_risk_weight_blast_radius:v})} min={0} max={1} step={0.05} /></SettingRow>
      <SettingRow label="ActionRisk w2 (Irreversibility)"><NumberField value={r.action_risk_weight_irreversibility} onChange={(v)=>update("risk_gate",{action_risk_weight_irreversibility:v})} min={0} max={1} step={0.05} /></SettingRow>
      <SettingRow label="InactionRisk w5 (Service Impact)"><NumberField value={r.inaction_risk_weight_service_impact} onChange={(v)=>update("risk_gate",{inaction_risk_weight_service_impact:v})} min={0} max={1} step={0.05} /></SettingRow>
      <SettingRow label="Emergency Lease Enabled"><ToggleField checked={r.emergency_lease_enabled} onChange={(v)=>update("risk_gate",{emergency_lease_enabled:v})} /></SettingRow>
      <SettingRow label="Default Lease Duration (s)"><NumberField value={r.default_lease_duration_seconds} onChange={(v)=>update("risk_gate",{default_lease_duration_seconds:v})} min={60} max={3600} /></SettingRow>
      <SettingRow label="Default Lease Budget"><NumberField value={r.default_lease_budget} onChange={(v)=>update("risk_gate",{default_lease_budget:v})} min={1} max={20} /></SettingRow>
      <SettingRow label="Require Dual-Sign for Emergency"><ToggleField checked={r.require_dual_sign_for_emergency} onChange={(v)=>update("risk_gate",{require_dual_sign_for_emergency:v})} /></SettingRow>
    </SettingsSection>
  );
}

function CognitiveCustomsPanel() {
  const { settings, update } = useSettings();
  const c = settings.cognitive_customs;
  return (
    <SettingsSection title={t("settings.cognitive_customs")}>
      <SettingRow label="Default Fact TTL (s)"><NumberField value={c.default_fact_ttl_seconds} onChange={(v)=>update("cognitive_customs",{default_fact_ttl_seconds:v})} min={60} max={86400} /></SettingRow>
      <SettingRow label="Require Provenance for Fact"><ToggleField checked={c.require_provenance_for_fact} onChange={(v)=>update("cognitive_customs",{require_provenance_for_fact:v})} /></SettingRow>
      <SettingRow label="Allow Hypothesis Auto-Promotion"><ToggleField checked={c.allow_hypothesis_auto_promotion} onChange={(v)=>update("cognitive_customs",{allow_hypothesis_auto_promotion:v})} /></SettingRow>
      <SettingRow label="Fact Promotion Evidence Level">
        <SelectField value={c.fact_promotion_evidence_level} options={[
          {value:"unit_tests_passing",label:"Unit Tests Passing"},{value:"integration_verified",label:"Integration Verified"},{value:"manual_review",label:"Manual Review"}
        ]} onChange={(v)=>update("cognitive_customs",{fact_promotion_evidence_level:v as never})} />
      </SettingRow>
      <SettingRow label="Revoked Fact Invalidation">
        <SelectField value={c.revoked_fact_invalidation_strategy} options={[
          {value:"direct_only",label:"Direct Only"},{value:"transitive",label:"Transitive"}
        ]} onChange={(v)=>update("cognitive_customs",{revoked_fact_invalidation_strategy:v as never})} />
      </SettingRow>
      <SettingRow label="Replay Mode Blocks Fact Write"><ToggleField checked={c.replay_mode_blocks_fact_write} onChange={(v)=>update("cognitive_customs",{replay_mode_blocks_fact_write:v})} /></SettingRow>
    </SettingsSection>
  );
}

function PolicyEnginePanel() {
  const { settings, update } = useSettings();
  const p = settings.policy_engine;
  const selectedPolicyEngine: PolicyEngineType = p.policy_engine === "custom" ? "custom" : "opa";
  return (
    <SettingsSection title={t("settings.policy_engine")} desc="Protocol layer does not bind to OPA. OPA is a reference implementation profile.">
      <SettingRow label="Policy Engine">
        <SelectField value={selectedPolicyEngine} options={[
          {value:"opa",label:"OPA"},{value:"custom",label:"Custom"}
        ]} onChange={(v)=>update("policy_engine",{policy_engine:v as PolicyEngineType})} />
      </SettingRow>
      <SettingRow label="Policy Bundle Path"><TextField monospace value={p.policy_bundle_path} onChange={(v)=>update("policy_engine",{policy_bundle_path:v})} /></SettingRow>
      <SettingRow label="Policy Version"><TextField monospace value={p.policy_version} onChange={(v)=>update("policy_engine",{policy_version:v})} /></SettingRow>
      <SettingRow label="Decision Log Enabled"><ToggleField checked={p.decision_log_enabled} onChange={(v)=>update("policy_engine",{decision_log_enabled:v})} /></SettingRow>
      <SettingRow label="Policy Simulation Enabled"><ToggleField checked={p.policy_simulation_enabled} onChange={(v)=>update("policy_engine",{policy_simulation_enabled:v})} /></SettingRow>
      <SettingRow label="Policy Diff Enabled"><ToggleField checked={p.policy_diff_enabled} onChange={(v)=>update("policy_engine",{policy_diff_enabled:v})} /></SettingRow>
      <SettingRow label="Health Check URL"><TextField monospace value={p.health_check_url} onChange={(v)=>update("policy_engine",{health_check_url:v})} /></SettingRow>
    </SettingsSection>
  );
}

function PrivacyPanel() {
  const { settings, update } = useSettings();
  const p = settings.privacy;
  return (
    <SettingsSection title={t("settings.privacy")}>
      <SettingRow label="Log Retention (days)"><NumberField value={p.log_retention_days} onChange={(v)=>update("privacy",{log_retention_days:v})} min={1} max={365} /></SettingRow>
      <SettingRow label="Store Full Prompts"><ToggleField checked={p.store_full_prompts} onChange={(v)=>update("privacy",{store_full_prompts:v})} /></SettingRow>
      <SettingRow label="Store Model Outputs"><ToggleField checked={p.store_model_outputs} onChange={(v)=>update("privacy",{store_model_outputs:v})} /></SettingRow>
      <SettingRow label="PII Redaction Enabled"><ToggleField checked={p.pii_redaction_enabled} onChange={(v)=>update("privacy",{pii_redaction_enabled:v})} /></SettingRow>
      <SettingRow label="Local Database Path"><TextField monospace value={p.local_database_path} onChange={(v)=>update("privacy",{local_database_path:v})} /></SettingRow>
      <SettingRow label="Export Audit Log">
        <button className="px-3 py-1.5 text-xs rounded-md border" style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}
          onClick={()=>{const blob=new Blob(["[]"],{type:"application/json"});const a=document.createElement("a");a.href=URL.createObjectURL(blob);a.download="coevo-audit.json";a.click();}}>
          Export
        </button>
      </SettingRow>
      <SettingRow label="Clear Local Data">
        <button className="px-3 py-1.5 text-xs rounded-md border" style={{borderColor:"rgba(239,68,68,0.4)",color:"var(--red)"}}
          onClick={()=>{if(confirm("Clear all local data?")){localStorage.clear();window.location.reload();}}}>
          Clear
        </button>
      </SettingRow>
    </SettingsSection>
  );
}

function DataManagementPanel() {
  const [coevoHome, setCoevoHome] = useState<string>("Loading...");

  useEffect(() => {
    (async () => {
      try {
        const invoke = getTauriInvoke();
        if (invoke) {
          setCoevoHome(await invoke<string>("get_coevo_home"));
        } else {
          setCoevoHome("~/.coevo (web mode)");
        }
      } catch { setCoevoHome("~/.coevo"); }
    })();
  }, []);

  async function tauriCmd(name: string) {
    try {
      const invoke = getTauriInvoke();
      if (invoke) {
        await invoke(name);
        return;
      }
    } catch (e) {
      alert(`Command ${name} failed: ${e instanceof Error ? e.message : String(e)}`);
      return;
    }
    alert(`${name} is available in Tauri desktop mode.`);
  }

  return (
    <SettingsSection title="Data Management">
      <SettingRow label="COEVO_HOME"><div className="text-xs font-mono" style={{color:"var(--text-secondary)"}}>{coevoHome}</div></SettingRow>
      <SettingRow label="Database"><div className="text-xs font-mono" style={{color:"var(--text-secondary)"}}>{`${coevoHome}\\data\\coevo.db`}</div></SettingRow>
      <SettingRow label="Logs"><div className="text-xs font-mono" style={{color:"var(--text-secondary)"}}>{`${coevoHome}\\logs\\`}</div></SettingRow>
      <SettingRow label="Runtime"><div className="text-xs font-mono" style={{color:"var(--text-secondary)"}}>{`${coevoHome}\\runtime\\server.port, server.pid`}</div></SettingRow>
      <SettingRow label="API Base"><div className="text-xs font-mono" style={{color:"var(--text-secondary)"}}>{getApiBase()}</div></SettingRow>
      <SettingRow label="Actions">
        <div className="flex gap-2">
          <button onClick={() => tauriCmd("open_logs_dir")} className="px-3 py-1.5 text-xs rounded-md border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>Open Logs</button>
          <button onClick={() => tauriCmd("open_coevo_dir")} className="px-3 py-1.5 text-xs rounded-md border" style={{borderColor:"var(--accent)",color:"var(--accent)"}}>Open coevo Folder</button>
        </div>
      </SettingRow>
    </SettingsSection>
  );
}

function DeveloperPanel() {
  const { settings, update } = useSettings();
  const d = settings.developer;
  return (
    <SettingsSection title="Developer Mode">
      <SettingRow label="API Base URL">
        <TextField monospace value={d.api_base_url} onChange={(v)=>update("developer",{api_base_url:v})} />
      </SettingRow>
      <SettingRow label="OpenAPI URL">
        <TextField monospace value={d.openapi_url} onChange={(v)=>update("developer",{openapi_url:v})} />
      </SettingRow>
      <SettingRow label="Debug Logs Enabled">
        <ToggleField checked={d.debug_logs_enabled} onChange={(v)=>update("developer",{debug_logs_enabled:v})} />
      </SettingRow>
      <SettingRow label="Show Traceparent">
        <ToggleField checked={d.show_traceparent} onChange={(v)=>update("developer",{show_traceparent:v})} />
      </SettingRow>
      <SettingRow label="Show Raw JSON Panels">
        <ToggleField checked={d.show_raw_json_panels} onChange={(v)=>update("developer",{show_raw_json_panels:v})} />
      </SettingRow>
      <SettingRow label="Feature Flags">
        <TextField value={d.feature_flags} onChange={(v)=>update("developer",{feature_flags:v})} />
      </SettingRow>
      <SettingRow label="Reset Local UI State">
        <button className="px-3 py-1.5 text-xs rounded-md border" style={{borderColor:"rgba(239,68,68,0.4)",color:"var(--red)"}}
          onClick={()=>{if(confirm("Reset local UI state?")){localStorage.clear();alert("Local UI state reset.");}}}>
          Reset
        </button>
      </SettingRow>
    </SettingsSection>
  );
}
