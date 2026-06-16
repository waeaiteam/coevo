use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use coevo_store::repos_opc::conversation_repo::{
    ConversationMessage, ConversationRepo, ConversationThread,
};
use coevo_store::{migrate::run_migrations, pool::create_pool};
use serde::Deserialize;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err {
    ($code:expr, $msg:expr) => {
        ($code, Json(serde_json::json!({"error":$msg})))
    };
}

#[derive(Deserialize)]
pub struct CreateConversationRequest {
    pub conversation_id: Option<String>,
    pub opc_id: Option<String>,
    pub user_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub struct AppendMessageRequest {
    pub message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub linked_work_order_id: Option<String>,
}

async fn company_pool(
    state: &AppState,
    opc_id: &str,
) -> Result<sqlx::SqlitePool, (StatusCode, Json<serde_json::Value>)> {
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

const LEGACY_OPC_ID_HEADER: &str = "x-coevo-opc-id";

fn legacy_opc_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get(LEGACY_OPC_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| is_plain_identifier(value))
        .map(ToString::to_string)
}

fn require_legacy_opc_id(
    headers: &HeaderMap,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    legacy_opc_id(headers).ok_or_else(|| {
        err!(
            StatusCode::BAD_REQUEST,
            format!(
                "LEGACY_OPC_ID_REQUIRED: header {LEGACY_OPC_ID_HEADER} is required for legacy /opc/conversations routes"
            )
        )
    })
}

pub async fn list_conversations(
    headers: HeaderMap,
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = ConversationRepo::list_threads(&pool).await;
    pool.close().await;
    match result {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_conversation(
    headers: HeaderMap,
    State(s): State<AppState>,
    Json(req): Json<CreateConversationRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let now = chrono::Utc::now().timestamp_millis();
    let title = req
        .title
        .unwrap_or_else(|| "New OPC conversation".to_string())
        .trim()
        .to_string();
    if title.is_empty() {
        pool.close().await;
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "title required");
    }
    let thread = ConversationThread {
        conversation_id: req
            .conversation_id
            .unwrap_or_else(|| format!("conv-{}", uuid::Uuid::new_v4())),
        opc_id,
        user_id: req.user_id.unwrap_or_else(|| "default-founder".to_string()),
        title,
        status: "open".to_string(),
        created_at_ms: now,
        updated_at_ms: now,
    };
    let result = ConversationRepo::create_thread(&pool, &thread).await;
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::to_value(thread).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_conversation(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = ConversationRepo::get_thread(&pool, &id).await;
    pool.close().await;
    match result {
        Ok(Some(thread)) => ok!(serde_json::to_value(thread).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "conversation not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_conversation_messages(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    match ConversationRepo::get_thread(&pool, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, "conversation not found");
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    }
    let result = ConversationRepo::list_messages(&pool, &id).await;
    pool.close().await;
    match result {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn append_conversation_message(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AppendMessageRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(err) => return err,
    };
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    match ConversationRepo::get_thread(&pool, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, "conversation not found");
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    }
    let role = req.role.trim();
    if !matches!(role, "user" | "assistant" | "system") {
        pool.close().await;
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "role must be user, assistant, or system"
        );
    }
    let content = req.content.trim().to_string();
    if content.is_empty() {
        pool.close().await;
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "content required");
    }
    let message = ConversationMessage {
        message_id: req
            .message_id
            .unwrap_or_else(|| format!("msg-{}", uuid::Uuid::new_v4())),
        conversation_id: id,
        role: role.to_string(),
        content,
        linked_work_order_id: req.linked_work_order_id,
        created_at_ms: chrono::Utc::now().timestamp_millis(),
    };
    let result = ConversationRepo::append_message(&pool, &message).await;
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::to_value(message).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_company_conversations(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = ConversationRepo::list_threads(&pool).await;
    pool.close().await;
    match result {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_company_conversation(
    State(s): State<AppState>,
    Path(opc_id): Path<String>,
    Json(req): Json<CreateConversationRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let now = chrono::Utc::now().timestamp_millis();
    let title = req
        .title
        .unwrap_or_else(|| "New OPC conversation".to_string())
        .trim()
        .to_string();
    if title.is_empty() {
        pool.close().await;
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "title required");
    }
    let thread = ConversationThread {
        conversation_id: req
            .conversation_id
            .unwrap_or_else(|| format!("conv-{}", uuid::Uuid::new_v4())),
        opc_id,
        user_id: req.user_id.unwrap_or_else(|| "default-founder".to_string()),
        title,
        status: "open".to_string(),
        created_at_ms: now,
        updated_at_ms: now,
    };
    let result = ConversationRepo::create_thread(&pool, &thread).await;
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::to_value(thread).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_company_conversation(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let result = ConversationRepo::get_thread(&pool, &id).await;
    pool.close().await;
    match result {
        Ok(Some(thread)) => ok!(serde_json::to_value(thread).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "conversation not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_company_conversation_messages(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    let thread = ConversationRepo::get_thread(&pool, &id).await;
    match thread {
        Ok(Some(_)) => {}
        Ok(None) => {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, "conversation not found");
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    }
    let result = ConversationRepo::list_messages(&pool, &id).await;
    pool.close().await;
    match result {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn append_company_conversation_message(
    State(s): State<AppState>,
    Path((opc_id, id)): Path<(String, String)>,
    Json(req): Json<AppendMessageRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pool = match company_pool(&s, &opc_id).await {
        Ok(pool) => pool,
        Err(err) => return err,
    };
    match ConversationRepo::get_thread(&pool, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            pool.close().await;
            return err!(StatusCode::NOT_FOUND, "conversation not found");
        }
        Err(e) => {
            pool.close().await;
            return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
    }
    let role = req.role.trim();
    if !matches!(role, "user" | "assistant" | "system") {
        pool.close().await;
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "role must be user, assistant, or system"
        );
    }
    let content = req.content.trim().to_string();
    if content.is_empty() {
        pool.close().await;
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "content required");
    }
    let message = ConversationMessage {
        message_id: req
            .message_id
            .unwrap_or_else(|| format!("msg-{}", uuid::Uuid::new_v4())),
        conversation_id: id,
        role: role.to_string(),
        content,
        linked_work_order_id: req.linked_work_order_id,
        created_at_ms: chrono::Utc::now().timestamp_millis(),
    };
    let result = ConversationRepo::append_message(&pool, &message).await;
    pool.close().await;
    match result {
        Ok(()) => ok!(serde_json::to_value(message).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::extract::{Path, State};
    use axum::http::HeaderMap;
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};

    async fn seeded_company_state() -> (AppState, String) {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root_path =
            std::env::temp_dir().join(format!("coevo-conversations-test-{}", uuid::Uuid::new_v4()));
        let state = AppState::new(pool, root_path);
        let company = state
            .company_workspace
            .create_company(
                "Conversation Co",
                Some("Conversation tests"),
                "default-founder",
            )
            .await
            .unwrap();
        (state, company.opc_id)
    }

    fn legacy_headers(opc_id: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, opc_id.parse().unwrap());
        headers
    }

    #[tokio::test]
    async fn conversation_handlers_persist_thread_messages_and_task_links() {
        let (state, opc_id) = seeded_company_state().await;

        let (create_status, Json(created)) = create_conversation(
            legacy_headers(&opc_id),
            State(state.clone()),
            Json(CreateConversationRequest {
                conversation_id: Some("conv-test".to_string()),
                opc_id: None,
                user_id: Some("default-founder".to_string()),
                title: Some("Founder inbox".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);
        assert_eq!(created["conversation_id"], "conv-test");

        let (append_status, Json(appended)) = append_conversation_message(
            legacy_headers(&opc_id),
            State(state.clone()),
            Path("conv-test".to_string()),
            Json(AppendMessageRequest {
                message_id: Some("msg-test".to_string()),
                role: "assistant".to_string(),
                content: "Created task card.".to_string(),
                linked_work_order_id: Some("wo-test".to_string()),
            }),
        )
        .await;
        assert_eq!(append_status, StatusCode::OK);
        assert_eq!(appended["linked_work_order_id"], "wo-test");

        let (list_status, Json(messages)) = list_conversation_messages(
            legacy_headers(&opc_id),
            State(state),
            Path("conv-test".to_string()),
        )
        .await;
        assert_eq!(list_status, StatusCode::OK);
        assert_eq!(messages.as_array().unwrap().len(), 1);
        assert_eq!(messages[0]["content"], "Created task card.");
    }

    #[tokio::test]
    async fn append_conversation_message_rejects_empty_content() {
        let (state, opc_id) = seeded_company_state().await;
        let (create_status, _) = create_conversation(
            legacy_headers(&opc_id),
            State(state.clone()),
            Json(CreateConversationRequest {
                conversation_id: Some("conv-empty".to_string()),
                opc_id: None,
                user_id: None,
                title: Some("Empty guard".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);

        let (status, Json(body)) = append_conversation_message(
            legacy_headers(&opc_id),
            State(state),
            Path("conv-empty".to_string()),
            Json(AppendMessageRequest {
                message_id: None,
                role: "user".to_string(),
                content: "   ".to_string(),
                linked_work_order_id: None,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "content required");
    }

    #[tokio::test]
    async fn legacy_conversation_routes_require_opc_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());

        let (create_status, Json(create_body)) = create_conversation(
            HeaderMap::new(),
            State(state.clone()),
            Json(CreateConversationRequest {
                conversation_id: Some("conv-missing-header".to_string()),
                opc_id: Some("default-opc".to_string()),
                user_id: Some("default-founder".to_string()),
                title: Some("Missing header".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::BAD_REQUEST);
        assert!(create_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (list_status, Json(list_body)) =
            list_conversations(HeaderMap::new(), State(state.clone())).await;
        assert_eq!(list_status, StatusCode::BAD_REQUEST);
        assert!(list_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (get_status, Json(get_body)) = get_conversation(
            HeaderMap::new(),
            State(state.clone()),
            Path("conv-missing-header".to_string()),
        )
        .await;
        assert_eq!(get_status, StatusCode::BAD_REQUEST);
        assert!(get_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (messages_status, Json(messages_body)) = list_conversation_messages(
            HeaderMap::new(),
            State(state.clone()),
            Path("conv-missing-header".to_string()),
        )
        .await;
        assert_eq!(messages_status, StatusCode::BAD_REQUEST);
        assert!(messages_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));

        let (append_status, Json(append_body)) = append_conversation_message(
            HeaderMap::new(),
            State(state),
            Path("conv-missing-header".to_string()),
            Json(AppendMessageRequest {
                message_id: None,
                role: "user".to_string(),
                content: "hello".to_string(),
                linked_work_order_id: None,
            }),
        )
        .await;
        assert_eq!(append_status, StatusCode::BAD_REQUEST);
        assert!(append_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));
    }

    #[tokio::test]
    async fn legacy_conversation_routes_reject_malformed_opc_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, "../escape".parse().unwrap());

        let (status, Json(body)) = list_conversations(headers, State(state)).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("LEGACY_OPC_ID_REQUIRED"));
    }

    #[tokio::test]
    async fn legacy_conversation_routes_isolate_threads_per_company_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let root_path = std::env::temp_dir().join(format!(
            "coevo-conversations-scope-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState::new(pool, root_path.clone());
        let alpha = state
            .company_workspace
            .create_company("Alpha Conversation Co", Some("alpha"), "default-founder")
            .await
            .unwrap();
        let beta = state
            .company_workspace
            .create_company("Beta Conversation Co", Some("beta"), "default-founder")
            .await
            .unwrap();

        let conversation_id = "conv-shared";
        let (alpha_create_status, _) = create_conversation(
            legacy_headers(&alpha.opc_id),
            State(state.clone()),
            Json(CreateConversationRequest {
                conversation_id: Some(conversation_id.to_string()),
                opc_id: None,
                user_id: Some("default-founder".to_string()),
                title: Some("Alpha thread".to_string()),
            }),
        )
        .await;
        assert_eq!(alpha_create_status, StatusCode::OK);

        let (beta_create_status, _) = create_conversation(
            legacy_headers(&beta.opc_id),
            State(state.clone()),
            Json(CreateConversationRequest {
                conversation_id: Some(conversation_id.to_string()),
                opc_id: None,
                user_id: Some("default-founder".to_string()),
                title: Some("Beta thread".to_string()),
            }),
        )
        .await;
        assert_eq!(beta_create_status, StatusCode::OK);

        let (alpha_append_status, _) = append_conversation_message(
            legacy_headers(&alpha.opc_id),
            State(state.clone()),
            Path(conversation_id.to_string()),
            Json(AppendMessageRequest {
                message_id: Some("msg-alpha".to_string()),
                role: "assistant".to_string(),
                content: "alpha-only message".to_string(),
                linked_work_order_id: None,
            }),
        )
        .await;
        assert_eq!(alpha_append_status, StatusCode::OK);

        let (beta_append_status, _) = append_conversation_message(
            legacy_headers(&beta.opc_id),
            State(state.clone()),
            Path(conversation_id.to_string()),
            Json(AppendMessageRequest {
                message_id: Some("msg-beta".to_string()),
                role: "assistant".to_string(),
                content: "beta-only message".to_string(),
                linked_work_order_id: None,
            }),
        )
        .await;
        assert_eq!(beta_append_status, StatusCode::OK);

        let (alpha_get_status, Json(alpha_thread)) = get_conversation(
            legacy_headers(&alpha.opc_id),
            State(state.clone()),
            Path(conversation_id.to_string()),
        )
        .await;
        assert_eq!(alpha_get_status, StatusCode::OK);
        assert_eq!(alpha_thread["title"], "Alpha thread");

        let (beta_get_status, Json(beta_thread)) = get_conversation(
            legacy_headers(&beta.opc_id),
            State(state.clone()),
            Path(conversation_id.to_string()),
        )
        .await;
        assert_eq!(beta_get_status, StatusCode::OK);
        assert_eq!(beta_thread["title"], "Beta thread");

        let (alpha_messages_status, Json(alpha_messages)) = list_conversation_messages(
            legacy_headers(&alpha.opc_id),
            State(state.clone()),
            Path(conversation_id.to_string()),
        )
        .await;
        assert_eq!(alpha_messages_status, StatusCode::OK);
        let alpha_messages = alpha_messages.as_array().unwrap();
        assert_eq!(alpha_messages.len(), 1);
        assert_eq!(alpha_messages[0]["content"], "alpha-only message");

        let (beta_messages_status, Json(beta_messages)) = list_conversation_messages(
            legacy_headers(&beta.opc_id),
            State(state),
            Path(conversation_id.to_string()),
        )
        .await;
        assert_eq!(beta_messages_status, StatusCode::OK);
        let beta_messages = beta_messages.as_array().unwrap();
        assert_eq!(beta_messages.len(), 1);
        assert_eq!(beta_messages[0]["content"], "beta-only message");

        std::fs::remove_dir_all(root_path).ok();
    }
}
