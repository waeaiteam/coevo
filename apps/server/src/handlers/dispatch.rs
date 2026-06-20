//! Secretary dispatch: the intelligent routing brain.
//!
//! Replaces keyword-only MCL routing with a real model-backed plan. The company secretary
//! (`agent-secretary-01`, seeded for every company) reads the founder's plain-language
//! request plus the live org (departments, employees, skills) and proposes which
//! department head(s) should handle it, broken into concrete sub-tasks.
//!
//! Governance is NOT bypassed: the secretary only *proposes* who does what. Track/risk and
//! authority are still decided server-side by the WorkOrder governance verdict. If the
//! model is unavailable, callers fall back to the existing keyword classifier.

use axum::{
    extract::{Path, State},
    Json,
};
use coevo_models::{
    gateway::select_gateway,
    types::{ModelMessage, ModelRequest, ModelRole, ResponseFormat},
};
use coevo_store::repos::model_config_repo::ModelConfigRepo;
use coevo_store::repos_opc::agent_employee_repo::AgentEmployeeRepo;
use coevo_store::seed::{SECRETARY_AGENT_ID, SECRETARY_SYSTEM_PROMPT};
use serde::{Deserialize, Serialize};

use crate::handlers::opc::company_pool;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DispatchRequest {
    pub intent: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct DispatchSubtask {
    pub department: String,
    /// Resolved head agent id for that department (empty if none found).
    pub assignee_agent_id: String,
    pub goal: String,
    pub rationale: String,
}

#[derive(Debug, Serialize)]
pub struct DispatchPlan {
    pub understanding: String,
    pub subtasks: Vec<DispatchSubtask>,
    /// True when the plan came from the model; false when we fell back.
    pub model_backed: bool,
    pub secretary_agent_id: String,
}

macro_rules! ok {
    ($v:expr) => {
        (axum::http::StatusCode::OK, Json($v))
    };
}
macro_rules! err {
    ($code:expr, $msg:expr) => {
        ($code, Json(serde_json::json!({ "error": $msg })))
    };
}

/// POST /companies/{opc_id}/dispatch  — the secretary's intelligent routing plan.
pub async fn dispatch_plan(
    State(state): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<DispatchRequest>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    if req.intent.trim().is_empty() {
        return err!(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "intent is required"
        );
    }
    match plan_dispatch(&state, &opc_id, &req.intent).await {
        Ok(plan) => ok!(serde_json::to_value(plan).unwrap()),
        Err(e) => err!(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// Core planner reused by the handler and by the work-order creation path.
pub async fn plan_dispatch(
    state: &AppState,
    opc_id: &str,
    intent: &str,
) -> Result<DispatchPlan, String> {
    let pool = company_pool(state, opc_id)
        .await
        .map_err(|(_, body)| body.0.to_string())?;
    let employees = AgentEmployeeRepo::list(&pool).await.unwrap_or_default();
    pool.close().await;

    // Build the department -> head map. The first active employee in a department that is
    // not the secretary is treated as that department's head.
    let mut dept_heads: Vec<(String, String, String)> = Vec::new(); // (dept, agent_id, display)
    for e in &employees {
        let dept = format!("{:?}", e.department);
        if e.agent_id == SECRETARY_AGENT_ID {
            continue;
        }
        if !dept_heads.iter().any(|(d, _, _)| d == &dept) {
            dept_heads.push((dept, e.agent_id.clone(), e.display_name.clone()));
        }
    }

    // Try a model-backed plan; on any failure, fall back to a single-department guess.
    match model_plan(state, intent, &dept_heads).await {
        Ok(mut plan) => {
            resolve_assignees(&mut plan, &dept_heads);
            Ok(plan)
        }
        Err(_) => Ok(fallback_plan(intent, &dept_heads)),
    }
}

async fn model_plan(
    state: &AppState,
    intent: &str,
    dept_heads: &[(String, String, String)],
) -> Result<DispatchPlan, String> {
    let config = match ModelConfigRepo::get_active_config(&state.pool).await {
        Ok(Some(c)) => c,
        _ => return Err("no active model provider".into()),
    };
    if config.kind == coevo_models::types::ModelProviderKind::Mock {
        return Err("mock provider not accepted for dispatch".into());
    }
    let gateway = select_gateway(config.kind);

    let org_summary = dept_heads
        .iter()
        .map(|(dept, id, name)| format!("- {dept}: {name} ({id})"))
        .collect::<Vec<_>>()
        .join("\n");

    let user_prompt = format!(
        "Founder request:\n{intent}\n\nCompany departments and their heads:\n{org_summary}\n\n\
        Decide which department head(s) should handle this. Return JSON with: \
        understanding (one sentence), subtasks (array of {{department, goal, rationale}}). \
        Only use department names from the list above. Keep subtasks minimal — do not invent work."
    );

    let request = ModelRequest {
        config: config.clone(),
        role: ModelRole::StructuredOutput,
        model: config.default_model.clone(),
        messages: vec![
            ModelMessage {
                role: "system".to_string(),
                content: SECRETARY_SYSTEM_PROMPT.to_string(),
                ..Default::default()
            },
            ModelMessage {
                role: "user".to_string(),
                content: user_prompt,
                ..Default::default()
            },
        ],
        temperature: config.temperature,
        max_tokens: config.max_tokens,
        response_format: ResponseFormat::Json,
        stream: false,
        tools: vec![],
        tool_choice: None,
    };
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "understanding": { "type": "string" },
            "subtasks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "department": { "type": "string" },
                        "goal": { "type": "string" },
                        "rationale": { "type": "string" }
                    },
                    "required": ["department", "goal"]
                }
            }
        },
        "required": ["understanding", "subtasks"]
    });

    let response = gateway
        .structured(&request, &schema)
        .await
        .map_err(|e| e.to_string())?;
    let json = response.json.ok_or("provider returned no JSON")?;

    let understanding = json
        .get("understanding")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mut subtasks = Vec::new();
    if let Some(arr) = json.get("subtasks").and_then(|v| v.as_array()) {
        for item in arr {
            let department = item
                .get("department")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let goal = item
                .get("goal")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let rationale = item
                .get("rationale")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if department.is_empty() || goal.is_empty() {
                continue;
            }
            subtasks.push(DispatchSubtask {
                department,
                assignee_agent_id: String::new(),
                goal,
                rationale,
            });
        }
    }
    if subtasks.is_empty() {
        return Err("model returned no usable subtasks".into());
    }
    Ok(DispatchPlan {
        understanding,
        subtasks,
        model_backed: true,
        secretary_agent_id: SECRETARY_AGENT_ID.to_string(),
    })
}

/// Match each subtask's department (case-insensitive) to a head agent id.
fn resolve_assignees(plan: &mut DispatchPlan, dept_heads: &[(String, String, String)]) {
    for task in &mut plan.subtasks {
        let want = task.department.to_lowercase();
        if let Some((_, id, _)) = dept_heads
            .iter()
            .find(|(dept, _, _)| dept.to_lowercase() == want)
        {
            task.assignee_agent_id = id.clone();
        }
    }
}

/// Deterministic fallback when the model is unavailable: route the whole request to the
/// founder office head (or the first available head), as a single sub-task.
fn fallback_plan(intent: &str, dept_heads: &[(String, String, String)]) -> DispatchPlan {
    let chosen = dept_heads
        .iter()
        .find(|(dept, _, _)| dept.eq_ignore_ascii_case("FounderOffice"))
        .or_else(|| dept_heads.first());
    let (department, assignee_agent_id) = match chosen {
        Some((dept, id, _)) => (dept.clone(), id.clone()),
        None => ("FounderOffice".to_string(), String::new()),
    };
    DispatchPlan {
        understanding: "Routed without model planning (provider unavailable).".to_string(),
        subtasks: vec![DispatchSubtask {
            department,
            assignee_agent_id,
            goal: intent.trim().to_string(),
            rationale: "Single-owner fallback while the model provider is unavailable.".to_string(),
        }],
        model_backed: false,
        secretary_agent_id: SECRETARY_AGENT_ID.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heads() -> Vec<(String, String, String)> {
        vec![
            (
                "FounderOffice".into(),
                "agent-founder-01".into(),
                "Founder Assistant".into(),
            ),
            (
                "Product".into(),
                "agent-pm-01".into(),
                "Product Manager".into(),
            ),
            (
                "Engineering".into(),
                "agent-engineer-01".into(),
                "Engineer".into(),
            ),
        ]
    }

    #[test]
    fn fallback_routes_to_founder_office_head() {
        let plan = fallback_plan("organize my week", &heads());
        assert!(!plan.model_backed);
        assert_eq!(plan.subtasks.len(), 1);
        assert_eq!(plan.subtasks[0].assignee_agent_id, "agent-founder-01");
    }

    #[test]
    fn resolve_assignees_matches_department_case_insensitively() {
        let mut plan = DispatchPlan {
            understanding: "x".into(),
            subtasks: vec![DispatchSubtask {
                department: "engineering".into(),
                assignee_agent_id: String::new(),
                goal: "ship it".into(),
                rationale: String::new(),
            }],
            model_backed: true,
            secretary_agent_id: SECRETARY_AGENT_ID.into(),
        };
        resolve_assignees(&mut plan, &heads());
        assert_eq!(plan.subtasks[0].assignee_agent_id, "agent-engineer-01");
    }

    #[test]
    fn fallback_handles_empty_org() {
        let plan = fallback_plan("do something", &[]);
        assert_eq!(plan.subtasks[0].assignee_agent_id, "");
        assert_eq!(plan.subtasks[0].department, "FounderOffice");
    }
}
