//! Persisted MCP server registrations (migration 051+054).
//!
//! Backs the real MCP client: connection settings for stdio/http transports,
//! the enable switch, the last observed connection status, and the cached
//! tool list discovered from the server. MCP registrations are scoped by opc_id
//! so companies cannot see or call each other's configured servers.

use sqlx::SqlitePool;

const DEFAULT_OPC_ID: &str = "default-opc";
const MCP_SELECT_COLUMNS: &str = "opc_id, id, name, transport, command, args_json, env_json, url, headers_json, enabled, status, last_error, tools_json, created_at, updated_at";

/// One row of `mcp_servers`.
///
/// `transport` is `'stdio'` or `'http'` (enforced by a CHECK constraint);
/// `status` is one of `'unknown'|'connected'|'error'|'disabled'`.
/// `args_json`/`env_json`/`headers_json`/`tools_json` hold JSON strings.
/// `created_at`/`updated_at` are RFC 3339 timestamps.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct McpServerRecord {
    pub opc_id: String,
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args_json: String,
    pub env_json: String,
    pub url: Option<String>,
    pub headers_json: String,
    pub enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub tools_json: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct McpServerRepo;
impl McpServerRepo {
    pub async fn insert(pool: &SqlitePool, record: &McpServerRecord) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO mcp_servers (\
                opc_id, id, name, transport, command, args_json, env_json, url, headers_json, \
                enabled, status, last_error, tools_json, created_at, updated_at\
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&record.opc_id)
        .bind(&record.id)
        .bind(&record.name)
        .bind(&record.transport)
        .bind(&record.command)
        .bind(&record.args_json)
        .bind(&record.env_json)
        .bind(&record.url)
        .bind(&record.headers_json)
        .bind(record.enabled as i32)
        .bind(&record.status)
        .bind(&record.last_error)
        .bind(&record.tools_json)
        .bind(&record.created_at)
        .bind(&record.updated_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Full-row update by company + id (`created_at` is preserved as stored on insert).
    /// Returns `RowNotFound` when the scoped id does not exist.
    pub async fn update(pool: &SqlitePool, record: &McpServerRecord) -> Result<(), sqlx::Error> {
        Self::update_for_opc(pool, &record.opc_id, record).await
    }

    pub async fn update_for_opc(
        pool: &SqlitePool,
        opc_id: &str,
        record: &McpServerRecord,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            "UPDATE mcp_servers SET \
                name=?, transport=?, command=?, args_json=?, env_json=?, url=?, headers_json=?, \
                enabled=?, status=?, last_error=?, tools_json=?, updated_at=? \
            WHERE opc_id=? AND id=?",
        )
        .bind(&record.name)
        .bind(&record.transport)
        .bind(&record.command)
        .bind(&record.args_json)
        .bind(&record.env_json)
        .bind(&record.url)
        .bind(&record.headers_json)
        .bind(record.enabled as i32)
        .bind(&record.status)
        .bind(&record.last_error)
        .bind(&record.tools_json)
        .bind(&record.updated_at)
        .bind(opc_id)
        .bind(&record.id)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        Self::delete_for_opc(pool, DEFAULT_OPC_ID, id).await
    }

    pub async fn delete_for_opc(
        pool: &SqlitePool,
        opc_id: &str,
        id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM mcp_servers WHERE opc_id=? AND id=?")
            .bind(opc_id)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<McpServerRecord>, sqlx::Error> {
        Self::get_for_opc(pool, DEFAULT_OPC_ID, id).await
    }

    pub async fn get_for_opc(
        pool: &SqlitePool,
        opc_id: &str,
        id: &str,
    ) -> Result<Option<McpServerRecord>, sqlx::Error> {
        let sql = format!("SELECT {MCP_SELECT_COLUMNS} FROM mcp_servers WHERE opc_id=? AND id=?");
        sqlx::query_as::<_, McpServerRecord>(&sql)
            .bind(opc_id)
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    /// List all MCP registrations across companies. Prefer `list_for_opc` in request paths.
    pub async fn list(pool: &SqlitePool) -> Result<Vec<McpServerRecord>, sqlx::Error> {
        let sql = format!("SELECT {MCP_SELECT_COLUMNS} FROM mcp_servers ORDER BY opc_id, name");
        sqlx::query_as::<_, McpServerRecord>(&sql)
            .fetch_all(pool)
            .await
    }

    pub async fn list_for_opc(
        pool: &SqlitePool,
        opc_id: &str,
    ) -> Result<Vec<McpServerRecord>, sqlx::Error> {
        let sql =
            format!("SELECT {MCP_SELECT_COLUMNS} FROM mcp_servers WHERE opc_id=? ORDER BY name");
        sqlx::query_as::<_, McpServerRecord>(&sql)
            .bind(opc_id)
            .fetch_all(pool)
            .await
    }

    /// List all enabled MCP registrations across companies. Prefer `list_enabled_for_opc` in workers.
    pub async fn list_enabled(pool: &SqlitePool) -> Result<Vec<McpServerRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {MCP_SELECT_COLUMNS} FROM mcp_servers WHERE enabled=1 ORDER BY opc_id, name"
        );
        sqlx::query_as::<_, McpServerRecord>(&sql)
            .fetch_all(pool)
            .await
    }

    pub async fn list_enabled_for_opc(
        pool: &SqlitePool,
        opc_id: &str,
    ) -> Result<Vec<McpServerRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {MCP_SELECT_COLUMNS} FROM mcp_servers WHERE opc_id=? AND enabled=1 ORDER BY name"
        );
        sqlx::query_as::<_, McpServerRecord>(&sql)
            .bind(opc_id)
            .fetch_all(pool)
            .await
    }

    /// Record the latest connection status. `last_error` should be `Some`
    /// only for `'error'` status; pass `None` to clear a previous error.
    pub async fn set_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        last_error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        Self::set_status_for_opc(pool, DEFAULT_OPC_ID, id, status, last_error).await
    }

    pub async fn set_status_for_opc(
        pool: &SqlitePool,
        opc_id: &str,
        id: &str,
        status: &str,
        last_error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            "UPDATE mcp_servers SET status=?, last_error=?, updated_at=? WHERE opc_id=? AND id=?",
        )
        .bind(status)
        .bind(last_error)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(opc_id)
        .bind(id)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    /// Cache the tool list discovered from the server (JSON array string).
    pub async fn set_tools(
        pool: &SqlitePool,
        id: &str,
        tools_json: &str,
    ) -> Result<(), sqlx::Error> {
        Self::set_tools_for_opc(pool, DEFAULT_OPC_ID, id, tools_json).await
    }

    pub async fn set_tools_for_opc(
        pool: &SqlitePool,
        opc_id: &str,
        id: &str,
        tools_json: &str,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            "UPDATE mcp_servers SET tools_json=?, updated_at=? WHERE opc_id=? AND id=?",
        )
        .bind(tools_json)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(opc_id)
        .bind(id)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{migrate::run_migrations, pool::create_test_pool};

    fn stdio_record(id: &str, name: &str) -> McpServerRecord {
        let now = chrono::Utc::now().to_rfc3339();
        McpServerRecord {
            opc_id: DEFAULT_OPC_ID.to_string(),
            id: id.to_string(),
            name: name.to_string(),
            transport: "stdio".to_string(),
            command: Some("npx".to_string()),
            args_json: r#"["-y","@modelcontextprotocol/server-filesystem"]"#.to_string(),
            env_json: "{}".to_string(),
            url: None,
            headers_json: "{}".to_string(),
            enabled: true,
            status: "unknown".to_string(),
            last_error: None,
            tools_json: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn mcp_servers_are_scoped_by_opc_id() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        let mut alpha = stdio_record("shared-server", "shared-name");
        alpha.opc_id = "opc-alpha".to_string();
        let mut beta = stdio_record("shared-server", "shared-name");
        beta.opc_id = "opc-beta".to_string();

        McpServerRepo::insert(&pool, &alpha).await.unwrap();
        McpServerRepo::insert(&pool, &beta).await.unwrap();

        let alpha_rows = McpServerRepo::list_for_opc(&pool, "opc-alpha")
            .await
            .unwrap();
        assert_eq!(alpha_rows.len(), 1);
        assert_eq!(alpha_rows[0].opc_id, "opc-alpha");

        let beta_row = McpServerRepo::get_for_opc(&pool, "opc-beta", "shared-server")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(beta_row.opc_id, "opc-beta");

        assert!(
            McpServerRepo::get_for_opc(&pool, "opc-gamma", "shared-server")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn insert_get_list_delete_roundtrip() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        let record = stdio_record("mcp-fs", "filesystem");
        McpServerRepo::insert(&pool, &record).await.unwrap();

        let fetched = McpServerRepo::get(&pool, "mcp-fs").await.unwrap().unwrap();
        assert_eq!(fetched, record);
        assert!(McpServerRepo::get(&pool, "missing")
            .await
            .unwrap()
            .is_none());

        let listed = McpServerRepo::list(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "mcp-fs");

        McpServerRepo::delete(&pool, "mcp-fs").await.unwrap();
        assert!(McpServerRepo::get(&pool, "mcp-fs").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_enabled_filters_disabled_servers() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        McpServerRepo::insert(&pool, &stdio_record("mcp-on", "enabled-server"))
            .await
            .unwrap();
        let mut disabled = stdio_record("mcp-off", "disabled-server");
        disabled.enabled = false;
        McpServerRepo::insert(&pool, &disabled).await.unwrap();

        let enabled = McpServerRepo::list_enabled(&pool).await.unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "mcp-on");
        assert_eq!(McpServerRepo::list(&pool).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn update_replaces_fields_and_errors_on_missing_id() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        McpServerRepo::insert(&pool, &stdio_record("mcp-up", "update-me"))
            .await
            .unwrap();

        let mut updated = stdio_record("mcp-up", "renamed");
        updated.transport = "http".to_string();
        updated.command = None;
        updated.url = Some("http://127.0.0.1:9000/mcp".to_string());
        updated.headers_json = r#"{"authorization":"Bearer x"}"#.to_string();
        updated.enabled = false;
        McpServerRepo::update(&pool, &updated).await.unwrap();

        let fetched = McpServerRepo::get(&pool, "mcp-up").await.unwrap().unwrap();
        assert_eq!(fetched.name, "renamed");
        assert_eq!(fetched.transport, "http");
        assert_eq!(fetched.command, None);
        assert_eq!(fetched.url.as_deref(), Some("http://127.0.0.1:9000/mcp"));
        assert!(!fetched.enabled);

        let missing = stdio_record("mcp-missing", "ghost");
        assert!(matches!(
            McpServerRepo::update(&pool, &missing).await,
            Err(sqlx::Error::RowNotFound)
        ));
    }

    #[tokio::test]
    async fn set_status_and_set_tools_update_row() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        McpServerRepo::insert(&pool, &stdio_record("mcp-st", "status-server"))
            .await
            .unwrap();

        McpServerRepo::set_status(&pool, "mcp-st", "error", Some("connection refused"))
            .await
            .unwrap();
        let fetched = McpServerRepo::get(&pool, "mcp-st").await.unwrap().unwrap();
        assert_eq!(fetched.status, "error");
        assert_eq!(fetched.last_error.as_deref(), Some("connection refused"));

        McpServerRepo::set_status(&pool, "mcp-st", "connected", None)
            .await
            .unwrap();
        let tools = r#"[{"name":"read_file","description":"Read a file"}]"#;
        McpServerRepo::set_tools(&pool, "mcp-st", tools)
            .await
            .unwrap();
        let fetched = McpServerRepo::get(&pool, "mcp-st").await.unwrap().unwrap();
        assert_eq!(fetched.status, "connected");
        assert_eq!(fetched.last_error, None);
        assert_eq!(fetched.tools_json, tools);

        assert!(matches!(
            McpServerRepo::set_status(&pool, "missing", "connected", None).await,
            Err(sqlx::Error::RowNotFound)
        ));
        assert!(matches!(
            McpServerRepo::set_tools(&pool, "missing", "[]").await,
            Err(sqlx::Error::RowNotFound)
        ));
    }

    #[tokio::test]
    async fn schema_rejects_invalid_transport_status_and_duplicate_name_in_same_company() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        let mut bad_transport = stdio_record("mcp-bad-t", "bad-transport");
        bad_transport.transport = "websocket".to_string();
        assert!(McpServerRepo::insert(&pool, &bad_transport).await.is_err());

        let mut bad_status = stdio_record("mcp-bad-s", "bad-status");
        bad_status.status = "weird".to_string();
        assert!(McpServerRepo::insert(&pool, &bad_status).await.is_err());

        McpServerRepo::insert(&pool, &stdio_record("mcp-a", "same-name"))
            .await
            .unwrap();
        let duplicate = stdio_record("mcp-b", "same-name");
        assert!(McpServerRepo::insert(&pool, &duplicate).await.is_err());
    }
}
