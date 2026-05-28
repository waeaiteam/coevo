import { Routes, Route, Navigate, useLocation, useNavigate } from "react-router-dom";
import SettingsLayout from "../components/SettingsLayout";
import SettingsSection from "../components/SettingsSection";
import SettingRow from "../components/SettingRow";
import SelectField from "../components/SelectField";
import ToggleField from "../components/ToggleField";
import TextField from "../components/TextField";
import NumberField from "../components/NumberField";
import PasswordField from "../components/PasswordField";
import { useSettings } from "../hooks/useSettings";
import { t, setLanguage, getLanguage } from "../settings/i18n";
import { useState } from "react";
import { updateModelConfig, testModelConnection } from "../api/client";

const CATEGORIES = ["general","appearance","model_provider","agent_runtime","governance","risk_gate","cognitive_customs","policy_engine","privacy","developer"];

export default function Settings() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/settings/general" replace />} />
      {CATEGORIES.map((cat) => (
        <Route key={cat} path={`/${cat}`} element={<SettingsCategory cat={cat as keyof typeof panels} />} />
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
};

function SettingsCategory({ cat }: { cat: string }) {
  const Panel = panels[cat] || GeneralPanel;
  return (
    <SettingsLayout section={cat as never} content={<Panel />} />
  );
}

/* ============ GENERAL ============ */
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

/* ============ APPEARANCE ============ */
function AppearancePanel() {
  const { settings, update } = useSettings();
  const a = settings.appearance;
  return (
    <SettingsSection title={t("settings.appearance")}>
      <SettingRow label={t("settings.language")}>
        <SelectField value={a.language} options={[
          {value:"en",label:"English"},{value:"zh",label:"中文"}
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

/* ============ MODEL PROVIDER ============ */
function ModelProviderPanel() {
  const { settings, update } = useSettings();
  const m = settings.model_provider;
  const [testResult, setTestResult] = useState<"idle"|"ok"|"fail">("idle");
  const [testMsg, setTestMsg] = useState("");

  async function handleTestConnection() {
    setTestResult("idle"); setTestMsg("");
    const providerMap: Record<string,string> = {"mock":"Mock","openai-compatible":"OpenAICompatible","openai":"OpenAI","anthropic":"Anthropic","gemini":"Gemini","deepseek":"DeepSeek","ollama":"Ollama","local":"Local"};
    try {
      await updateModelConfig({provider_id: "desktop", kind: providerMap[m.provider]||"Mock", base_url: m.base_url, api_key: m.api_key, default_model: m.default_model, fast_model: m.fast_model, reasoning_model: m.reasoning_model, structured_output_model: m.structured_output_model, max_tokens: m.max_tokens, temperature: m.temperature, timeout_ms: m.request_timeout_ms, max_cost_per_task_usd: m.max_cost_per_task_usd});
      const r = await testModelConnection() as Record<string,unknown>;
      setTestResult("ok"); setTestMsg(`${r.model||"ok"} | ${r.latency_ms}ms | ${r.provider_kind||""}`);
      setTimeout(()=>setTestResult("idle"),4000);
    } catch(e: unknown) {
      setTestResult("fail");
      setTestMsg(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <SettingsSection title={t("settings.model_provider")}>
      <SettingRow label={t("settings.provider")}>
        <SelectField value={m.provider} options={[
          {value:"mock",label:"Mock / Local Test Provider"},{value:"openai-compatible",label:"OpenAI Compatible"},{value:"openai",label:"OpenAI"},{value:"anthropic",label:"Anthropic"},{value:"gemini",label:"Gemini"},{value:"deepseek",label:"DeepSeek"},{value:"ollama",label:"Ollama"},{value:"local",label:"Local"}
        ]} onChange={(v)=>update("model_provider",{provider:v as never})} />
      </SettingRow>
      <SettingRow label={t("settings.base_url")}>
        <TextField monospace value={m.base_url} onChange={(v)=>update("model_provider",{base_url:v})} />
      </SettingRow>
      <SettingRow label={t("settings.api_key")} desc={t("settings.api_key_warning")}>
        <PasswordField value={m.api_key} onChange={(v)=>update("model_provider",{api_key:v})} />
      </SettingRow>
      <SettingRow label={t("settings.default_model")}>
        <TextField monospace value={m.default_model} onChange={(v)=>update("model_provider",{default_model:v})} />
      </SettingRow>
      <SettingRow label={t("settings.fast_model")}>
        <TextField monospace value={m.fast_model} onChange={(v)=>update("model_provider",{fast_model:v})} />
      </SettingRow>
      <SettingRow label={t("settings.reasoning_model")}>
        <TextField monospace value={m.reasoning_model} onChange={(v)=>update("model_provider",{reasoning_model:v})} />
      </SettingRow>
      <SettingRow label="Structured Output Model">
        <TextField monospace value={m.structured_output_model} onChange={(v)=>update("model_provider",{structured_output_model:v})} />
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
      <SettingRow label={t("settings.test_connection")}>
        <div className="flex items-center gap-2">
          <button onClick={handleTestConnection}
            className="px-3 py-1.5 text-xs rounded-md border transition-colors"
            style={{borderColor:"var(--accent)",color:"var(--accent)"}}>
            {t("settings.test_connection")}
          </button>
          {testResult==="ok" && <span className="text-xs" style={{color:"var(--green)"}}>✓ {testMsg}</span>}
          {testResult==="fail" && <span className="text-xs" style={{color:"var(--red)"}}>✗ {testMsg}</span>}
        </div>
      </SettingRow>
    </SettingsSection>
  );
}

/* ============ AGENT RUNTIME ============ */
function AgentRuntimePanel() {
  const { settings, update } = useSettings();
  const a = settings.agent_runtime;
  return (
    <SettingsSection title={t("settings.agent_runtime")} desc="coevo will never create unconstrained agents. It selects from Agent Registry. Task Agent Instances are short-lived. Ephemeral Sub-Agents default to low privilege — Hypothesis/Suggestion only.">
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

/* ============ GOVERNANCE ============ */
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

/* ============ RISK GATE ============ */
function RiskGatePanel() {
  const { settings, update } = useSettings();
  const r = settings.risk_gate;
  return (
    <SettingsSection title={t("settings.risk_gate")}>
      <SettingRow label="Green Threshold">
        <NumberField value={r.green_threshold} onChange={(v)=>update("risk_gate",{green_threshold:v})} min={0} max={1} step={0.1} />
      </SettingRow>
      <SettingRow label="Yellow Threshold">
        <NumberField value={r.yellow_threshold} onChange={(v)=>update("risk_gate",{yellow_threshold:v})} min={0} max={1} step={0.1} />
      </SettingRow>
      <SettingRow label="Red Threshold">
        <NumberField value={r.red_threshold} onChange={(v)=>update("risk_gate",{red_threshold:v})} min={0} max={1} step={0.1} />
      </SettingRow>
      <SettingRow label="ActionRisk w1 (Blast Radius)">
        <NumberField value={r.action_risk_weight_blast_radius} onChange={(v)=>update("risk_gate",{action_risk_weight_blast_radius:v})} min={0} max={1} step={0.05} />
      </SettingRow>
      <SettingRow label="ActionRisk w2 (Irreversibility)">
        <NumberField value={r.action_risk_weight_irreversibility} onChange={(v)=>update("risk_gate",{action_risk_weight_irreversibility:v})} min={0} max={1} step={0.05} />
      </SettingRow>
      <SettingRow label="InactionRisk w5 (Service Impact)">
        <NumberField value={r.inaction_risk_weight_service_impact} onChange={(v)=>update("risk_gate",{inaction_risk_weight_service_impact:v})} min={0} max={1} step={0.05} />
      </SettingRow>
      <SettingRow label="Emergency Lease Enabled">
        <ToggleField checked={r.emergency_lease_enabled} onChange={(v)=>update("risk_gate",{emergency_lease_enabled:v})} />
      </SettingRow>
      <SettingRow label="Default Lease Duration (s)">
        <NumberField value={r.default_lease_duration_seconds} onChange={(v)=>update("risk_gate",{default_lease_duration_seconds:v})} min={60} max={3600} />
      </SettingRow>
      <SettingRow label="Default Lease Budget">
        <NumberField value={r.default_lease_budget} onChange={(v)=>update("risk_gate",{default_lease_budget:v})} min={1} max={20} />
      </SettingRow>
      <SettingRow label="Require Dual-Sign for Emergency">
        <ToggleField checked={r.require_dual_sign_for_emergency} onChange={(v)=>update("risk_gate",{require_dual_sign_for_emergency:v})} />
      </SettingRow>
    </SettingsSection>
  );
}

/* ============ COGNITIVE CUSTOMS ============ */
function CognitiveCustomsPanel() {
  const { settings, update } = useSettings();
  const c = settings.cognitive_customs;
  return (
    <SettingsSection title={t("settings.cognitive_customs")}>
      <SettingRow label="Default Fact TTL (s)">
        <NumberField value={c.default_fact_ttl_seconds} onChange={(v)=>update("cognitive_customs",{default_fact_ttl_seconds:v})} min={60} max={86400} />
      </SettingRow>
      <SettingRow label="Require Provenance for Fact">
        <ToggleField checked={c.require_provenance_for_fact} onChange={(v)=>update("cognitive_customs",{require_provenance_for_fact:v})} />
      </SettingRow>
      <SettingRow label="Allow Hypothesis Auto-Promotion">
        <ToggleField checked={c.allow_hypothesis_auto_promotion} onChange={(v)=>update("cognitive_customs",{allow_hypothesis_auto_promotion:v})} />
      </SettingRow>
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
      <SettingRow label="Replay Mode Blocks Fact Write">
        <ToggleField checked={c.replay_mode_blocks_fact_write} onChange={(v)=>update("cognitive_customs",{replay_mode_blocks_fact_write:v})} />
      </SettingRow>
    </SettingsSection>
  );
}

/* ============ POLICY ENGINE ============ */
function PolicyEnginePanel() {
  const { settings, update } = useSettings();
  const p = settings.policy_engine;
  return (
    <SettingsSection title={t("settings.policy_engine")} desc="Protocol layer does not bind to OPA. OPA is a reference implementation profile.">
      <SettingRow label="Policy Engine">
        <SelectField value={p.policy_engine} options={[
          {value:"mock",label:"Mock"},{value:"opa",label:"OPA"},{value:"custom",label:"Custom"}
        ]} onChange={(v)=>update("policy_engine",{policy_engine:v as never})} />
      </SettingRow>
      <SettingRow label="Policy Bundle Path">
        <TextField monospace value={p.policy_bundle_path} onChange={(v)=>update("policy_engine",{policy_bundle_path:v})} />
      </SettingRow>
      <SettingRow label="Policy Version">
        <TextField monospace value={p.policy_version} onChange={(v)=>update("policy_engine",{policy_version:v})} />
      </SettingRow>
      <SettingRow label="Decision Log Enabled">
        <ToggleField checked={p.decision_log_enabled} onChange={(v)=>update("policy_engine",{decision_log_enabled:v})} />
      </SettingRow>
      <SettingRow label="Policy Simulation Enabled">
        <ToggleField checked={p.policy_simulation_enabled} onChange={(v)=>update("policy_engine",{policy_simulation_enabled:v})} />
      </SettingRow>
      <SettingRow label="Policy Diff Enabled">
        <ToggleField checked={p.policy_diff_enabled} onChange={(v)=>update("policy_engine",{policy_diff_enabled:v})} />
      </SettingRow>
      <SettingRow label="Health Check URL">
        <TextField monospace value={p.health_check_url} onChange={(v)=>update("policy_engine",{health_check_url:v})} />
      </SettingRow>
    </SettingsSection>
  );
}

/* ============ PRIVACY ============ */
function PrivacyPanel() {
  const { settings, update } = useSettings();
  const p = settings.privacy;
  return (
    <SettingsSection title={t("settings.privacy")}>
      <SettingRow label="Log Retention (days)">
        <NumberField value={p.log_retention_days} onChange={(v)=>update("privacy",{log_retention_days:v})} min={1} max={365} />
      </SettingRow>
      <SettingRow label="Store Full Prompts">
        <ToggleField checked={p.store_full_prompts} onChange={(v)=>update("privacy",{store_full_prompts:v})} />
      </SettingRow>
      <SettingRow label="Store Model Outputs">
        <ToggleField checked={p.store_model_outputs} onChange={(v)=>update("privacy",{store_model_outputs:v})} />
      </SettingRow>
      <SettingRow label="PII Redaction Enabled">
        <ToggleField checked={p.pii_redaction_enabled} onChange={(v)=>update("privacy",{pii_redaction_enabled:v})} />
      </SettingRow>
      <SettingRow label="Local Database Path">
        <TextField monospace value={p.local_database_path} onChange={(v)=>update("privacy",{local_database_path:v})} />
      </SettingRow>
      <SettingRow label="Export Audit Log">
        <button className="px-3 py-1.5 text-xs rounded-md border" style={{borderColor:"var(--border-accent)",color:"var(--text-secondary)"}}
          onClick={()=>{const blob=new Blob(['[]'],{type:'application/json'});const a=document.createElement('a');a.href=URL.createObjectURL(blob);a.download='coevo-audit.json';a.click();}}>
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

/* ============ DEVELOPER ============ */
function DeveloperPanel() {
  const { settings, update } = useSettings();
  const d = settings.developer;
  return (
    <SettingsSection title={t("settings.developer")}>
      <SettingRow label="API Base URL">
        <TextField monospace value={d.api_base_url} onChange={(v)=>update("developer",{api_base_url:v})} />
      </SettingRow>
      <SettingRow label="OpenAPI URL">
        <TextField monospace value={d.openapi_url} onChange={(v)=>update("developer",{openapi_url:v})} />
      </SettingRow>
      <SettingRow label="Mock Mode Enabled">
        <ToggleField checked={d.mock_mode_enabled} onChange={(v)=>update("developer",{mock_mode_enabled:v})} />
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
      <SettingRow label="Reset Demo Data">
        <button className="px-3 py-1.5 text-xs rounded-md border" style={{borderColor:"rgba(239,68,68,0.4)",color:"var(--red)"}}
          onClick={()=>{if(confirm("Reset demo data?")){alert("Demo data reset (mock)");}}}>
          Reset
        </button>
      </SettingRow>
    </SettingsSection>
  );
}
