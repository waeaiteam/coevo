export type Theme = "light" | "dark" | "system";
export type FontSize = "small" | "normal" | "large" | "extra-large";
export type Density = "comfortable" | "compact";
export type SidebarMode = "icon+text" | "icon-only";
export type Language = "zh" | "en";
export type TimeFormat = "12h" | "24h";
export type HomePage = "mission-chat" | "dashboard";
export type StartupBehavior = "last-task" | "new-task";
export type MissionMode = "auto" | "readonly" | "collaborative" | "high-risk";
export type ApprovalMode = "negative_consent" | "explicit_approval";
export type PolicyEngineType = "mock" | "opa" | "custom";
export type InvalidationStrategy = "direct_only" | "transitive";
export type ProviderType = "openai-compatible" | "openai" | "anthropic" | "gemini" | "deepseek" | "ollama" | "local";
export type EvidenceLevel = "unit_tests_passing" | "integration_verified" | "manual_review";

export interface GeneralSettings {
  default_home: HomePage;
  startup_behavior: StartupBehavior;
  default_mission_mode: MissionMode;
  autosave_drafts: boolean;
  time_format: TimeFormat;
  region: string;
}

export interface AppearanceSettings {
  language: Language;
  theme: Theme;
  font_size: FontSize;
  density: Density;
  sidebar_mode: SidebarMode;
  reduce_motion: boolean;
  high_contrast: boolean;
}

export interface ModelProviderSettings {
  provider: ProviderType;
  base_url: string;
  api_key: string;
  default_model: string;
  fast_model: string;
  reasoning_model: string;
  structured_output_model: string;
  max_tokens: number;
  temperature: number;
  request_timeout_ms: number;
  max_cost_per_task_usd: number;
}

export interface AgentRuntimeSettings {
  default_agent_registry: string;
  allow_task_agent_instance: boolean;
  allow_ephemeral_sub_agent: boolean;
  ephemeral_agent_ttl_minutes: number;
  max_agents_per_task: number;
  max_hops: number;
  allow_cross_org_a2a: boolean;
  default_tool_scope: string;
}

export interface GovernanceSettings {
  auto_confirm_green_track: boolean;
  yellow_approval_mode: ApprovalMode;
  negative_consent_timeout_seconds: number;
  red_requires_mfa: boolean;
  human_override_enabled: boolean;
  one_vote_blocker_roles: string;
  responsibility_anchor_required: boolean;
}

export interface RiskGateSettings {
  green_threshold: number;
  yellow_threshold: number;
  red_threshold: number;
  action_risk_weight_blast_radius: number;
  action_risk_weight_irreversibility: number;
  inaction_risk_weight_service_impact: number;
  emergency_lease_enabled: boolean;
  default_lease_duration_seconds: number;
  default_lease_budget: number;
  require_dual_sign_for_emergency: boolean;
}

export interface CognitiveCustomsSettings {
  default_fact_ttl_seconds: number;
  require_provenance_for_fact: boolean;
  allow_hypothesis_auto_promotion: boolean;
  fact_promotion_evidence_level: EvidenceLevel;
  revoked_fact_invalidation_strategy: InvalidationStrategy;
  replay_mode_blocks_fact_write: boolean;
}

export interface PolicyEngineSettings {
  policy_engine: PolicyEngineType;
  policy_bundle_path: string;
  policy_version: string;
  decision_log_enabled: boolean;
  policy_simulation_enabled: boolean;
  policy_diff_enabled: boolean;
  health_check_url: string;
}

export interface PrivacySettings {
  log_retention_days: number;
  store_full_prompts: boolean;
  store_model_outputs: boolean;
  pii_redaction_enabled: boolean;
  local_database_path: string;
}

export interface DeveloperSettings {
  api_base_url: string;
  openapi_url: string;
  mock_mode_enabled: boolean;
  debug_logs_enabled: boolean;
  show_traceparent: boolean;
  show_raw_json_panels: boolean;
  feature_flags: string;
}

export interface CoevoSettings {
  general: GeneralSettings;
  appearance: AppearanceSettings;
  model_provider: ModelProviderSettings;
  agent_runtime: AgentRuntimeSettings;
  governance: GovernanceSettings;
  risk_gate: RiskGateSettings;
  cognitive_customs: CognitiveCustomsSettings;
  policy_engine: PolicyEngineSettings;
  privacy: PrivacySettings;
  developer: DeveloperSettings;
}
