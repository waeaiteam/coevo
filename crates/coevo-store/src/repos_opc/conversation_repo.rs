use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationThread {
    pub conversation_id: String,
    pub opc_id: String,
    pub user_id: String,
    pub title: String,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationMessage {
    pub message_id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub linked_work_order_id: Option<String>,
    pub created_at_ms: i64,
}

pub struct ConversationRepo;

impl ConversationRepo {
    fn thread_from_row(row: &sqlx::sqlite::SqliteRow) -> ConversationThread {
        ConversationThread {
            conversation_id: row.get("conversation_id"),
            opc_id: row.get("opc_id"),
            user_id: row.get("user_id"),
            title: row.get("title"),
            status: row.get("status"),
            created_at_ms: row.get("created_at_ms"),
            updated_at_ms: row.get("updated_at_ms"),
        }
    }

    fn message_from_row(row: &sqlx::sqlite::SqliteRow) -> ConversationMessage {
        ConversationMessage {
            message_id: row.get("message_id"),
            conversation_id: row.get("conversation_id"),
            role: row.get("role"),
            content: row.get("content"),
            linked_work_order_id: row.get("linked_work_order_id"),
            created_at_ms: row.get("created_at_ms"),
        }
    }

    pub async fn create_thread(
        pool: &SqlitePool,
        thread: &ConversationThread,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO conversation_threads (
                conversation_id, opc_id, user_id, title, status, created_at_ms, updated_at_ms
            ) VALUES (?,?,?,?,?,?,?)",
        )
        .bind(&thread.conversation_id)
        .bind(&thread.opc_id)
        .bind(&thread.user_id)
        .bind(&thread.title)
        .bind(&thread.status)
        .bind(thread.created_at_ms)
        .bind(thread.updated_at_ms)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_threads(pool: &SqlitePool) -> Result<Vec<ConversationThread>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT * FROM conversation_threads WHERE status != 'archived' ORDER BY updated_at_ms DESC LIMIT 100",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.iter().map(Self::thread_from_row).collect())
    }

    pub async fn get_thread(
        pool: &SqlitePool,
        conversation_id: &str,
    ) -> Result<Option<ConversationThread>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM conversation_threads WHERE conversation_id=?")
            .bind(conversation_id)
            .fetch_optional(pool)
            .await?;
        Ok(row.as_ref().map(Self::thread_from_row))
    }

    pub async fn append_message(
        pool: &SqlitePool,
        message: &ConversationMessage,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO conversation_messages (
                message_id, conversation_id, role, content, linked_work_order_id, created_at_ms
            ) VALUES (?,?,?,?,?,?)",
        )
        .bind(&message.message_id)
        .bind(&message.conversation_id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(&message.linked_work_order_id)
        .bind(message.created_at_ms)
        .execute(pool)
        .await?;
        sqlx::query("UPDATE conversation_threads SET updated_at_ms=? WHERE conversation_id=?")
            .bind(message.created_at_ms)
            .bind(&message.conversation_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn list_messages(
        pool: &SqlitePool,
        conversation_id: &str,
    ) -> Result<Vec<ConversationMessage>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT * FROM conversation_messages WHERE conversation_id=? ORDER BY created_at_ms, message_id",
        )
        .bind(conversation_id)
        .fetch_all(pool)
        .await?;
        Ok(rows.iter().map(Self::message_from_row).collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::{migrate::run_migrations, pool::create_test_pool};

    use super::{ConversationMessage, ConversationRepo, ConversationThread};

    #[tokio::test]
    async fn conversation_threads_and_messages_round_trip_in_created_order() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let thread = ConversationThread {
            conversation_id: "conv-customer-feedback".to_string(),
            opc_id: "default-opc".to_string(),
            user_id: "default-founder".to_string(),
            title: "Customer feedback analysis".to_string(),
            status: "open".to_string(),
            created_at_ms: now,
            updated_at_ms: now,
        };

        ConversationRepo::create_thread(&pool, &thread)
            .await
            .unwrap();
        ConversationRepo::append_message(
            &pool,
            &ConversationMessage {
                message_id: "msg-1".to_string(),
                conversation_id: thread.conversation_id.clone(),
                role: "user".to_string(),
                content: "Summarize this week of customer feedback".to_string(),
                linked_work_order_id: None,
                created_at_ms: now,
            },
        )
        .await
        .unwrap();
        ConversationRepo::append_message(
            &pool,
            &ConversationMessage {
                message_id: "msg-2".to_string(),
                conversation_id: thread.conversation_id.clone(),
                role: "assistant".to_string(),
                content: "Created a task card and kept it linked to this conversation.".to_string(),
                linked_work_order_id: Some("wo-customer-feedback".to_string()),
                created_at_ms: now + 1,
            },
        )
        .await
        .unwrap();

        let saved = ConversationRepo::get_thread(&pool, &thread.conversation_id)
            .await
            .unwrap()
            .unwrap();
        let messages = ConversationRepo::list_messages(&pool, &thread.conversation_id)
            .await
            .unwrap();

        assert_eq!(saved.title, "Customer feedback analysis");
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[0].content,
            "Summarize this week of customer feedback"
        );
        assert_eq!(
            messages[1].linked_work_order_id.as_deref(),
            Some("wo-customer-feedback")
        );
    }
}
