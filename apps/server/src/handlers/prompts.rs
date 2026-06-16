use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use coevo_store::repos_opc::agent_employee_repo;
use coevo_store::{migrate::run_migrations, pool::create_pool};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;

fn prompt_body_storage_value(is_employee_prompt: bool, content: &str) -> &str {
    if is_employee_prompt {
        ""
    } else {
        content
    }
}

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

const LEGACY_OPC_ID_HEADER: &str = "x-coevo-opc-id";

fn legacy_opc_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get(LEGACY_OPC_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| is_plain_identifier(value))
        .map(ToString::to_string)
}

fn validate_prompt_id(prompt_id: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if prompt_id.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error":"prompt_id is required"})),
        ));
    }
    if !is_plain_identifier(prompt_id) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error":"prompt_id must be a plain employee identifier"})),
        ));
    }
    Ok(())
}

fn require_legacy_opc_id(
    headers: &HeaderMap,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    legacy_opc_id(headers).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "LEGACY_OPC_ID_REQUIRED: header {LEGACY_OPC_ID_HEADER} is required for legacy /opc/prompts routes"
                )
            })),
        )
    })
}

async fn company_pool(
    state: &AppState,
    opc_id: &str,
) -> Result<sqlx::SqlitePool, (StatusCode, Json<serde_json::Value>)> {
    let company_dir = state.company_workspace.company_dir(opc_id);
    if !company_dir.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"company not found"})),
        ));
    }
    let pool = create_pool(
        &state
            .company_workspace
            .company_db_path(opc_id)
            .to_string_lossy(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
    })?;
    run_migrations(&pool).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
    })?;
    Ok(pool)
}

async fn sync_company_employee_prompt_files(
    state: &AppState,
    pool: &sqlx::SqlitePool,
    opc_id: &str,
    prompt_id: &str,
    version_number: i32,
    content: &str,
    publish_current: bool,
) -> Result<(), String> {
    if !is_plain_identifier(prompt_id) {
        return Err("prompt_id must be a plain employee identifier".to_string());
    }
    let employee = agent_employee_repo::AgentEmployeeRepo::get(pool, prompt_id)
        .await
        .map_err(|e| e.to_string())?;
    if employee.is_none() {
        return Ok(());
    }
    state
        .company_workspace
        .ensure_company_employee_skeleton(opc_id, prompt_id)
        .map_err(|e| e.to_string())?;
    std::fs::write(
        state
            .company_workspace
            .company_employee_prompt_version_path(opc_id, prompt_id, version_number),
        content,
    )
    .map_err(|e| e.to_string())?;
    if publish_current {
        std::fs::write(
            state
                .company_workspace
                .company_employee_prompt_path(opc_id, prompt_id),
            content,
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            state
                .company_workspace
                .company_employee_prompt_current_version_path(opc_id, prompt_id),
            version_number.to_string(),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn is_company_employee_prompt(
    pool: &sqlx::SqlitePool,
    prompt_id: &str,
) -> Result<bool, sqlx::Error> {
    agent_employee_repo::AgentEmployeeRepo::exists(pool, prompt_id).await
}

async fn hydrate_company_prompt_version(
    state: &AppState,
    pool: &sqlx::SqlitePool,
    opc_id: &str,
    version: PromptVersion,
) -> PromptVersion {
    let is_employee_prompt = is_company_employee_prompt(pool, &version.prompt_id)
        .await
        .unwrap_or(false);
    if !is_employee_prompt {
        return version;
    }
    let path = state
        .company_workspace
        .company_employee_prompt_version_path(opc_id, &version.prompt_id, version.version_number);
    let content = std::fs::read_to_string(path).unwrap_or_else(|_| version.content.clone());
    PromptVersion { content, ..version }
}

async fn next_version_number(pool: &sqlx::SqlitePool, prompt_id: &str) -> Result<i32, sqlx::Error> {
    let max_version: Option<i32> =
        sqlx::query_scalar("SELECT MAX(version_number) FROM prompt_versions WHERE prompt_id = ?")
            .bind(prompt_id)
            .fetch_optional(pool)
            .await?;
    Ok(max_version.unwrap_or(0) + 1)
}

pub async fn publish_prompt_version_number(
    pool: &sqlx::SqlitePool,
    prompt_id: &str,
    version_number: i32,
    content: &str,
    created_by: &str,
    change_summary: Option<&str>,
) -> Result<String, sqlx::Error> {
    let created_at = Utc::now().to_rfc3339();
    let is_employee_prompt = is_company_employee_prompt(pool, prompt_id).await?;
    let stored_content = prompt_body_storage_value(is_employee_prompt, content);
    sqlx::query(
        "UPDATE prompt_versions SET status = 'DRAFT' WHERE prompt_id = ? AND status = 'PUBLISHED'",
    )
    .bind(prompt_id)
    .execute(pool)
    .await?;

    let existing_id: Option<String> = sqlx::query_scalar(
        "SELECT version_id FROM prompt_versions WHERE prompt_id = ? AND version_number = ?",
    )
    .bind(prompt_id)
    .bind(version_number)
    .fetch_optional(pool)
    .await?;

    if let Some(version_id) = existing_id {
        sqlx::query(
            "UPDATE prompt_versions
             SET content = ?, status = 'PUBLISHED', variables = ?, created_by = ?, change_summary = ?
             WHERE version_id = ?",
        )
        .bind(stored_content)
        .bind("[]")
        .bind(created_by)
        .bind(change_summary)
        .bind(&version_id)
        .execute(pool)
        .await?;
        Ok(version_id)
    } else {
        let version_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO prompt_versions (version_id, prompt_id, version_number, content, variables, status, created_at, created_by, change_summary)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&version_id)
        .bind(prompt_id)
        .bind(version_number)
        .bind(stored_content)
        .bind("[]")
        .bind("PUBLISHED")
        .bind(&created_at)
        .bind(created_by)
        .bind(change_summary)
        .execute(pool)
        .await?;
        Ok(version_id)
    }
}

pub async fn publish_prompt_version_number_in_company(
    state: &AppState,
    pool: &sqlx::SqlitePool,
    opc_id: &str,
    prompt_id: &str,
    version_number: i32,
    content: &str,
    created_by: &str,
    change_summary: Option<&str>,
) -> Result<String, String> {
    let version_id = publish_prompt_version_number(
        pool,
        prompt_id,
        version_number,
        content,
        created_by,
        change_summary,
    )
    .await
    .map_err(|e| e.to_string())?;
    sync_company_employee_prompt_files(
        state,
        pool,
        opc_id,
        prompt_id,
        version_number,
        content,
        true,
    )
    .await?;
    Ok(version_id)
}

/// Record a new prompt version and immediately publish it. Reusable by other
/// handlers (e.g. skill-evolution approval) so the evolution upgrade path and
/// the prompt-version history converge into one source of truth.
pub async fn record_and_publish_version(
    state: &AppState,
    pool: &sqlx::SqlitePool,
    opc_id: &str,
    prompt_id: &str,
    content: &str,
    created_by: &str,
    change_summary: Option<&str>,
) -> Result<String, sqlx::Error> {
    let version_number = next_version_number(pool, prompt_id).await?;
    publish_prompt_version_number_in_company(
        state,
        pool,
        opc_id,
        prompt_id,
        version_number,
        content,
        created_by,
        change_summary,
    )
    .await
    .map_err(sqlx::Error::Protocol)
}

pub async fn create_prompt_version(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(req): Json<CreatePromptVersionRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let version_id = Uuid::new_v4().to_string();
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    if let Err(err) = validate_prompt_id(&req.prompt_id) {
        return err;
    }
    let pool = match company_pool(&state, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let db = &pool;
    let version_number = next_version_number(db, &req.prompt_id).await.unwrap_or(1);
    let created_at = Utc::now().to_rfc3339();
    let variables_json = serde_json::to_string(&req.variables).unwrap();
    let is_employee_prompt = is_company_employee_prompt(&pool, &req.prompt_id)
        .await
        .unwrap_or(false);

    sqlx::query(
        "INSERT INTO prompt_versions (version_id, prompt_id, version_number, content, variables, status, created_at, created_by, change_summary)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&version_id)
    .bind(&req.prompt_id)
    .bind(version_number)
    .bind(prompt_body_storage_value(is_employee_prompt, &req.content))
    .bind(&variables_json)
    .bind("DRAFT")
    .bind(&created_at)
    .bind("system")
    .bind(&req.change_summary)
    .execute(db)
    .await
    .unwrap();
    if let Err(e) = sync_company_employee_prompt_files(
        &state,
        &pool,
        &opc_id,
        &req.prompt_id,
        version_number,
        &req.content,
        false,
    )
    .await
    {
        pool.close().await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e})),
        );
    }

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
    pool.close().await;

    (StatusCode::OK, Json(serde_json::json!(version)))
}

pub async fn publish_prompt_version(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&state, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let db = &pool;
    let prompt_id: Option<String> =
        sqlx::query_scalar("SELECT prompt_id FROM prompt_versions WHERE version_id = ?")
            .bind(&version_id)
            .fetch_optional(db)
            .await
            .unwrap_or(None);

    if prompt_id.is_none() {
        pool.close().await;
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Version not found"
            })),
        );
    }
    let prompt_id = prompt_id.unwrap();
    if let Err(err) = validate_prompt_id(&prompt_id) {
        pool.close().await;
        return err;
    }

    sqlx::query(
        "UPDATE prompt_versions SET status = 'DRAFT' WHERE prompt_id = ? AND status = 'PUBLISHED'",
    )
    .bind(&prompt_id)
    .execute(db)
    .await
    .unwrap();

    sqlx::query("UPDATE prompt_versions SET status = 'PUBLISHED' WHERE version_id = ?")
        .bind(&version_id)
        .execute(db)
        .await
        .unwrap();
    let version: Option<PromptVersion> =
        sqlx::query_as("SELECT * FROM prompt_versions WHERE version_id = ?")
            .bind(&version_id)
            .fetch_optional(&pool)
            .await
            .unwrap_or(None);
    if let Some(version) = version {
        let version = hydrate_company_prompt_version(&state, &pool, &opc_id, version).await;
        if let Err(e) = sync_company_employee_prompt_files(
            &state,
            &pool,
            &opc_id,
            &version.prompt_id,
            version.version_number,
            &version.content,
            true,
        )
        .await
        {
            pool.close().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":e})),
            );
        }
    }
    pool.close().await;

    (StatusCode::OK, Json(serde_json::json!({ "success": true })))
}

pub async fn list_prompt_versions(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(prompt_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    if let Err(err) = validate_prompt_id(&prompt_id) {
        return err;
    }
    let pool = match company_pool(&state, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let db = &pool;
    let mut versions: Vec<PromptVersion> = sqlx::query_as(
        "SELECT * FROM prompt_versions WHERE prompt_id = ? ORDER BY version_number DESC",
    )
    .bind(&prompt_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();
    let mut hydrated = Vec::with_capacity(versions.len());
    for version in versions {
        hydrated.push(hydrate_company_prompt_version(&state, db, &opc_id, version).await);
    }
    versions = hydrated;
    pool.close().await;

    (StatusCode::OK, Json(serde_json::json!(versions)))
}

pub async fn get_prompt_version(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(version_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&state, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let db = &pool;
    let version: Option<PromptVersion> =
        sqlx::query_as("SELECT * FROM prompt_versions WHERE version_id = ?")
            .bind(&version_id)
            .fetch_optional(db)
            .await
            .unwrap_or(None);
    let version = match version {
        Some(v) => Some(hydrate_company_prompt_version(&state, db, &opc_id, v).await),
        None => None,
    };
    pool.close().await;

    match version {
        Some(v) => (StatusCode::OK, Json(serde_json::json!(v))),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Version not found"
            })),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{router::build_router, state::AppState};
    use axum::{
        body::Body,
        http::{HeaderMap, Request},
    };
    use coevo_core::opc::{
        AgentEmployee, AgentPassport, Department, LifecycleStatus, MemoryScope,
        ModelProviderProfile, PermissionBoundary,
    };
    use coevo_core::reputation::ReputationVector;
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};
    use tower::ServiceExt;

    fn employee_with_id(agent_id: &str) -> AgentEmployee {
        AgentEmployee {
            agent_id: agent_id.to_string(),
            display_name: "Shared Id".to_string(),
            department: Department::Product,
            role: "Product".to_string(),
            passport: AgentPassport {
                passport_id: format!("passport-{agent_id}"),
                issued_by: "test".to_string(),
                roles: vec!["product".to_string()],
                capabilities: vec!["read".to_string()],
                restrictions: vec![],
                expires_at_ms: None,
            },
            model_profile: ModelProviderProfile {
                provider: "mock".to_string(),
                base_url: String::new(),
                api_key_ref: String::new(),
                default_model: "mock-model".to_string(),
                fast_model: "mock-model".to_string(),
                reasoning_model: "mock-model".to_string(),
                structured_output_model: "mock-model".to_string(),
                timeout_ms: 1000,
                max_tokens: 256,
                max_cost_per_task_usd: 0.0,
            },
            tool_scopes: vec!["read".to_string()],
            memory_scope: MemoryScope::Company,
            permission_boundary: PermissionBoundary {
                max_risk_score: 0.3,
                can_write_fact: false,
                can_write_decision: false,
                can_access_network: false,
                can_access_filesystem: true,
                can_call_external_executor: false,
                can_propose_skill: false,
            },
            allowed_cognitive_layers: vec!["suggestion".to_string()],
            allowed_action_modes: vec!["read".to_string()],
            risk_ceiling: 0.3,
            reputation_vector: ReputationVector::new(agent_id.to_string()),
            supervisor_agent_id: None,
            lifecycle_status: LifecycleStatus::Draft,
            system_prompt: String::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[tokio::test]
    async fn get_prompt_version_keeps_db_content_when_employee_is_created_later() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-prompts-history-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());
        let company = state
            .company_workspace
            .create_company(
                "Prompt History Co",
                Some("preserve prompt history"),
                "default-founder",
            )
            .await
            .unwrap();
        let app = build_router(state.clone());

        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, company.opc_id.parse().unwrap());

        let request = CreatePromptVersionRequest {
            prompt_id: "shared-id".to_string(),
            content: "db-only prompt content".to_string(),
            variables: vec![],
            change_summary: Some("seed".to_string()),
        };
        let (create_status, Json(created)) =
            create_prompt_version(headers.clone(), State(state.clone()), Json(request)).await;
        assert_eq!(create_status, StatusCode::OK, "{created:?}");
        let version_id = created["version_id"].as_str().unwrap().to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/companies/{}/employees", company.opc_id))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&employee_with_id("shared-id")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let (get_status, Json(version)) =
            get_prompt_version(headers, State(state), Path(version_id)).await;
        assert_eq!(get_status, StatusCode::OK, "{version:?}");
        assert_eq!(version["content"], "db-only prompt content");

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn legacy_prompt_routes_isolate_versions_per_company_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root =
            std::env::temp_dir().join(format!("coevo-prompts-scope-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool.clone(), root.clone());

        let alpha = state
            .company_workspace
            .create_company(
                "Alpha Prompt Co",
                Some("alpha prompt scope"),
                "default-founder",
            )
            .await
            .unwrap();
        let beta = state
            .company_workspace
            .create_company(
                "Beta Prompt Co",
                Some("beta prompt scope"),
                "default-founder",
            )
            .await
            .unwrap();

        let mut alpha_headers = HeaderMap::new();
        alpha_headers.insert(LEGACY_OPC_ID_HEADER, alpha.opc_id.parse().unwrap());
        let mut beta_headers = HeaderMap::new();
        beta_headers.insert(LEGACY_OPC_ID_HEADER, beta.opc_id.parse().unwrap());

        let (alpha_create_status, Json(alpha_created)) = create_prompt_version(
            alpha_headers.clone(),
            State(state.clone()),
            Json(CreatePromptVersionRequest {
                prompt_id: "shared-id".to_string(),
                content: "alpha prompt content".to_string(),
                variables: vec![],
                change_summary: Some("alpha".to_string()),
            }),
        )
        .await;
        assert_eq!(alpha_create_status, StatusCode::OK, "{alpha_created:?}");
        let alpha_version_id = alpha_created["version_id"].as_str().unwrap().to_string();

        let (beta_create_status, Json(beta_created)) = create_prompt_version(
            beta_headers.clone(),
            State(state.clone()),
            Json(CreatePromptVersionRequest {
                prompt_id: "shared-id".to_string(),
                content: "beta prompt content".to_string(),
                variables: vec![],
                change_summary: Some("beta".to_string()),
            }),
        )
        .await;
        assert_eq!(beta_create_status, StatusCode::OK, "{beta_created:?}");
        let beta_version_id = beta_created["version_id"].as_str().unwrap().to_string();

        let (alpha_list_status, Json(alpha_versions)) = list_prompt_versions(
            alpha_headers.clone(),
            State(state.clone()),
            Path("shared-id".to_string()),
        )
        .await;
        assert_eq!(alpha_list_status, StatusCode::OK, "{alpha_versions:?}");
        let alpha_versions = alpha_versions.as_array().unwrap();
        assert_eq!(alpha_versions.len(), 1);
        assert_eq!(alpha_versions[0]["content"], "alpha prompt content");

        let (beta_list_status, Json(beta_versions)) = list_prompt_versions(
            beta_headers.clone(),
            State(state.clone()),
            Path("shared-id".to_string()),
        )
        .await;
        assert_eq!(beta_list_status, StatusCode::OK, "{beta_versions:?}");
        let beta_versions = beta_versions.as_array().unwrap();
        assert_eq!(beta_versions.len(), 1);
        assert_eq!(beta_versions[0]["content"], "beta prompt content");

        let (cross_status, Json(cross_body)) =
            get_prompt_version(alpha_headers, State(state), Path(beta_version_id.clone())).await;
        assert_eq!(cross_status, StatusCode::NOT_FOUND, "{cross_body:?}");

        assert_ne!(alpha_version_id, beta_version_id);
        std::fs::remove_dir_all(root).ok();
    }
}
