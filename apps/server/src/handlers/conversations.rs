use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_store::repos_opc::conversation_repo::{
    ConversationMessage, ConversationRepo, ConversationThread,
};
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

pub async fn list_conversations(
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match ConversationRepo::list_threads(&s.pool).await {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_conversation(
    State(s): State<AppState>,
    Json(req): Json<CreateConversationRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let now = chrono::Utc::now().timestamp_millis();
    let title = req
        .title
        .unwrap_or_else(|| "New OPC conversation".to_string())
        .trim()
        .to_string();
    if title.is_empty() {
        return err!(StatusCode::UNPROCESSABLE_ENTITY, "title required");
    }
    let thread = ConversationThread {
        conversation_id: req
            .conversation_id
            .unwrap_or_else(|| format!("conv-{}", uuid::Uuid::new_v4())),
        opc_id: req.opc_id.unwrap_or_else(|| "default-opc".to_string()),
        user_id: req.user_id.unwrap_or_else(|| "default-founder".to_string()),
        title,
        status: "open".to_string(),
        created_at_ms: now,
        updated_at_ms: now,
    };
    match ConversationRepo::create_thread(&s.pool, &thread).await {
        Ok(()) => ok!(serde_json::to_value(thread).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_conversation(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match ConversationRepo::get_thread(&s.pool, &id).await {
        Ok(Some(thread)) => ok!(serde_json::to_value(thread).unwrap()),
        Ok(None) => err!(StatusCode::NOT_FOUND, "conversation not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_conversation_messages(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match ConversationRepo::get_thread(&s.pool, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => return err!(StatusCode::NOT_FOUND, "conversation not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
    match ConversationRepo::list_messages(&s.pool, &id).await {
        Ok(items) => ok!(serde_json::to_value(items).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn append_conversation_message(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AppendMessageRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match ConversationRepo::get_thread(&s.pool, &id).await {
        Ok(Some(_)) => {}
        Ok(None) => return err!(StatusCode::NOT_FOUND, "conversation not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
    let role = req.role.trim();
    if !matches!(role, "user" | "assistant" | "system") {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "role must be user, assistant, or system"
        );
    }
    let content = req.content.trim().to_string();
    if content.is_empty() {
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
    match ConversationRepo::append_message(&s.pool, &message).await {
        Ok(()) => ok!(serde_json::to_value(message).unwrap()),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::extract::{Path, State};
    use coevo_store::{migrate::run_migrations, pool::create_test_pool};

    #[tokio::test]
    async fn conversation_handlers_persist_thread_messages_and_task_links() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool);

        let (create_status, Json(created)) = create_conversation(
            State(state.clone()),
            Json(CreateConversationRequest {
                conversation_id: Some("conv-test".to_string()),
                opc_id: Some("default-opc".to_string()),
                user_id: Some("default-founder".to_string()),
                title: Some("Founder inbox".to_string()),
            }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);
        assert_eq!(created["conversation_id"], "conv-test");

        let (append_status, Json(appended)) = append_conversation_message(
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

        let (list_status, Json(messages)) =
            list_conversation_messages(State(state), Path("conv-test".to_string())).await;
        assert_eq!(list_status, StatusCode::OK);
        assert_eq!(messages.as_array().unwrap().len(), 1);
        assert_eq!(messages[0]["content"], "Created task card.");
    }

    #[tokio::test]
    async fn append_conversation_message_rejects_empty_content() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool);
        let (create_status, _) = create_conversation(
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
}
