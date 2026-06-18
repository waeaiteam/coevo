use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Datelike;
use coevo_models::{
    gateway::select_gateway,
    types::{ModelMessage, ModelProviderConfig, ModelRequest, ModelRole, ResponseFormat},
};
use coevo_store::{
    company_workspace::CompanyWorkspaceManager,
    migrate::run_migrations,
    pool::create_pool,
    repos::model_config_repo::ModelConfigRepo,
    repos_opc::{agent_employee_repo, work_order_repo::WorkOrderRepo},
};
use serde::Deserialize;
use sqlx::Row;
use std::collections::HashMap;

use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

#[derive(Deserialize)]
pub struct CreateMeetingRequest {
    pub topic: String,
    pub participants: Vec<String>,
    pub close_mode: String,
}

#[derive(Deserialize)]
pub struct CreateKpiRequest {
    pub work_order_id: String,
    pub scores: serde_json::Value,
    pub reviewer: String,
    pub comment: Option<String>,
}

#[derive(Deserialize)]
pub struct GenerateReportRequest {
    pub period: String,
}

#[derive(Deserialize)]
pub struct UpdateCostQuotaRequest {
    pub department: String,
    pub token_quota: i64,
}

async fn company_pool(
    state: &AppState,
    opc_id: &str,
) -> Result<sqlx::SqlitePool, (StatusCode, Json<serde_json::Value>)> {
    if !is_plain_identifier(opc_id) {
        return Err(err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "opc_id must be a plain identifier"
        ));
    }
    let company_dir = state.company_workspace.company_dir(opc_id);
    if !company_dir.exists() {
        return Err(err!(StatusCode::NOT_FOUND, "company not found"));
    }
    let pool = create_pool(
        &state
            .company_workspace
            .company_db_path(opc_id)
            .to_string_lossy(),
    )
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    run_migrations(&pool)
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(pool)
}

fn meeting_dir(state: &AppState, opc_id: &str, meeting_id: &str) -> std::path::PathBuf {
    state
        .company_workspace
        .company_dir(opc_id)
        .join(".meetings")
        .join(meeting_id)
}

fn report_path(state: &AppState, opc_id: &str, report_id: &str) -> std::path::PathBuf {
    state
        .company_workspace
        .company_dir(opc_id)
        .join("reports")
        .join(format!("{report_id}.md"))
}

fn relative_company_path(state: &AppState, opc_id: &str, path: &std::path::Path) -> String {
    path.strip_prefix(state.company_workspace.company_dir(opc_id))
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

async fn load_company_run_usage(
    state: &AppState,
    company_db: &sqlx::SqlitePool,
    opc_id: &str,
    window_start_ms: Option<i64>,
) -> Result<Vec<CompanyRunUsage>, (StatusCode, Json<serde_json::Value>)> {
    let work_order_ids = sqlx::query(
        "SELECT work_order_id
         FROM work_orders
         WHERE opc_id = ?",
    )
    .bind(opc_id)
    .fetch_all(company_db)
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .into_iter()
    .map(|row| row.get::<String, _>("work_order_id"))
    .collect::<Vec<_>>();

    if work_order_ids.is_empty() {
        return Ok(Vec::new());
    }

    let department_by_agent = sqlx::query(
        "SELECT agent_id, department
         FROM agent_employees",
    )
    .fetch_all(company_db)
    .await
    .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("agent_id"),
            row.try_get::<Option<String>, _>("department")
                .ok()
                .flatten()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Unknown".to_string()),
        )
    })
    .collect::<HashMap<_, _>>();

    let mut sql = String::from(
        "SELECT agent_id,
                COALESCE(SUM(total_tokens),0) AS tokens,
                COALESCE(SUM(total_cost_usd),0) AS cost
         FROM worker_runs
         WHERE opc_id = ?",
    );
    if window_start_ms.is_some() {
        sql.push_str(" AND COALESCE(ended_at_ms, started_at_ms, 0) >= ?");
    }
    sql.push_str(" GROUP BY agent_id");
    let mut query = sqlx::query(&sql);
    query = query.bind(opc_id);
    if let Some(window_start_ms) = window_start_ms {
        query = query.bind(window_start_ms);
    }
    let rows = query
        .fetch_all(&state.pool)
        .await
        .map_err(|e| err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let agent_id = row.get::<String, _>("agent_id");
            CompanyRunUsage {
                department: department_by_agent
                    .get(&agent_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown".to_string()),
                agent_id,
                tokens: row.get::<i64, _>("tokens"),
                cost_usd: row.get::<f64, _>("cost"),
            }
        })
        .collect())
}

fn report_window_start_ms(period: &str, now: chrono::DateTime<chrono::Utc>) -> i64 {
    match period {
        "daily" => now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid")
            .and_utc()
            .timestamp_millis(),
        "monthly" => now
            .date_naive()
            .with_day(1)
            .expect("month start exists")
            .and_hms_opt(0, 0, 0)
            .expect("midnight is valid")
            .and_utc()
            .timestamp_millis(),
        _ => now.timestamp_millis(),
    }
}

#[derive(Debug, Clone)]
struct MeetingTurnDraft {
    agent_id: String,
    stance: String,
    text: String,
}

#[derive(Debug, Clone)]
struct MeetingDraft {
    transcript: Vec<MeetingTurnDraft>,
    resolution_md: String,
    responsibility_anchor: String,
}

#[derive(Debug, Clone)]
struct CompanyRunUsage {
    agent_id: String,
    department: String,
    tokens: i64,
    cost_usd: f64,
}

fn meeting_transcript_json(transcript: &[MeetingTurnDraft]) -> Vec<serde_json::Value> {
    transcript
        .iter()
        .map(|turn| {
            serde_json::json!({
                "agent_id": turn.agent_id,
                "stance": turn.stance,
                "text": turn.text,
            })
        })
        .collect()
}

fn preferred_structured_model(config: &ModelProviderConfig) -> String {
    if !config.structured_output_model.is_empty() {
        config.structured_output_model.clone()
    } else {
        config.default_model.clone()
    }
}

#[cfg(test)]
fn parse_meeting_draft(json: &serde_json::Value, participants: &[String]) -> Option<MeetingDraft> {
    let turns = parse_transcript_turns(json.get("transcript")?, participants)?;
    let resolution_md = json
        .get("resolution_md")
        .or_else(|| json.get("resolution"))
        .or_else(|| json.get("decision_md"))
        .or_else(|| json.get("summary_md"))?
        .as_str()?
        .trim()
        .to_string();
    let responsibility_anchor = json
        .get("responsibility_anchor")
        .or_else(|| json.get("owner"))
        .or_else(|| json.get("chair"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| normalize_meeting_participant_id(value, participants))
        .unwrap_or_default();
    if turns.is_empty() || resolution_md.is_empty() || responsibility_anchor.is_empty() {
        return None;
    }
    for participant in participants {
        if !turns.iter().any(|turn| &turn.agent_id == participant) {
            return None;
        }
    }
    // A resolution can only close once a scrutiny role has actually voiced opposition —
    // role-derived, so any company's risk/governance head qualifies.
    if !turns.iter().any(|turn| {
        inferred_stance(&turn.agent_id) == "oppose" && turn.stance.eq_ignore_ascii_case("oppose")
    }) {
        return None;
    }
    Some(MeetingDraft {
        transcript: turns,
        resolution_md,
        responsibility_anchor,
    })
}

#[cfg(test)]
fn parse_transcript_turns(
    transcript: &serde_json::Value,
    participants: &[String],
) -> Option<Vec<MeetingTurnDraft>> {
    if let Some(items) = transcript.as_array() {
        let turns = items
            .iter()
            .filter_map(|item| {
                let agent_id = item
                    .get("agent_id")
                    .or_else(|| item.get("agent"))
                    .or_else(|| item.get("speaker"))
                    .or_else(|| item.get("role"))
                    .or_else(|| item.get("participant"))
                    .or_else(|| item.get("name"))?
                    .as_str()?
                    .trim()
                    .to_string();
                let agent_id =
                    normalize_meeting_participant_id(&agent_id, participants).unwrap_or(agent_id);
                let text = item
                    .get("text")
                    .or_else(|| item.get("statement"))
                    .or_else(|| item.get("message"))
                    .or_else(|| item.get("content"))?
                    .as_str()?
                    .trim()
                    .to_string();
                let stance = item
                    .get("stance")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| inferred_stance(&agent_id));
                if agent_id.is_empty() || stance.is_empty() || text.is_empty() {
                    return None;
                }
                Some(MeetingTurnDraft {
                    agent_id,
                    stance,
                    text,
                })
            })
            .collect::<Vec<_>>();
        return Some(turns);
    }

    let transcript_text = transcript.as_str()?.trim();
    if transcript_text.is_empty() {
        return None;
    }

    let mut turns = Vec::new();
    for line in transcript_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (agent_id, text) = trimmed.split_once(':')?;
        let agent_id = normalize_meeting_participant_id(agent_id.trim(), participants)
            .unwrap_or_else(|| agent_id.trim().to_string());
        let text = text.trim().to_string();
        if agent_id.is_empty() || text.is_empty() {
            continue;
        }
        if !participants
            .iter()
            .any(|participant| participant == &agent_id)
        {
            continue;
        }
        let stance = inferred_stance(&agent_id);
        turns.push(MeetingTurnDraft {
            agent_id,
            stance,
            text,
        });
    }

    if turns.is_empty() {
        None
    } else {
        Some(turns)
    }
}

fn normalize_meeting_participant_id(value: &str, participants: &[String]) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if participants
        .iter()
        .any(|participant| participant == trimmed)
    {
        return Some(trimmed.to_string());
    }
    let normalized = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    match normalized.as_str() {
        "founder" => participants
            .iter()
            .find(|participant| participant.contains("founder"))
            .cloned(),
        "productmanager" | "pm" => participants
            .iter()
            .find(|participant| participant.contains("pm"))
            .cloned(),
        "critic" | "riskanalyst" | "risk" => participants
            .iter()
            .find(|participant| participant.contains("critic") || participant.contains("risk"))
            .cloned(),
        _ => None,
    }
}

/// A participant's default debate stance, derived from their role rather than a fixed id
/// list. Governance/risk/compliance/critic-type roles scrutinize (oppose) by their nature;
/// everyone else advocates (support). This lets any company's heads debate autonomously —
/// the dissent comes from what a role *is*, not from a hardcoded agent id.
fn inferred_stance(agent_id_or_role: &str) -> String {
    let lower = agent_id_or_role.to_lowercase();
    const SCRUTINY_MARKERS: [&str; 6] =
        ["critic", "risk", "compliance", "governance", "audit", "legal"];
    if SCRUTINY_MARKERS.iter().any(|m| lower.contains(m)) {
        "oppose".to_string()
    } else {
        "support".to_string()
    }
}

fn read_nonempty_markdown(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

fn load_meeting_participant_persona(
    workspace: &CompanyWorkspaceManager,
    opc_id: &str,
    agent_id: &str,
) -> String {
    const PERSONA_SECTION_CHAR_LIMIT: usize = 4000;
    let employee_dir = workspace.company_employee_dir(opc_id, agent_id);
    let mut sections = vec![format!("Participant: {agent_id}")];

    if let Some(prompt_body) =
        read_nonempty_markdown(&workspace.company_employee_prompt_path(opc_id, agent_id))
    {
        sections.push(prompt_body);
    }

    for (label, path) in [
        ("identity.md", employee_dir.join("identity.md")),
        ("soul.md", employee_dir.join("soul.md")),
        ("agents.md", employee_dir.join("agents.md")),
        ("owner.md", employee_dir.join("owner.md")),
        ("tools.md", employee_dir.join("tools.md")),
    ] {
        if let Some(content) = read_nonempty_markdown(&path) {
            let content = if content.chars().count() > PERSONA_SECTION_CHAR_LIMIT {
                let truncated: String = content.chars().take(PERSONA_SECTION_CHAR_LIMIT).collect();
                format!(
                    "{truncated}\n[TRUNCATED: {} chars total]",
                    content.chars().count()
                )
            } else {
                content
            };
            sections.push(format!("[{label}]\n{content}"));
        }
    }

    sections.join("\n\n")
}

fn build_meeting_turn_user_prompt(
    workspace: &CompanyWorkspaceManager,
    opc_id: &str,
    topic: &str,
    close_mode: &str,
    participants: &[String],
    current_agent_id: &str,
    prior_turns: &[MeetingTurnDraft],
) -> String {
    let persona = load_meeting_participant_persona(workspace, opc_id, current_agent_id);
    let prior_transcript = if prior_turns.is_empty() {
        "(none yet)".to_string()
    } else {
        prior_turns
            .iter()
            .map(|turn| format!("{} [{}]: {}", turn.agent_id, turn.stance, turn.text))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Topic: {topic}\nClose mode: {close_mode}\nParticipants: {}\nCurrent participant: {current_agent_id}\nAssigned stance: {}\nParticipant persona:\n{}\nPrior transcript:\n{}\nReturn JSON with agent_id, stance, text for the current participant only.",
        participants.join(", "),
        inferred_stance(current_agent_id),
        persona,
        prior_transcript,
    )
}

async fn generate_meeting_turn(
    state: &AppState,
    opc_id: &str,
    topic: &str,
    participants: &[String],
    close_mode: &str,
    current_agent_id: &str,
    prior_turns: &[MeetingTurnDraft],
) -> Result<MeetingTurnDraft, String> {
    let config = match ModelConfigRepo::get_active_config(&state.pool).await {
        Ok(Some(config)) => config,
        Ok(None) => {
            return Err(
                "meeting turn generation failed: MODEL_PROVIDER_NOT_CONFIGURED: configure a real model provider before running meetings"
                    .to_string(),
            )
        }
        Err(e) => return Err(format!("meeting turn generation failed: {e}")),
    };
    if config.kind == coevo_models::types::ModelProviderKind::Mock {
        return Err(
            "meeting turn generation failed: meetings require a real configured provider; mock provider is not accepted"
                .to_string(),
        );
    }
    let gateway = select_gateway(config.kind);
    let request = ModelRequest {
        config: config.clone(),
        role: ModelRole::StructuredOutput,
        model: preferred_structured_model(&config),
        messages: vec![
            ModelMessage {
                role: "system".to_string(),
                content: "You generate one participant's meeting statement for a backend collaboration platform. Return JSON only. Keep the assigned stance. A participant in a risk, compliance, governance, audit, or critic role must oppose with a concrete risk argument.".to_string(),
                ..Default::default()
            },
            ModelMessage {
                role: "user".to_string(),
                content: build_meeting_turn_user_prompt(
                    &state.company_workspace,
                    opc_id,
                    topic,
                    close_mode,
                    participants,
                    current_agent_id,
                    prior_turns,
                ),
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
            "agent_id": { "type": "string" },
            "stance": { "type": "string" },
            "text": { "type": "string" }
        },
        "required": ["agent_id", "stance", "text"]
    });
    let response = match gateway.structured(&request, &schema).await {
        Ok(response) => response,
        Err(e) => return Err(format!("meeting turn generation failed: {e}")),
    };
    let json = match response.json {
        Some(json) => json,
        None => return Err("meeting turn generation failed: provider returned no JSON".to_string()),
    };
    let agent_id = json
        .get("agent_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| normalize_meeting_participant_id(value, participants))
        .unwrap_or_else(|| current_agent_id.to_string());
    let text = json
        .get("text")
        .or_else(|| json.get("statement"))
        .or_else(|| json.get("message"))
        .or_else(|| json.get("content"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "meeting turn generation failed: provider JSON missing text".to_string())?
        .to_string();
    let stance = json
        .get("stance")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| inferred_stance(&agent_id));
    Ok(MeetingTurnDraft {
        agent_id,
        stance,
        text,
    })
}

async fn generate_meeting_resolution(
    state: &AppState,
    topic: &str,
    close_mode: &str,
    transcript: &[MeetingTurnDraft],
) -> Result<(String, String), String> {
    let config = match ModelConfigRepo::get_active_config(&state.pool).await {
        Ok(Some(config)) => config,
        Ok(None) => {
            return Err(
                "meeting resolution generation failed: MODEL_PROVIDER_NOT_CONFIGURED: configure a real model provider before running meetings"
                    .to_string(),
            )
        }
        Err(e) => return Err(format!("meeting resolution generation failed: {e}")),
    };
    if config.kind == coevo_models::types::ModelProviderKind::Mock {
        return Err(
            "meeting resolution generation failed: meetings require a real configured provider; mock provider is not accepted"
                .to_string(),
        );
    }
    let gateway = select_gateway(config.kind);
    let transcript_text = transcript
        .iter()
        .map(|turn| format!("{} [{}]: {}", turn.agent_id, turn.stance, turn.text))
        .collect::<Vec<_>>()
        .join("\n");
    let request = ModelRequest {
        config: config.clone(),
        role: ModelRole::StructuredOutput,
        model: preferred_structured_model(&config),
        messages: vec![
            ModelMessage {
                role: "system".to_string(),
                content: "You summarize a company meeting into a concise markdown opinion letter. Return JSON only.".to_string(),
                ..Default::default()
            },
            ModelMessage {
                role: "user".to_string(),
                content: format!(
                    "Topic: {topic}\nClose mode: {close_mode}\nTranscript:\n{transcript_text}\nReturn JSON with resolution_md and responsibility_anchor."
                ),
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
            "resolution_md": { "type": "string" },
            "responsibility_anchor": { "type": "string" }
        },
        "required": ["resolution_md", "responsibility_anchor"]
    });
    let response = match gateway.structured(&request, &schema).await {
        Ok(response) => response,
        Err(e) => return Err(format!("meeting resolution generation failed: {e}")),
    };
    let json = match response.json {
        Some(json) => json,
        None => {
            return Err(
                "meeting resolution generation failed: provider returned no JSON".to_string(),
            )
        }
    };
    let resolution_md = json
        .get("resolution_md")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .ok_or_else(|| {
            "meeting resolution generation failed: provider JSON missing resolution_md".to_string()
        })?;
    let responsibility_anchor = json
        .get("responsibility_anchor")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .ok_or_else(|| {
            "meeting resolution generation failed: provider JSON missing responsibility_anchor"
                .to_string()
        })?;
    Ok((resolution_md, responsibility_anchor))
}

async fn generate_meeting_draft(
    state: &AppState,
    opc_id: &str,
    topic: &str,
    close_mode: &str,
    participants: &[String],
) -> Result<MeetingDraft, String> {
    let mut transcript = Vec::new();
    for participant in participants {
        let turn = generate_meeting_turn(
            state,
            opc_id,
            topic,
            participants,
            close_mode,
            participant,
            &transcript,
        )
        .await?;
        transcript.push(turn);
    }
    let (resolution_md, responsibility_anchor_raw) =
        generate_meeting_resolution(state, topic, close_mode, &transcript).await?;
    let responsibility_anchor = normalize_meeting_participant_id(
        responsibility_anchor_raw.trim(),
        participants,
    )
    .or_else(|| infer_meeting_responsibility_anchor(&transcript, participants))
    .ok_or_else(|| {
        format!(
            "meeting resolution generation failed: responsibility_anchor {} is not a meeting participant",
            responsibility_anchor_raw
        )
    })?;
    Ok(MeetingDraft {
        transcript,
        resolution_md,
        responsibility_anchor,
    })
}

fn infer_meeting_responsibility_anchor(
    transcript: &[MeetingTurnDraft],
    participants: &[String],
) -> Option<String> {
    transcript
        .iter()
        .find(|turn| turn.stance.eq_ignore_ascii_case("support"))
        .map(|turn| turn.agent_id.clone())
        .or_else(|| transcript.first().map(|turn| turn.agent_id.clone()))
        .filter(|agent_id| {
            participants
                .iter()
                .any(|participant| participant == agent_id)
        })
}

async fn ensure_company_employee_exists(
    pool: &sqlx::SqlitePool,
    agent_id: &str,
) -> Result<(), String> {
    match agent_employee_repo::AgentEmployeeRepo::exists(pool, agent_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("Employee not found: {agent_id}")),
        Err(e) => Err(e.to_string()),
    }
}

pub async fn create_meeting(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<CreateMeetingRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.topic.trim().is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "topic is required");
    }
    if req.participants.is_empty() {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "participants are required"
        );
    }
    if req.close_mode != "vote" && req.close_mode != "chair" {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "close_mode must be vote or chair"
        );
    }
    // A debate needs a built-in dissenter. Accept any participant whose id/role implies a
    // scrutiny function (risk/compliance/governance/audit/legal/critic), not just two fixed ids.
    if !req
        .participants
        .iter()
        .any(|id| inferred_stance(id) == "oppose")
    {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "meetings must include a scrutiny role (risk, compliance, governance, audit, legal, or critic) as a dissenting participant"
        );
    }

    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    for participant in &req.participants {
        if let Err(e) = ensure_company_employee_exists(&pool, participant).await {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, e);
        }
    }

    let now = chrono::Utc::now().timestamp_millis();
    let meeting_id = format!("meeting-{}", uuid::Uuid::new_v4().simple());
    let archive_dir = meeting_dir(&s, &opc_id, &meeting_id);
    if let Err(e) = std::fs::create_dir_all(&archive_dir) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let meeting_draft =
        match generate_meeting_draft(&s, &opc_id, &req.topic, &req.close_mode, &req.participants)
            .await
        {
            Ok(draft) => draft,
            Err(e) => {
                pool.close().await;
                return err!(StatusCode::BAD_GATEWAY, e);
            }
        };
    let transcript = meeting_transcript_json(&meeting_draft.transcript);
    let resolution_md = meeting_draft.resolution_md.clone();
    let agenda = serde_json::json!([req.topic.clone()]);
    let participants_json =
        serde_json::to_string(&req.participants).unwrap_or_else(|_| "[]".to_string());
    let transcript_json = serde_json::to_string(&transcript).unwrap_or_else(|_| "[]".to_string());
    let agenda_json = agenda.to_string();
    let archive_relpath = relative_company_path(&s, &opc_id, &archive_dir);
    if let Err(e) = std::fs::write(
        archive_dir.join("agenda.json"),
        serde_json::json!({
            "topic": req.topic,
            "participants": req.participants,
            "close_mode": req.close_mode,
        })
        .to_string(),
    ) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    if let Err(e) = std::fs::write(archive_dir.join("resolution.md"), &resolution_md) {
        pool.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    let insert = sqlx::query(
        "INSERT INTO meetings (
            meeting_id, topic, status, close_mode, agenda_json, participants_json, transcript_json,
            resolution_md, responsibility_anchor, archive_relpath, created_at_ms, updated_at_ms
        ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&meeting_id)
    .bind(&req.topic)
    .bind("running")
    .bind(&req.close_mode)
    .bind(&agenda_json)
    .bind(&participants_json)
    .bind(&transcript_json)
    .bind(&resolution_md)
    .bind(&meeting_draft.responsibility_anchor)
    .bind(&archive_relpath)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await;
    pool.close().await;

    match insert {
        Ok(_) => ok!(serde_json::json!({
            "meeting_id": meeting_id,
            "status": "running"
        })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_meetings(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let rows = sqlx::query(
        "SELECT meeting_id, topic, status, close_mode, created_at_ms
         FROM meetings
         ORDER BY created_at_ms DESC",
    )
    .fetch_all(&pool)
    .await;
    pool.close().await;
    match rows {
        Ok(rows) => ok!(serde_json::Value::Array(
            rows.iter()
                .map(|row| {
                    serde_json::json!({
                        "meeting_id": row.get::<String, _>("meeting_id"),
                        "topic": row.get::<String, _>("topic"),
                        "status": row.get::<String, _>("status"),
                        "close_mode": row.get::<String, _>("close_mode"),
                        "created_at_ms": row.get::<i64, _>("created_at_ms"),
                    })
                })
                .collect(),
        )),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_meeting(
    State(s): State<AppState>,
    Path((opc_id, meeting_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let row = sqlx::query("SELECT * FROM meetings WHERE meeting_id = ?")
        .bind(&meeting_id)
        .fetch_optional(&pool)
        .await;
    pool.close().await;
    match row {
        Ok(Some(row)) => {
            let agenda =
                serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("agenda_json"))
                    .unwrap_or_else(|_| serde_json::json!([]));
            let transcript =
                serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("transcript_json"))
                    .unwrap_or_else(|_| serde_json::json!([]));
            ok!(serde_json::json!({
                "meeting_id": row.get::<String, _>("meeting_id"),
                "topic": row.get::<String, _>("topic"),
                "status": row.get::<String, _>("status"),
                "agenda": agenda,
                "transcript": transcript,
                "resolution_md": row.get::<String, _>("resolution_md"),
                "responsibility_anchor": row.get::<String, _>("responsibility_anchor"),
                "created_at_ms": row.get::<i64, _>("created_at_ms"),
            }))
        }
        Ok(None) => err!(StatusCode::NOT_FOUND, "Meeting not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_employee_kpi(
    State(s): State<AppState>,
    Path((opc_id, agent_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let rows = sqlx::query(
        "SELECT kpi_id, work_order_id, reviewer_agent_id, scores_json, comment, created_at_ms
         FROM kpi_records
         WHERE agent_id = ?
         ORDER BY created_at_ms DESC",
    )
    .bind(&agent_id)
    .fetch_all(&pool)
    .await;
    pool.close().await;
    match rows {
        Ok(rows) => ok!(serde_json::Value::Array(
            rows.iter()
                .map(|row| {
                    let scores = serde_json::from_str::<serde_json::Value>(
                        &row.get::<String, _>("scores_json"),
                    )
                    .unwrap_or_else(|_| serde_json::json!({}));
                    serde_json::json!({
                        "kpi_id": row.get::<String, _>("kpi_id"),
                        "work_order_id": row.get::<String, _>("work_order_id"),
                        "reviewer": row.get::<String, _>("reviewer_agent_id"),
                        "scores": scores,
                        "comment": row.get::<String, _>("comment"),
                        "created_at_ms": row.get::<i64, _>("created_at_ms"),
                    })
                })
                .collect(),
        )),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_employee_kpi(
    State(s): State<AppState>,
    Path((opc_id, agent_id)): Path<(String, String)>,
    Json(req): Json<CreateKpiRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    if let Err(e) = ensure_company_employee_exists(&company_db, &agent_id).await {
        company_db.close().await;
        return err!(StatusCode::NOT_FOUND, e);
    }
    if let Err(e) = ensure_company_employee_exists(&company_db, &req.reviewer).await {
        company_db.close().await;
        return err!(StatusCode::NOT_FOUND, e);
    }
    let work_order = match WorkOrderRepo::get(&company_db, &req.work_order_id).await {
        Ok(Some(work_order)) => Some(work_order),
        Ok(None) => match WorkOrderRepo::get(&s.pool, &req.work_order_id).await {
            Ok(Some(work_order)) => Some(work_order),
            Ok(None) => None,
            Err(e) => {
                company_db.close().await;
                return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
        },
        Err(e) => {
            company_db.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    match work_order {
        Some(work_order) if work_order.opc_id == opc_id => {}
        Some(_) => {
            company_db.close().await;
            return err!(
                StatusCode::FORBIDDEN,
                "work order belongs to a different company"
            );
        }
        None => {
            company_db.close().await;
            return err!(StatusCode::NOT_FOUND, "WorkOrder not found");
        }
    }

    let kpi_id = format!("kpi-{}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now().timestamp_millis();
    let insert = sqlx::query(
        "INSERT INTO kpi_records (
            kpi_id, agent_id, work_order_id, reviewer_agent_id, scores_json, comment, created_at_ms
        ) VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&kpi_id)
    .bind(&agent_id)
    .bind(&req.work_order_id)
    .bind(&req.reviewer)
    .bind(req.scores.to_string())
    .bind(req.comment.clone().unwrap_or_default())
    .bind(now)
    .execute(&company_db)
    .await;
    company_db.close().await;
    match insert {
        Ok(_) => ok!(serde_json::json!({
            "kpi_id": kpi_id,
            "work_order_id": req.work_order_id,
            "reviewer": req.reviewer,
            "scores": req.scores,
            "comment": req.comment.unwrap_or_default(),
            "created_at_ms": now,
        })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_reports(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let rows = sqlx::query(
        "SELECT report_id, period, report_md_path, created_at_ms
         FROM generated_reports
         ORDER BY created_at_ms DESC",
    )
    .fetch_all(&company_db)
    .await;
    company_db.close().await;
    match rows {
        Ok(rows) => ok!(serde_json::Value::Array(
            rows.iter()
                .map(|row| {
                    serde_json::json!({
                        "report_id": row.get::<String, _>("report_id"),
                        "period": row.get::<String, _>("period"),
                        "path": row.get::<String, _>("report_md_path"),
                        "created_at_ms": row.get::<i64, _>("created_at_ms"),
                    })
                })
                .collect(),
        )),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_report(
    State(s): State<AppState>,
    Path((opc_id, report_id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let row = sqlx::query(
        "SELECT report_id, period, report_md_path, kpi_summary_json, token_usage_json, alerts_json
         FROM generated_reports
         WHERE report_id = ?",
    )
    .bind(&report_id)
    .fetch_optional(&company_db)
    .await;
    company_db.close().await;
    match row {
        Ok(Some(row)) => {
            let absolute_path = s
                .company_workspace
                .company_dir(&opc_id)
                .join(row.get::<String, _>("report_md_path"));
            let report_md = std::fs::read_to_string(absolute_path).unwrap_or_default();
            ok!(serde_json::json!({
                "report_md": report_md,
                "period": row.get::<String, _>("period"),
                "kpi_summary": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("kpi_summary_json")).unwrap_or_else(|_| serde_json::json!([])),
                "token_usage": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("token_usage_json")).unwrap_or_else(|_| serde_json::json!({})),
                "alerts": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("alerts_json")).unwrap_or_else(|_| serde_json::json!([])),
            }))
        }
        Ok(None) => err!(StatusCode::NOT_FOUND, "Report not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn generate_report(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<GenerateReportRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.period != "daily" && req.period != "monthly" {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "period must be daily or monthly"
        );
    }
    let now_utc = chrono::Utc::now();
    let window_start_ms = report_window_start_ms(&req.period, now_utc);
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let kpi_rows = match sqlx::query(
        "SELECT agent_id, scores_json, created_at_ms
         FROM kpi_records
         WHERE created_at_ms >= ?
         ORDER BY created_at_ms DESC
         LIMIT 20",
    )
    .bind(window_start_ms)
    .fetch_all(&company_db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            company_db.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };

    let run_rows =
        match load_company_run_usage(&s, &company_db, &opc_id, Some(window_start_ms)).await {
            Ok(rows) => rows,
            Err(err) => {
                company_db.close().await;
                return err;
            }
        };

    let kpi_summary: Vec<serde_json::Value> = kpi_rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "agent_id": row.get::<String, _>("agent_id"),
                "scores": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("scores_json")).unwrap_or_else(|_| serde_json::json!({})),
                "created_at_ms": row.get::<i64, _>("created_at_ms"),
            })
        })
        .collect();
    let token_usage = serde_json::json!({
        "by_agent": run_rows.iter().map(|row| serde_json::json!({
            "agent_id": row.agent_id,
            "department": row.department,
            "tokens": row.tokens,
            "cost_usd": row.cost_usd,
        })).collect::<Vec<_>>()
    });
    let has_kpi_data = !kpi_summary.is_empty();
    let has_usage_data = token_usage["by_agent"]
        .as_array()
        .map(|items| {
            items.iter().any(|item| {
                item["tokens"].as_i64().unwrap_or_default() > 0
                    || item["cost_usd"].as_f64().unwrap_or_default() > 0.0
            })
        })
        .unwrap_or(false);
    if !has_kpi_data && !has_usage_data {
        company_db.close().await;
        return err!(
            StatusCode::CONFLICT,
            format!(
                "REPORT_SOURCE_EMPTY: no KPI or worker usage data found for opc_id={} period={}",
                opc_id, req.period
            )
        );
    }
    let alerts: Vec<serde_json::Value> = run_rows
        .iter()
        .filter(|row| row.cost_usd > 0.0)
        .map(|row| {
            serde_json::json!({
                "level": "info",
                "message": format!(
                    "{} consumed ${:.4}",
                    row.agent_id,
                    row.cost_usd
                )
            })
        })
        .collect();

    let report_id = format!("report-{}", uuid::Uuid::new_v4().simple());
    let markdown = format!(
        "# {} Report\n\n## KPI\n{}\n\n## Token Usage\n{}\n",
        req.period,
        kpi_summary
            .iter()
            .map(|item| format!(
                "- {}: {}",
                item["agent_id"].as_str().unwrap_or_default(),
                item["scores"]
            ))
            .collect::<Vec<_>>()
            .join("\n"),
        token_usage["by_agent"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|item| format!(
                "- {} / {}: {} tokens / ${:.4}",
                item["department"].as_str().unwrap_or("Unknown"),
                item["agent_id"].as_str().unwrap_or_default(),
                item["tokens"].as_i64().unwrap_or_default(),
                item["cost_usd"].as_f64().unwrap_or_default()
            ))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let absolute_report_path = report_path(&s, &opc_id, &report_id);
    if let Err(e) = std::fs::write(&absolute_report_path, &markdown) {
        company_db.close().await;
        return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let now = now_utc.timestamp_millis();
    let relative_path = relative_company_path(&s, &opc_id, &absolute_report_path);
    let insert = sqlx::query(
        "INSERT INTO generated_reports (
            report_id, period, report_md_path, kpi_summary_json, token_usage_json, alerts_json, created_at_ms
        ) VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&report_id)
    .bind(&req.period)
    .bind(&relative_path)
    .bind(serde_json::to_string(&kpi_summary).unwrap_or_else(|_| "[]".to_string()))
    .bind(token_usage.to_string())
    .bind(serde_json::to_string(&alerts).unwrap_or_else(|_| "[]".to_string()))
    .bind(now)
    .execute(&company_db)
    .await;
    company_db.close().await;
    match insert {
        Ok(_) => ok!(serde_json::json!({ "report_id": report_id })),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_cost_summary(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let quota_rows = sqlx::query("SELECT department, token_quota FROM department_cost_quotas")
        .fetch_all(&company_db)
        .await;
    let quota_map = match quota_rows {
        Ok(rows) => rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("department"),
                    row.get::<i64, _>("token_quota"),
                )
            })
            .collect::<std::collections::HashMap<_, _>>(),
        Err(e) => {
            company_db.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    };
    let run_rows = match load_company_run_usage(&s, &company_db, &opc_id, None).await {
        Ok(rows) => rows,
        Err(err) => {
            company_db.close().await;
            return err;
        }
    };
    company_db.close().await;
    let mut by_department_totals: HashMap<String, (i64, f64)> = HashMap::new();
    for row in run_rows {
        let entry = by_department_totals
            .entry(row.department)
            .or_insert((0, 0.0));
        entry.0 += row.tokens;
        entry.1 += row.cost_usd;
    }
    let mut departments = by_department_totals.into_iter().collect::<Vec<_>>();
    departments.sort_by(|a, b| a.0.cmp(&b.0));
    let by_department: Vec<serde_json::Value> = departments
        .iter()
        .map(|(department, (tokens, cost_usd))| {
            serde_json::json!({
                "dept": department,
                "tokens": tokens,
                "cost_usd": cost_usd,
                "quota": quota_map.get(department).copied(),
            })
        })
        .collect();
    let total_tokens = by_department
        .iter()
        .map(|row| row["tokens"].as_i64().unwrap_or_default())
        .sum::<i64>();
    let total_cost = by_department
        .iter()
        .map(|row| row["cost_usd"].as_f64().unwrap_or_default())
        .sum::<f64>();
    ok!(serde_json::json!({
        "by_department": by_department,
        "total": {
            "tokens": total_tokens,
            "cost_usd": total_cost
        }
    }))
}

pub async fn put_cost_quota(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<UpdateCostQuotaRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.department.trim().is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "department is required");
    }
    if req.token_quota < 0 {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "token_quota must be >= 0");
    }
    let company_db = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let now = chrono::Utc::now().timestamp_millis();
    let result = sqlx::query(
        "INSERT INTO department_cost_quotas (department, token_quota, updated_at_ms)
         VALUES (?,?,?)
         ON CONFLICT(department)
         DO UPDATE SET token_quota = excluded.token_quota, updated_at_ms = excluded.updated_at_ms",
    )
    .bind(&req.department)
    .bind(req.token_quota)
    .bind(now)
    .execute(&company_db)
    .await;
    company_db.close().await;
    match result {
        Ok(_) => ok!(
            serde_json::json!({"ok": true, "department": req.department, "token_quota": req.token_quota})
        ),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{router::build_router, state::AppState};
    use axum::{body::Body, http::Request};
    use coevo_core::opc::{WorkOrder, WorkOrderStatus};
    use coevo_store::{
        migrate::run_migrations,
        pool::{create_pool, create_test_pool},
        repos::model_config_repo::ModelConfigRepo,
        repos::worker_run_repo::WorkerRunRepo,
        repos_opc::work_order_repo::WorkOrderRepo,
    };
    use tower::ServiceExt;

    async fn create_company(app: &axum::Router) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/companies")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"Org Co","mission":"Stage 5"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        body["opc_id"].as_str().unwrap().to_string()
    }

    async fn configure_active_mock_provider(pool: &sqlx::SqlitePool) {
        ModelConfigRepo::upsert_config(
            pool,
            "desktop-test",
            "Mock",
            "",
            "",
            &ModelConfigRepo::mask_key(""),
            "mock-model",
            "mock-model",
            "mock-model",
            "mock-model",
            4096,
            0.2,
            30000,
            0.0,
        )
        .await
        .unwrap();
    }

    async fn configure_active_openai_compatible(pool: &sqlx::SqlitePool) {
        ModelConfigRepo::upsert_config(
            pool,
            "desktop-test",
            "OpenAICompatible",
            "https://api.openai.com/v1",
            "sk-test",
            &ModelConfigRepo::mask_key("sk-test"),
            "gpt-4o",
            "gpt-4o-mini",
            "o3-mini",
            "gpt-4o",
            16384,
            0.2,
            30000,
            5.0,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn company_organization_routes_support_meetings_kpi_reports_and_costs() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_openai_compatible(&pool).await;
        let root = std::env::temp_dir().join(format!("coevo-company-org-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool.clone(), root.clone()));
        let opc_id = create_company(&app).await;

        let seed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed.status(), StatusCode::OK);

        let meeting = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/meetings"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "topic": "Ship Stage 5",
                            "participants": ["agent-founder-01", "agent-critic-01", "agent-risk-01"],
                            "close_mode": "vote"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(meeting.status(), StatusCode::OK);
        let meeting_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(meeting.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let meeting_id = meeting_body["meeting_id"].as_str().unwrap();
        assert!(root
            .join(&opc_id)
            .join(".meetings")
            .join(meeting_id)
            .join("resolution.md")
            .exists());

        let meeting_detail = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/meetings/{meeting_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(meeting_detail.status(), StatusCode::OK);
        let meeting_detail_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(meeting_detail.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let transcript = meeting_detail_body["transcript"].as_array().unwrap();
        assert!(transcript.len() >= 3);
        assert!(transcript
            .iter()
            .any(|turn| { turn["agent_id"] == "agent-critic-01" && turn["stance"] == "oppose" }));
        assert!(transcript.iter().all(|turn| {
            let text = turn["text"].as_str().unwrap_or_default();
            text != "Support the proposal and summarize the execution benefit."
                && text != "Raise a concrete dissenting risk or quality concern before adopting the plan."
        }));
        assert_ne!(
            meeting_detail_body["resolution_md"].as_str().unwrap_or_default(),
            "# Opinion Letter\n\nTopic: Ship Stage 5\n\nMode: vote\n\nResolution: Proceed with the discussion outcome and keep the dissent on record.\n"
        );
        assert!(transcript.iter().any(|turn| {
            turn["text"]
                .as_str()
                .unwrap_or_default()
                .contains("Ship Stage 5")
        }));
        assert!(meeting_detail_body["resolution_md"]
            .as_str()
            .unwrap_or_default()
            .contains("Ship Stage 5"));

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let company_db = create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
            .await
            .unwrap();
        WorkOrderRepo::create(
            &company_db,
            &WorkOrder {
                work_order_id: "wo-stage5".to_string(),
                conversation_id: None,
                contract_hash: "a".repeat(64),
                plan_hash: "b".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: opc_id.clone(),
                mission_intent: "Stage 5 reporting".to_string(),
                selected_agents: vec!["agent-founder-01".to_string()],
                selected_executors: vec![],
                required_skills: vec![],
                track: "green".to_string(),
                status: WorkOrderStatus::Completed,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec!["delete".to_string()],
                risk_summary: "stage5".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .await
        .unwrap();
        company_db.close().await;
        WorkerRunRepo::create(
            &pool,
            &opc_id,
            "run-stage5-a",
            "wo-stage5",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-stage5",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            now as i64,
            Some(now as i64 + 10),
        )
        .await
        .unwrap();
        WorkerRunRepo::record_summary(&pool, "run-stage5-a", 10, 20, 30, 0.25, 10)
            .await
            .unwrap();

        let kpi = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/employees/agent-founder-01/kpi"
                    ))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": "wo-stage5",
                            "scores": {"completion": 95, "speed": 88, "clarity": 90},
                            "reviewer": "agent-risk-01",
                            "comment": "solid"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(kpi.status(), StatusCode::OK);

        let kpi_list = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{opc_id}/employees/agent-founder-01/kpi"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(kpi_list.status(), StatusCode::OK);

        let quota = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/companies/{opc_id}/cost/quota"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "department": "FounderOffice",
                            "token_quota": 1000
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(quota.status(), StatusCode::OK);

        let report = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/reports/generate"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"period": "daily"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report.status(), StatusCode::OK);
        let report_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(report.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let report_id = report_body["report_id"].as_str().unwrap();
        assert!(root
            .join(&opc_id)
            .join("reports")
            .join(format!("{report_id}.md"))
            .exists());
        let report_detail = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/reports/{report_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report_detail.status(), StatusCode::OK);
        let report_detail_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(report_detail.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(report_detail_body["report_md"]
            .as_str()
            .unwrap_or_default()
            .contains("FounderOffice / agent-founder-01: 30 tokens / $0.2500"));
        assert_eq!(
            report_detail_body["token_usage"]["by_agent"][0]["tokens"],
            serde_json::json!(30)
        );
        assert_eq!(
            report_detail_body["token_usage"]["by_agent"][0]["department"],
            serde_json::json!("FounderOffice")
        );

        let cost = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/cost"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cost.status(), StatusCode::OK);
        let cost_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(cost.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(cost_body["total"]["tokens"], 30);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn create_meeting_returns_error_when_provider_generation_is_unavailable() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        configure_active_mock_provider(&pool).await;
        let root = std::env::temp_dir().join(format!(
            "coevo-company-org-provider-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool.clone(), root.clone()));
        let opc_id = create_company(&app).await;

        let seed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed.status(), StatusCode::OK);

        let meeting = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/meetings"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "topic": "Ship Stage 5",
                            "participants": ["agent-founder-01", "agent-critic-01", "agent-risk-01"],
                            "close_mode": "vote"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(meeting.status(), StatusCode::BAD_GATEWAY);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(meeting.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("meetings require a real configured provider"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn company_organization_routes_reject_malformed_opc_id() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-org-bad-opc-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool, root.clone());

        let (status, Json(body)) = list_meetings(State(state), Path("../escape".to_string())).await;

        std::fs::remove_dir_all(root).ok();

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("opc_id must be a plain identifier"));
    }

    #[test]
    fn parse_meeting_draft_requires_explicit_responsibility_anchor() {
        let json = serde_json::json!({
            "transcript": [
                {
                    "speaker": "agent-founder-01",
                    "message": "B2B gives us more predictable revenue."
                },
                {
                    "speaker": "agent-pm-01",
                    "message": "We can adapt the current roadmap to team workflows."
                },
                {
                    "speaker": "agent-critic-01",
                    "message": "I oppose until we address long sales cycles and enterprise trust risk."
                }
            ],
            "resolution_md": "Run a staged B2B transition with explicit risk review."
        });

        let parsed = parse_meeting_draft(
            &json,
            &[
                "agent-founder-01".to_string(),
                "agent-pm-01".to_string(),
                "agent-critic-01".to_string(),
            ],
        );

        assert!(parsed.is_none());
    }

    #[tokio::test]
    async fn cost_and_kpi_are_company_scoped() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-org-scope-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool.clone(), root.clone()));
        let company_a = create_company(&app).await;
        let company_b = create_company(&app).await;

        for opc_id in [&company_a, &company_b] {
            let seed = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/companies/{opc_id}/employees/seed"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(seed.status(), StatusCode::OK);
        }

        let now = chrono::Utc::now().timestamp_millis() as u64;
        for (work_order_id, opc_id, run_id, tokens, cost) in [
            ("wo-a", company_a.clone(), "run-a", 40, 0.4),
            ("wo-b", company_b.clone(), "run-b", 70, 0.7),
        ] {
            let company_db = create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
                .await
                .unwrap();
            WorkOrderRepo::create(
                &company_db,
                &WorkOrder {
                    work_order_id: work_order_id.to_string(),
                    conversation_id: None,
                    contract_hash: "a".repeat(64),
                    plan_hash: "b".repeat(64),
                    user_id: "default-founder".to_string(),
                    opc_id: opc_id.clone(),
                    mission_intent: "company scoped stats".to_string(),
                    selected_agents: vec!["agent-founder-01".to_string()],
                    selected_executors: vec![],
                    required_skills: vec![],
                    track: "green".to_string(),
                    status: WorkOrderStatus::Completed,
                    allowed_actions: vec!["read".to_string()],
                    restricted_actions: vec![],
                    risk_summary: "scope".to_string(),
                    governance_proposal: None,
                    governance_verdict: None,
                    created_at_ms: now,
                    updated_at_ms: now,
                },
            )
            .await
            .unwrap();
            company_db.close().await;
            WorkerRunRepo::create(
                &pool,
                &opc_id,
                run_id,
                work_order_id,
                "agent-founder-01",
                "worker-agent-founder-01",
                &format!("session-{run_id}"),
                "Completed",
                "{}",
                "[]",
                "[]",
                None,
                now as i64,
                Some(now as i64 + 5),
            )
            .await
            .unwrap();
            WorkerRunRepo::record_summary(&pool, run_id, 0, 0, tokens, cost, 5)
                .await
                .unwrap();
        }

        let kpi = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{company_a}/employees/agent-founder-01/kpi"
                    ))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": "wo-a",
                            "scores": {"completion": 91},
                            "reviewer": "agent-risk-01"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(kpi.status(), StatusCode::OK);

        let company_a_cost = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{company_a}/cost"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let company_a_cost_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_a_cost.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(company_a_cost_body["total"]["tokens"], 40);

        let company_b_cost = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{company_b}/cost"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let company_b_cost_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_b_cost.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(company_b_cost_body["total"]["tokens"], 70);

        let company_b_kpi = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/companies/{company_b}/employees/agent-founder-01/kpi"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let company_b_kpi_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_b_kpi.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(company_b_kpi_body.as_array().unwrap().len(), 0);

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn cost_and_reports_do_not_mix_usage_when_work_order_ids_collide() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-org-collision-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool.clone(), root.clone()));
        let company_a = create_company(&app).await;
        let company_b = create_company(&app).await;

        for opc_id in [&company_a, &company_b] {
            let seed = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/companies/{opc_id}/employees/seed"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(seed.status(), StatusCode::OK);
        }

        let now = chrono::Utc::now().timestamp_millis() as u64;
        for (opc_id, run_id, tokens, cost) in [
            (company_a.clone(), "run-collision-a", 40, 0.4),
            (company_b.clone(), "run-collision-b", 70, 0.7),
        ] {
            let company_db = create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
                .await
                .unwrap();
            WorkOrderRepo::create(
                &company_db,
                &WorkOrder {
                    work_order_id: "wo-shared-usage".to_string(),
                    conversation_id: None,
                    contract_hash: "a".repeat(64),
                    plan_hash: "b".repeat(64),
                    user_id: "default-founder".to_string(),
                    opc_id: opc_id.clone(),
                    mission_intent: "shared usage collision".to_string(),
                    selected_agents: vec!["agent-founder-01".to_string()],
                    selected_executors: vec![],
                    required_skills: vec![],
                    track: "green".to_string(),
                    status: WorkOrderStatus::Completed,
                    allowed_actions: vec!["read".to_string()],
                    restricted_actions: vec![],
                    risk_summary: "collision".to_string(),
                    governance_proposal: None,
                    governance_verdict: None,
                    created_at_ms: now,
                    updated_at_ms: now,
                },
            )
            .await
            .unwrap();
            company_db.close().await;

            WorkerRunRepo::create(
                &pool,
                &opc_id,
                run_id,
                "wo-shared-usage",
                "agent-founder-01",
                "worker-agent-founder-01",
                &format!("session-{run_id}"),
                "Completed",
                "{}",
                "[]",
                "[]",
                None,
                now as i64,
                Some(now as i64 + 5),
            )
            .await
            .unwrap();
            WorkerRunRepo::record_summary(&pool, run_id, 0, 0, tokens, cost, 5)
                .await
                .unwrap();
        }

        let company_a_cost = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{company_a}/cost"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let company_a_cost_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(company_a_cost.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(company_a_cost_body["total"]["tokens"], 40);

        let report = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{company_a}/reports/generate"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"period": "daily"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report.status(), StatusCode::OK);
        let report_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(report.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let report_id = report_body["report_id"].as_str().unwrap();

        let report_detail = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{company_a}/reports/{report_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report_detail.status(), StatusCode::OK);
        let report_detail_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(report_detail.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            report_detail_body["token_usage"]["by_agent"][0]["tokens"],
            serde_json::json!(40)
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn generate_report_filters_kpi_and_usage_by_period_window() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-company-org-period-{}", uuid::Uuid::new_v4()));
        let app = build_router(AppState::new(pool.clone(), root.clone()));
        let opc_id = create_company(&app).await;

        let seed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed.status(), StatusCode::OK);

        let now = chrono::Utc::now().timestamp_millis();
        let yesterday = now - 86_400_000;
        let company_db = create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
            .await
            .unwrap();
        for (work_order_id, created_at_ms) in [("wo-old", yesterday as u64), ("wo-new", now as u64)]
        {
            WorkOrderRepo::create(
                &company_db,
                &WorkOrder {
                    work_order_id: work_order_id.to_string(),
                    conversation_id: None,
                    contract_hash: "a".repeat(64),
                    plan_hash: "b".repeat(64),
                    user_id: "default-founder".to_string(),
                    opc_id: opc_id.clone(),
                    mission_intent: "period filter".to_string(),
                    selected_agents: vec!["agent-founder-01".to_string()],
                    selected_executors: vec![],
                    required_skills: vec![],
                    track: "green".to_string(),
                    status: WorkOrderStatus::Completed,
                    allowed_actions: vec!["read".to_string()],
                    restricted_actions: vec![],
                    risk_summary: "scope".to_string(),
                    governance_proposal: None,
                    governance_verdict: None,
                    created_at_ms,
                    updated_at_ms: created_at_ms,
                },
            )
            .await
            .unwrap();
        }

        sqlx::query(
            "INSERT INTO kpi_records (
                kpi_id, agent_id, work_order_id, reviewer_agent_id, scores_json, comment, created_at_ms
            ) VALUES (?,?,?,?,?,?,?)",
        )
        .bind("kpi-old")
        .bind("agent-founder-01")
        .bind("wo-old")
        .bind("agent-risk-01")
        .bind(serde_json::json!({"completion": 50}).to_string())
        .bind("old")
        .bind(yesterday)
        .execute(&company_db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO kpi_records (
                kpi_id, agent_id, work_order_id, reviewer_agent_id, scores_json, comment, created_at_ms
            ) VALUES (?,?,?,?,?,?,?)",
        )
        .bind("kpi-new")
        .bind("agent-founder-01")
        .bind("wo-new")
        .bind("agent-risk-01")
        .bind(serde_json::json!({"completion": 90}).to_string())
        .bind("new")
        .bind(now)
        .execute(&company_db)
        .await
        .unwrap();
        company_db.close().await;

        WorkerRunRepo::create(
            &pool,
            &opc_id,
            "run-old",
            "wo-old",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-old",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            yesterday,
            Some(yesterday + 10),
        )
        .await
        .unwrap();
        WorkerRunRepo::record_summary(&pool, "run-old", 0, 0, 40, 0.4, 10)
            .await
            .unwrap();
        WorkerRunRepo::create(
            &pool,
            &opc_id,
            "run-new",
            "wo-new",
            "agent-founder-01",
            "worker-agent-founder-01",
            "session-new",
            "Completed",
            "{}",
            "[]",
            "[]",
            None,
            now,
            Some(now + 10),
        )
        .await
        .unwrap();
        WorkerRunRepo::record_summary(&pool, "run-new", 0, 0, 70, 0.7, 10)
            .await
            .unwrap();

        let report = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/reports/generate"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"period": "daily"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report.status(), StatusCode::OK);
        let report_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(report.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let report_id = report_body["report_id"].as_str().unwrap();

        let report_detail = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/companies/{opc_id}/reports/{report_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report_detail.status(), StatusCode::OK);
        let report_detail_body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(report_detail.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let report_md = report_detail_body["report_md"].as_str().unwrap_or_default();
        assert!(report_md.contains("70 tokens / $0.7000"));
        assert!(!report_md.contains("40 tokens / $0.4000"));
        assert_eq!(
            report_detail_body["kpi_summary"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            report_detail_body["kpi_summary"][0]["scores"]["completion"],
            serde_json::json!(90)
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn generate_report_rejects_empty_period_window() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-org-empty-report-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool, root.clone()));
        let opc_id = create_company(&app).await;

        let seed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed.status(), StatusCode::OK);

        let report = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/reports/generate"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"period": "daily"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report.status(), StatusCode::CONFLICT);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(report.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("REPORT_SOURCE_EMPTY"));

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn create_employee_kpi_accepts_company_local_work_order() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "coevo-company-org-kpi-local-{}",
            uuid::Uuid::new_v4()
        ));
        let app = build_router(AppState::new(pool.clone(), root.clone()));
        let opc_id = create_company(&app).await;

        let seed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{opc_id}/employees/seed"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(seed.status(), StatusCode::OK);

        let company_pool = create_pool(&root.join(&opc_id).join("data.db").to_string_lossy())
            .await
            .unwrap();
        let now = chrono::Utc::now().timestamp_millis() as u64;
        WorkOrderRepo::create(
            &company_pool,
            &WorkOrder {
                work_order_id: "wo-company-local-kpi".to_string(),
                conversation_id: None,
                contract_hash: "a".repeat(64),
                plan_hash: "b".repeat(64),
                user_id: "default-founder".to_string(),
                opc_id: opc_id.clone(),
                mission_intent: "company local kpi".to_string(),
                selected_agents: vec!["agent-founder-01".to_string()],
                selected_executors: vec![],
                required_skills: vec![],
                track: "green".to_string(),
                status: WorkOrderStatus::Completed,
                allowed_actions: vec!["read".to_string()],
                restricted_actions: vec![],
                risk_summary: "scope".to_string(),
                governance_proposal: None,
                governance_verdict: None,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .await
        .unwrap();
        company_pool.close().await;

        let kpi = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/companies/{opc_id}/employees/agent-founder-01/kpi"
                    ))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "work_order_id": "wo-company-local-kpi",
                            "scores": {"completion": 93, "clarity": 90},
                            "reviewer": "agent-risk-01",
                            "comment": "company local"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(kpi.status(), StatusCode::OK);

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn parse_meeting_draft_accepts_string_transcript_from_real_provider() {
        let json = serde_json::json!({
            "transcript": "agent-founder-01: We should test the enterprise motion carefully.\nagent-pm-01: The product fit is stronger in multi-seat workflows.\nagent-critic-01: We still need a narrower rollout because migration risk is unresolved.",
            "resolution_md": "Adopt a staged B2B trial with explicit migration checkpoints.",
            "responsibility_anchor": "founder"
        });

        let parsed = parse_meeting_draft(
            &json,
            &[
                "agent-founder-01".to_string(),
                "agent-pm-01".to_string(),
                "agent-critic-01".to_string(),
            ],
        )
        .expect("meeting draft should parse");

        assert_eq!(parsed.transcript.len(), 3);
        assert_eq!(parsed.transcript[2].agent_id, "agent-critic-01");
        assert_eq!(parsed.transcript[2].stance, "oppose");
        assert!(parsed.transcript[2].text.contains("migration risk"));
        assert!(parsed.resolution_md.contains("staged B2B trial"));
    }

    #[test]
    fn parse_meeting_draft_accepts_agent_statement_shape_from_real_provider() {
        let json = serde_json::json!({
            "transcript": [
                {
                    "agent": "agent-founder-01",
                    "statement": "B2B gives us clearer pricing power."
                },
                {
                    "agent": "agent-pm-01",
                    "statement": "The current roadmap already supports team workflows."
                },
                {
                    "agent": "agent-critic-01",
                    "statement": "I oppose the shift until churn and migration risk are modeled."
                }
            ],
            "resolution_md": "Run a staged B2B transition with a churn checkpoint.",
            "responsibility_anchor": "agent-founder-01"
        });

        let parsed = parse_meeting_draft(
            &json,
            &[
                "agent-founder-01".to_string(),
                "agent-pm-01".to_string(),
                "agent-critic-01".to_string(),
            ],
        )
        .expect("meeting draft should parse");

        assert_eq!(parsed.transcript.len(), 3);
        assert_eq!(
            parsed.transcript[0].text,
            "B2B gives us clearer pricing power."
        );
        assert_eq!(parsed.transcript[2].stance, "oppose");
        assert!(parsed.transcript[2].text.contains("migration risk"));
    }

    #[test]
    fn parse_meeting_draft_accepts_role_participant_and_name_variants() {
        let json = serde_json::json!({
            "transcript": [
                {
                    "role": "agent-founder-01",
                    "content": "B2B gives us stronger contract value."
                },
                {
                    "participant": "agent-pm-01",
                    "statement": "The roadmap already supports multi-seat workflows."
                },
                {
                    "name": "agent-critic-01",
                    "content": "I oppose until churn and sales-cycle risk are modeled."
                }
            ],
            "resolution_md": "Run a staged B2B trial with explicit risk review.",
            "responsibility_anchor": "agent-founder-01"
        });

        let parsed = parse_meeting_draft(
            &json,
            &[
                "agent-founder-01".to_string(),
                "agent-pm-01".to_string(),
                "agent-critic-01".to_string(),
            ],
        )
        .expect("meeting draft should parse");

        assert_eq!(parsed.transcript.len(), 3);
        assert_eq!(
            parsed.transcript[0].text,
            "B2B gives us stronger contract value."
        );
        assert_eq!(parsed.transcript[1].agent_id, "agent-pm-01");
        assert_eq!(parsed.transcript[2].stance, "oppose");
        assert!(parsed.transcript[2].text.contains("sales-cycle risk"));
    }

    #[test]
    fn parse_meeting_draft_accepts_speaker_message_shape_from_real_provider() {
        let json = serde_json::json!({
            "transcript": [
                {
                    "speaker": "agent-founder-01",
                    "message": "B2B gives us more predictable revenue."
                },
                {
                    "speaker": "agent-pm-01",
                    "message": "We can adapt the current roadmap to team workflows."
                },
                {
                    "speaker": "agent-critic-01",
                    "message": "I oppose until we address long sales cycles and enterprise trust risk."
                }
            ],
            "resolution_md": "Run a staged B2B transition with explicit risk review.",
            "responsibility_anchor": "agent-founder-01"
        });

        let parsed = parse_meeting_draft(
            &json,
            &[
                "agent-founder-01".to_string(),
                "agent-pm-01".to_string(),
                "agent-critic-01".to_string(),
            ],
        )
        .expect("meeting draft should parse");

        assert_eq!(parsed.transcript.len(), 3);
        assert_eq!(parsed.transcript[0].agent_id, "agent-founder-01");
        assert_eq!(parsed.transcript[2].stance, "oppose");
        assert!(parsed.transcript[2].text.contains("enterprise trust risk"));
    }

    #[test]
    fn parse_meeting_draft_normalizes_role_labels_from_real_provider() {
        let json = serde_json::json!({
            "transcript": [
                {
                    "participant": "Founder",
                    "statement": "B2B gives us higher lifetime value."
                },
                {
                    "participant": "Product Manager",
                    "statement": "The roadmap already supports multi-seat workflows."
                },
                {
                    "participant": "Risk Analyst",
                    "statement": "I oppose until sales-cycle and migration risk are modeled."
                }
            ],
            "resolution_md": "Run a staged B2B trial with explicit risk review.",
            "responsibility_anchor": "agent-pm-01"
        });

        let parsed = parse_meeting_draft(
            &json,
            &[
                "agent-founder-01".to_string(),
                "agent-pm-01".to_string(),
                "agent-critic-01".to_string(),
            ],
        )
        .expect("meeting draft should parse");

        assert_eq!(parsed.transcript.len(), 3);
        assert_eq!(parsed.transcript[0].agent_id, "agent-founder-01");
        assert_eq!(parsed.transcript[1].agent_id, "agent-pm-01");
        assert_eq!(parsed.transcript[2].agent_id, "agent-critic-01");
        assert_eq!(parsed.transcript[2].stance, "oppose");
    }

    #[test]
    fn parse_meeting_draft_accepts_resolution_aliases_from_real_provider() {
        let json = serde_json::json!({
            "transcript": [
                {
                    "speaker": "agent-founder-01",
                    "message": "B2B gives us more predictable revenue."
                },
                {
                    "speaker": "agent-pm-01",
                    "message": "We can adapt the current roadmap to team workflows."
                },
                {
                    "speaker": "agent-critic-01",
                    "message": "I oppose until we address long sales cycles and enterprise trust risk."
                }
            ],
            "resolution": "Run a staged B2B transition with explicit risk review.",
            "decision_md": "Run a staged B2B transition with explicit risk review.",
            "owner": "agent-founder-01"
        });

        let parsed = parse_meeting_draft(
            &json,
            &[
                "agent-founder-01".to_string(),
                "agent-pm-01".to_string(),
                "agent-critic-01".to_string(),
            ],
        )
        .expect("meeting draft should parse alias fields");

        assert!(parsed.resolution_md.contains("staged B2B transition"));
        assert_eq!(parsed.responsibility_anchor, "agent-founder-01");
    }

    #[test]
    fn parse_meeting_draft_rejects_missing_responsibility_anchor() {
        let json = serde_json::json!({
            "transcript": [
                {
                    "speaker": "agent-founder-01",
                    "message": "B2B gives us more predictable revenue."
                },
                {
                    "speaker": "agent-pm-01",
                    "message": "We can adapt the current roadmap to team workflows."
                },
                {
                    "speaker": "agent-critic-01",
                    "message": "I oppose until we address long sales cycles and enterprise trust risk."
                }
            ],
            "summary_md": "Run a staged B2B transition with explicit risk review."
        });

        let parsed = parse_meeting_draft(
            &json,
            &[
                "agent-founder-01".to_string(),
                "agent-pm-01".to_string(),
                "agent-critic-01".to_string(),
            ],
        );

        assert!(parsed.is_none());
    }

    #[test]
    fn meeting_prompt_includes_employee_persona_markdown_sections() {
        let root =
            std::env::temp_dir().join(format!("coevo-meeting-persona-{}", uuid::Uuid::new_v4()));
        let workspace = coevo_store::company_workspace::CompanyWorkspaceManager::new(root.clone());
        let opc_id = "opc-meeting-persona";
        let agent_id = "agent-founder-01";
        workspace
            .ensure_company_employee_skeleton(opc_id, agent_id)
            .unwrap();
        let employee_dir = workspace.company_employee_dir(opc_id, agent_id);
        std::fs::write(employee_dir.join("prompt.md"), "Prompt body").unwrap();
        std::fs::write(employee_dir.join("identity.md"), "Identity body").unwrap();
        std::fs::write(employee_dir.join("soul.md"), "Soul body").unwrap();
        std::fs::write(employee_dir.join("agents.md"), "Agents body").unwrap();
        std::fs::write(employee_dir.join("owner.md"), "Owner body").unwrap();
        std::fs::write(employee_dir.join("tools.md"), "Tools body").unwrap();

        let persona = load_meeting_participant_persona(&workspace, opc_id, agent_id);

        std::fs::remove_dir_all(root).ok();

        assert!(persona.contains("Participant: agent-founder-01"));
        assert!(persona.contains("Prompt body"));
        assert!(persona.contains("[identity.md]\nIdentity body"));
        assert!(persona.contains("[soul.md]\nSoul body"));
        assert!(persona.contains("[agents.md]\nAgents body"));
        assert!(persona.contains("[owner.md]\nOwner body"));
        assert!(persona.contains("[tools.md]\nTools body"));
    }

    #[test]
    fn meeting_prompt_includes_all_participant_personas_in_user_message() {
        let root = std::env::temp_dir().join(format!(
            "coevo-meeting-personas-request-{}",
            uuid::Uuid::new_v4()
        ));
        let workspace = coevo_store::company_workspace::CompanyWorkspaceManager::new(root.clone());
        let opc_id = "opc-meeting-request";
        for (agent_id, prompt_body, identity_body) in [
            ("agent-founder-01", "Founder prompt", "Founder identity"),
            ("agent-critic-01", "Critic prompt", "Critic identity"),
        ] {
            workspace
                .ensure_company_employee_skeleton(opc_id, agent_id)
                .unwrap();
            let employee_dir = workspace.company_employee_dir(opc_id, agent_id);
            std::fs::write(employee_dir.join("prompt.md"), prompt_body).unwrap();
            std::fs::write(employee_dir.join("identity.md"), identity_body).unwrap();
        }

        let joined = [
            load_meeting_participant_persona(&workspace, opc_id, "agent-founder-01"),
            load_meeting_participant_persona(&workspace, opc_id, "agent-critic-01"),
        ]
        .join("\n\n---\n\n");
        let request_message = format!(
            "Topic: Stage 5\nClose mode: vote\nParticipants: agent-founder-01, agent-critic-01\nParticipant personas:\n{}\nReturn transcript, resolution_md, and responsibility_anchor.",
            joined
        );

        std::fs::remove_dir_all(root).ok();

        assert!(request_message.contains("Participant personas:"));
        assert!(request_message.contains("Participant: agent-founder-01"));
        assert!(request_message.contains("Founder prompt"));
        assert!(request_message.contains("[identity.md]\nFounder identity"));
        assert!(request_message.contains("Participant: agent-critic-01"));
        assert!(request_message.contains("Critic prompt"));
        assert!(request_message.contains("[identity.md]\nCritic identity"));
    }

    #[test]
    fn meeting_turn_prompt_includes_prior_transcript_context() {
        let root = std::env::temp_dir().join(format!(
            "coevo-meeting-turn-prompt-{}",
            uuid::Uuid::new_v4()
        ));
        let workspace = coevo_store::company_workspace::CompanyWorkspaceManager::new(root.clone());
        let opc_id = "opc-meeting-turn";
        workspace
            .ensure_company_employee_skeleton(opc_id, "agent-founder-01")
            .unwrap();
        workspace
            .ensure_company_employee_skeleton(opc_id, "agent-critic-01")
            .unwrap();
        std::fs::write(
            workspace.company_employee_prompt_path(opc_id, "agent-founder-01"),
            "Founder prompt",
        )
        .unwrap();
        std::fs::write(
            workspace.company_employee_prompt_path(opc_id, "agent-critic-01"),
            "Critic prompt",
        )
        .unwrap();

        let prior_turns = vec![MeetingTurnDraft {
            agent_id: "agent-founder-01".to_string(),
            stance: "support".to_string(),
            text: "We should test an enterprise rollout in stages.".to_string(),
        }];
        let prompt = build_meeting_turn_user_prompt(
            &workspace,
            opc_id,
            "Should we shift from B2C to B2B?",
            "vote",
            &[
                "agent-founder-01".to_string(),
                "agent-critic-01".to_string(),
            ],
            "agent-critic-01",
            &prior_turns,
        );

        std::fs::remove_dir_all(root).ok();

        assert!(prompt.contains("Current participant: agent-critic-01"));
        assert!(prompt.contains("Assigned stance: oppose"));
        assert!(prompt.contains("Critic prompt"));
        assert!(prompt.contains("Prior transcript:"));
        assert!(prompt.contains(
            "agent-founder-01 [support]: We should test an enterprise rollout in stages."
        ));
    }
}
