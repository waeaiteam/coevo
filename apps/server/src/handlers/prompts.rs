use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PromptVersion {
    pub version_id: String,
    pub prompt_id: String,
    pub version_number: i32,
    pub content: String,
    pub variables: String,
    pub status: String,
    pub created_at: String,
    pub created_by: String,
    pub change_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVariable {
    pub name: String,
    pub var_type: String,
    pub default_value: Option<String>,
    pub required: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreatePromptVersionRequest {
    pub prompt_id: String,
    pub content: String,
    pub variables: Vec<PromptVariable>,
    pub change_summary: Option<String>,
}

/// Record a new prompt version and immediately publish it. Reusable by other
/// handlers (e.g. skill-evolution approval) so the evolution upgrade path and
/// the prompt-version history converge into one source of truth.
pub async fn record_and_publish_version(
    pool: &sqlx::SqlitePool,
    prompt_id: &str,
    content: &str,
    created_by: &str,
    change_summary: Option<&str>,
) -> Result<String, sqlx::Error> {
    let version_id = Uuid::new_v4().to_string();
    let max_version: Option<i32> =
        sqlx::query_scalar("SELECT MAX(version_number) FROM prompt_versions WHERE prompt_id = ?")
            .bind(prompt_id)
            .fetch_optional(pool)
            .await?;
    let version_number = max_version.unwrap_or(0) + 1;
    let created_at = Utc::now().to_rfc3339();

    // Demote any currently-published version for this prompt.
    sqlx::query(
        "UPDATE prompt_versions SET status = 'DRAFT' WHERE prompt_id = ? AND status = 'PUBLISHED'",
    )
    .bind(prompt_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO prompt_versions (version_id, prompt_id, version_number, content, variables, status, created_at, created_by, change_summary)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&version_id)
    .bind(prompt_id)
    .bind(version_number)
    .bind(content)
    .bind("[]")
    .bind("PUBLISHED")
    .bind(&created_at)
    .bind(created_by)
    .bind(change_summary)
    .execute(pool)
    .await?;

    Ok(version_id)
}

pub async fn create_prompt_version(
    State(state): State<AppState>,
    Json(req): Json<CreatePromptVersionRequest>,
) -> impl IntoResponse {
    let version_id = Uuid::new_v4().to_string();

    let max_version: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(version_number) FROM prompt_versions WHERE prompt_id = ?"
    )
    .bind(&req.prompt_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let version_number = max_version.unwrap_or(0) + 1;
    let created_at = Utc::now().to_rfc3339();
    let variables_json = serde_json::to_string(&req.variables).unwrap();

    sqlx::query(
        "INSERT INTO prompt_versions (version_id, prompt_id, version_number, content, variables, status, created_at, created_by, change_summary)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&version_id)
    .bind(&req.prompt_id)
    .bind(version_number)
    .bind(&req.content)
    .bind(&variables_json)
    .bind("DRAFT")
    .bind(&created_at)
    .bind("system")
    .bind(&req.change_summary)
    .execute(&state.pool)
    .await
    .unwrap();

    let version = PromptVersion {
        version_id: version_id.clone(),
        prompt_id: req.prompt_id.clone(),
        version_number,
        content: req.content.clone(),
        variables: variables_json,
        status: "DRAFT".to_string(),
        created_at,
        created_by: "system".to_string(),
        change_summary: req.change_summary.clone(),
    };

    Json(version)
}

pub async fn publish_prompt_version(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> impl IntoResponse {
    let prompt_id: Option<String> = sqlx::query_scalar(
        "SELECT prompt_id FROM prompt_versions WHERE version_id = ?"
    )
    .bind(&version_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if prompt_id.is_none() {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "Version not found"
        })));
    }

    sqlx::query(
        "UPDATE prompt_versions SET status = 'DRAFT' WHERE prompt_id = ? AND status = 'PUBLISHED'"
    )
    .bind(&prompt_id.unwrap())
    .execute(&state.pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE prompt_versions SET status = 'PUBLISHED' WHERE version_id = ?"
    )
    .bind(&version_id)
    .execute(&state.pool)
    .await
    .unwrap();

    (StatusCode::OK, Json(serde_json::json!({ "success": true })))
}

pub async fn list_prompt_versions(
    State(state): State<AppState>,
    Path(prompt_id): Path<String>,
) -> impl IntoResponse {
    let versions: Vec<PromptVersion> = sqlx::query_as(
        "SELECT * FROM prompt_versions WHERE prompt_id = ? ORDER BY version_number DESC"
    )
    .bind(&prompt_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    Json(versions)
}

pub async fn get_prompt_version(
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> impl IntoResponse {
    let version: Option<PromptVersion> = sqlx::query_as(
        "SELECT * FROM prompt_versions WHERE version_id = ?"
    )
    .bind(&version_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    match version {
        Some(v) => (StatusCode::OK, Json(serde_json::json!(v))),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "Version not found"
        }))),
    }
}
