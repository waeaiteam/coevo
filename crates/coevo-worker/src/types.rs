use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkerStatus { Idle, Assigned, Planning, Executing, WaitingApproval, Reflecting, Completed, Failed, Cancelled }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkerSessionStatus { Open, Running, WaitingApproval, Completed, Failed, Cancelled }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkerRunStatus { Queued, Running, WaitingApproval, Completed, Failed, Cancelled, TimedOut, Blocked }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkerStepType { BuildContext, LoadMemory, LoadSkillIndex, LoadSkillFull, Think, ModelCall, SelectTool, CallTool, CallExecutor, WriteMemory, Reflect, ProposeSkillUpdate, AskHuman }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkerStepStatus { Pending, Running, Completed, Failed, Skipped }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WorkerEventType { LifecycleStart, LifecycleEnd, LifecycleError, AssistantDelta, ToolStart, ToolUpdate, ToolEnd, MemoryWrite, SkillLoaded, ApprovalRequired, WorkerBlocked }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ToolType { GitHubReadonly, FileReadonly, BrowserMock, MCPMock, LocalProcessSandbox, ExternalExecutor }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerChannel { MissionChat, API, System, Scheduled }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWorker { pub worker_id: String, pub agent_id: String, pub department: String, pub status: WorkerStatus, pub current_work_order_id: Option<String>, pub current_session_id: Option<String>, pub loaded_skills_json: serde_json::Value, pub memory_scope: String, pub tool_scope_json: serde_json::Value, pub created_at_ms: i64, pub updated_at_ms: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSession { pub session_id: String, pub worker_id: String, pub work_order_id: String, pub agent_id: String, pub channel: WorkerChannel, pub messages_json: serde_json::Value, pub context_memory_ids_json: serde_json::Value, pub loaded_skill_ids_json: serde_json::Value, pub tool_call_ids_json: serde_json::Value, pub status: WorkerSessionStatus, pub created_at_ms: i64, pub updated_at_ms: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRun { pub run_id: String, pub work_order_id: String, pub agent_id: String, pub worker_id: String, pub session_id: String, pub status: WorkerRunStatus, pub result_json: serde_json::Value, pub memory_ids_json: serde_json::Value, pub errors_json: serde_json::Value, pub audit_ref: Option<String>, pub started_at_ms: i64, pub ended_at_ms: Option<i64> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStep { pub step_id: String, pub run_id: String, pub step_index: i64, pub step_type: WorkerStepType, pub input_json: serde_json::Value, pub output_json: Option<serde_json::Value>, pub status: WorkerStepStatus, pub started_at_ms: i64, pub ended_at_ms: Option<i64>, pub error: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEvent { pub event_id: String, pub run_id: String, pub event_seq: i64, pub event_type: WorkerEventType, pub payload_json: serde_json::Value, pub created_at_ms: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUsageRecord { pub usage_id: String, pub run_id: String, pub skill_id: String, pub version: String, pub used_for: String, pub success: bool, pub score: f64, pub notes: String, pub created_at_ms: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord { pub tool_call_id: String, pub run_id: String, pub tool_id: String, pub tool_type: ToolType, pub input_summary: String, pub output_summary: String, pub success: bool, pub risk_ceiling: f64, pub memory_id: Option<String>, pub started_at_ms: i64, pub ended_at_ms: Option<i64> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionRecord { pub reflection_id: String, pub work_order_id: String, pub run_id: String, pub agent_id: String, pub worker_id: String, pub what_worked_json: serde_json::Value, pub what_failed_json: serde_json::Value, pub memory_to_add_json: serde_json::Value, pub skill_to_update_json: serde_json::Value, pub user_preference_observed_json: serde_json::Value, pub needs_human_review: bool, pub created_at_ms: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool { pub tool_id: String, pub name: String, pub tool_type: ToolType, pub risk_ceiling: f64, pub supported_actions: Vec<String>, pub permission_boundary_json: serde_json::Value, pub requires_credential: bool, pub credential_ref: Option<String>, pub enabled: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicyDecision { pub allowed: bool, pub reason: String, pub hidden_from_model: bool, pub required_approval: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryContext { pub user_profile: Option<serde_json::Value>, pub company_profile: Vec<serde_json::Value>, pub company_memory: Vec<serde_json::Value>, pub agent_memory: Vec<serde_json::Value>, pub task_memory: Vec<serde_json::Value>, pub relevant_skill_memory: Vec<serde_json::Value>, pub stale_memory_ids: Vec<String>, pub excluded_revoked_count: usize, pub context_budget_chars: usize, pub fact_without_provenance: usize }

#[derive(Debug, Clone)]
pub struct TransitionContext { pub track: String, pub has_approval_receipt: bool, pub has_valid_lease: bool, pub reason: String }
