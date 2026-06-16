import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { createEmployee, listEmployees, seedEmployees } from "../api/client";
import Icon, { type IconName } from "../components/Icon";
import { loadSettingsSnapshot } from "../hooks/useSettings";
import { t, useLanguage } from "../settings/i18n";
import { presetFor } from "../settings/modelPresets";

type Preset = {
  department: string;
  labelKey: string;
  descKey: string;
  icon: IconName;
  riskCeiling: number;
};

// Ready-made roles map onto the existing employee create endpoint (no new backend).
const PRESETS: Preset[] = [
  { department: "founder_office", labelKey: "employees.department_founder_office", descKey: "market.preset_founder_office", icon: "user", riskCeiling: 0.3 },
  { department: "product", labelKey: "employees.department_product", descKey: "market.preset_product", icon: "sparkles", riskCeiling: 0.4 },
  { department: "engineering", labelKey: "employees.department_engineering", descKey: "market.preset_engineering", icon: "wrench", riskCeiling: 0.5 },
  { department: "research", labelKey: "employees.department_research", descKey: "market.preset_research", icon: "brain", riskCeiling: 0.4 },
  { department: "governance", labelKey: "employees.department_governance", descKey: "market.preset_governance", icon: "shield-check", riskCeiling: 0.6 },
  { department: "growth", labelKey: "employees.department_growth", descKey: "market.preset_growth", icon: "gauge", riskCeiling: 0.4 },
  { department: "finance", labelKey: "employees.department_finance", descKey: "market.preset_finance", icon: "badge-check", riskCeiling: 0.3 },
  { department: "content", labelKey: "employees.department_content", descKey: "market.preset_content", icon: "file-text", riskCeiling: 0.3 },
];

function buildEmployee(preset: Preset, displayName: string) {
  const agentId = `agent-${preset.department.replace(/_/g, "-")}-${Math.random().toString(36).slice(2, 6)}`;
  const now = Date.now();
  const settings = loadSettingsSnapshot();
  const providerPreset = presetFor(settings.model_provider.provider);
  return {
    agent_id: agentId,
    display_name: displayName,
    department: preset.department,
    role: preset.department,
    passport: {
      passport_id: `passport-${agentId}`,
      issued_by: "talent-market",
      roles: [preset.department],
      capabilities: ["analysis", "planning"],
      restrictions: ["no production write", "no financial transfer"],
      expires_at_ms: null,
    },
    model_profile: {
      provider: providerPreset.provider,
      base_url: settings.model_provider.base_url || providerPreset.baseUrl,
      api_key_ref: "coevo/model-provider",
      default_model: settings.model_provider.default_model || providerPreset.defaultModel,
      fast_model: settings.model_provider.fast_model || providerPreset.fastModel,
      reasoning_model: settings.model_provider.reasoning_model || providerPreset.reasoningModel,
      structured_output_model: settings.model_provider.structured_output_model || providerPreset.structuredModel,
      timeout_ms: settings.model_provider.request_timeout_ms,
      max_tokens: settings.model_provider.max_tokens,
      max_cost_per_task_usd: settings.model_provider.max_cost_per_task_usd,
    },
    tool_scopes: ["urn:coevo:tool:read"],
    memory_scope: "agent",
    permission_boundary: {
      max_risk_score: preset.riskCeiling, can_write_fact: false, can_write_decision: false,
      can_access_network: false, can_access_filesystem: false, can_call_external_executor: false, can_propose_skill: true,
    },
    allowed_cognitive_layers: ["Hypothesis", "Suggestion"],
    allowed_action_modes: ["DRAFT_ONLY"],
    risk_ceiling: preset.riskCeiling,
    reputation_vector: {
      agent_id: agentId, task_domain_competence: 0.5, uncertainty_honesty: 0.5,
      policy_compliance: 0.5, resource_efficiency: 0.5, task_count: 0,
      high_difficulty_avoidance_count: 0, last_updated_ms: now,
    },
    supervisor_agent_id: "agent-founder-01",
    lifecycle_status: "active",
    system_prompt: "",
    created_at_ms: now,
    updated_at_ms: now,
  };
}

export default function TalentMarket() {
  useLanguage();
  const navigate = useNavigate();
  const [employees, setEmployees] = useState<Record<string, unknown>[]>([]);
  const [hiring, setHiring] = useState("");
  const [seeding, setSeeding] = useState(false);

  async function load() {
    try {
      setEmployees(await listEmployees());
    } catch {
      setEmployees([]);
    }
  }

  useEffect(() => {
    void load();
  }, []);

  const onTeam = useMemo(() => {
    const set = new Set<string>();
    for (const e of employees) set.add(String(e.department || "").replace(/([a-z0-9])([A-Z])/g, "$1_$2").replace(/[\s-]+/g, "_").toLowerCase());
    return set;
  }, [employees]);

  async function hire(preset: Preset) {
    if (hiring) return;
    setHiring(preset.department);
    try {
      await createEmployee(buildEmployee(preset, t(preset.labelKey)));
      await load();
      navigate("/employees");
    } catch {
      /* surfaced by reload; keep market usable */
    } finally {
      setHiring("");
    }
  }

  async function seedTeam() {
    if (seeding) return;
    setSeeding(true);
    try {
      await seedEmployees();
      await load();
    } finally {
      setSeeding(false);
    }
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("market.kicker")}</div>
          <h1 className="product-title">{t("market.title")}</h1>
          <p className="product-subtitle">{t("market.subtitle")}</p>
        </div>
      </header>

      <section className="feature-hero">
        <div className="feature-hero-icon"><Icon name="users" /></div>
        <div className="min-w-0">
          <h2>{t("market.hero_title")}</h2>
          <p>{t("market.hero_desc")}</p>
        </div>
      </section>

      <section className="product-grid-2">
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("market.starter_team")}</h2>
          </div>
          <p className="product-prose">{t("market.starter_team_desc")}</p>
          <button className="primary-button mt-3" disabled={seeding} onClick={seedTeam}>
            {seeding ? <Icon name="spinner" className="icon-spin" /> : <Icon name="users" />} {seeding ? t("market.seeding") : t("market.seed_team")}
          </button>
        </div>
        <div className="product-panel">
          <div className="product-panel-heading">
            <h2>{t("market.custom_hire")}</h2>
          </div>
          <p className="product-prose">{t("market.custom_hire_desc")}</p>
          <button className="product-link-button mt-3" onClick={() => navigate("/employees")}>
            <Icon name="plus" /> {t("market.open_custom")}
          </button>
        </div>
      </section>

      <section className="product-panel">
        <div className="product-panel-heading">
          <h2>{t("market.catalog")}</h2>
          <span>{PRESETS.length}</span>
        </div>
        <div className="project-grid">
          {PRESETS.map((preset) => {
            const hired = onTeam.has(preset.department);
            return (
              <div key={preset.department} className="project-card" style={{ cursor: "default" }}>
                <div className="project-card-head">
                  <div className="feature-hero-icon" style={{ width: 32, height: 32 }}><Icon name={preset.icon} /></div>
                  {hired && <span className="product-pill green">{t("market.already_on_team")}</span>}
                </div>
                <h2 className="mt-2">{t(preset.labelKey)}</h2>
                <p>{t(preset.descKey)}</p>
                <div className="project-card-meta">
                  <span className="mono-chip">{t("market.safety_limit")} {preset.riskCeiling}</span>
                </div>
                <div className="project-card-footer">
                  <button className="primary-button" disabled={hiring === preset.department} onClick={() => hire(preset)} style={{ marginLeft: 0 }}>
                    {hiring === preset.department
                      ? <><Icon name="spinner" className="icon-spin" /> {t("market.hiring")}</>
                      : <><Icon name="plus" /> {t("market.hire")}</>}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}
