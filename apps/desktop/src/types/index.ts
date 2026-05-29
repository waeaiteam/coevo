export interface ContractResponse {
  contract: Record<string, unknown>;
  contract_hash: string;
  ambiguity_score: number;
  compile_warnings: string[];
}

export interface PlanResponse {
  plan: Record<string, unknown>;
  plan_hash: string;
}

export interface ProposeResponse {
  receipt: {
    commit_index: number;
    new_version: number;
    key: string;
    committed_at_ms: number;
  };
}

export interface DemoResponse {
  track: string;
  contract_hash: string;
  plan_hash: string;
  traceparent: string;
  ambiguity_score: number | null;
  warnings: string[];
  entries_created: string[];
  elapsed_ms: number;
}

export interface HealthResponse {
  status: string;
  version: string;
}

// WorkerHarness types
export interface WorkerRun {
  run_id: string; work_order_id: string; agent_id: string; worker_id: string;
  session_id: string; status: string; result_json: unknown; memory_ids_json: unknown;
  errors_json: unknown; audit_ref?: string; started_at_ms: number; ended_at_ms?: number;
}
export interface WorkerStep {
  step_id: string; run_id: string; step_index: number; step_type: string;
  input_json: unknown; output_json?: unknown; status: string;
  started_at_ms: number; ended_at_ms?: number; error?: string;
}
export interface WorkerEvent {
  event_id: string; run_id: string; event_seq: number; event_type: string;
  payload_json: unknown; created_at_ms: number;
}
export interface ToolCallRecord {
  tool_call_id: string; run_id: string; tool_id: string; tool_type: string;
  input_summary: string; output_summary: string; success: boolean;
  risk_ceiling: number; memory_id?: string; started_at_ms: number; ended_at_ms?: number;
}
export interface SkillUsageRecord {
  usage_id: string; run_id: string; skill_id: string; version: string;
  used_for: string; success: boolean; score: number; notes: string; created_at_ms: number;
}
export interface ReflectionRecord {
  reflection_id: string; work_order_id: string; run_id: string; agent_id: string;
  worker_id: string; what_worked_json: unknown; what_failed_json: unknown;
  memory_to_add_json: unknown; skill_to_update_json: unknown;
  user_preference_observed_json: unknown; needs_human_review: boolean; created_at_ms: number;
}
export interface ModelCapability { type: string }
export interface ModelProfile {
  provider_id: string; model_id: string; display_name: string;
  capabilities: string[]; max_context_tokens: number; cost_per_1k_input_usd: number;
  cost_per_1k_output_usd: number; avg_latency_ms: number; supports_json: boolean;
  supports_tools: boolean; privacy_level: string; enabled: boolean;
}
export interface ToolPolicyDecision { allowed: boolean; reason: string; hidden_from_model: boolean; required_approval: boolean }
