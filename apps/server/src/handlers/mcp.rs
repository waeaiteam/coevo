use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use coevo_adapters::mcp_client::{McpServerConfig, McpServerRow, RealMcpClient};
use coevo_store::repos::mcp_server_repo::{McpServerRecord, McpServerRepo};
use serde::Deserialize;

use crate::state::AppState;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

#[derive(Debug, Deserialize)]
pub struct UpsertMcpServerRequest {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args_json: Option<String>,
    pub env_json: Option<String>,
    pub url: Option<String>,
    pub headers_json: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

fn record_to_config(record: &McpServerRecord) -> Result<McpServerConfig, String> {
    McpServerConfig::from_row(McpServerRow {
        id: record.id.clone(),
        name: record.name.clone(),
        transport: record.transport.clone(),
        command: record.command.clone(),
        args: Some(record.args_json.clone()),
        env: Some(record.env_json.clone()),
        url: record.url.clone(),
        headers: Some(record.headers_json.clone()),
    })
}

fn request_to_record(req: UpsertMcpServerRequest) -> McpServerRecord {
    let now = chrono::Utc::now().to_rfc3339();
    McpServerRecord {
        id: req.id,
        name: req.name,
        transport: req.transport,
        command: req.command,
        args_json: req.args_json.unwrap_or_else(|| "[]".to_string()),
        env_json: req.env_json.unwrap_or_else(|| "{}".to_string()),
        url: req.url,
        headers_json: req.headers_json.unwrap_or_else(|| "{}".to_string()),
        enabled: req.enabled,
        status: "unknown".to_string(),
        last_error: None,
        tools_json: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now,
    }
}

fn transport_is_stdio(transport: &str) -> bool {
    transport.trim().eq_ignore_ascii_case("stdio")
}

fn redact_secret_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for nested in map.values_mut() {
                redact_secret_value(nested);
            }
        }
        serde_json::Value::Array(items) => {
            for nested in items {
                redact_secret_value(nested);
            }
        }
        serde_json::Value::Null => {}
        _ => *value = serde_json::Value::String("[redacted]".to_string()),
    }
}

fn redact_secret_json(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "{}".to_string();
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(mut value) => {
            redact_secret_value(&mut value);
            serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
        }
        Err(_) => "[redacted]".to_string(),
    }
}

pub async fn list_mcp_servers(State(s): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match McpServerRepo::list(&s.pool).await {
        Ok(rows) => {
            let items = rows
                .into_iter()
                .map(|row| {
                    serde_json::json!({
                        "id": row.id,
                        "name": row.name,
                        "transport": row.transport,
                        "command": row.command,
                        "args_json": row.args_json,
                        "env_json": redact_secret_json(&row.env_json),
                        "url": row.url,
                        "headers_json": redact_secret_json(&row.headers_json),
                        "enabled": row.enabled,
                        "status": row.status,
                        "last_error": row.last_error,
                        "tools_json": row.tools_json,
                        "created_at": row.created_at,
                        "updated_at": row.updated_at,
                    })
                })
                .collect::<Vec<_>>();
            ok!(serde_json::Value::Array(items))
        }
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_mcp_server(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match McpServerRepo::get(&s.pool, &id).await {
        Ok(Some(row)) => ok!(serde_json::json!({
            "id": row.id,
            "name": row.name,
            "transport": row.transport,
            "command": row.command,
            "args_json": row.args_json,
            "env_json": redact_secret_json(&row.env_json),
            "url": row.url,
            "headers_json": redact_secret_json(&row.headers_json),
            "enabled": row.enabled,
            "status": row.status,
            "last_error": row.last_error,
            "tools_json": row.tools_json,
            "created_at": row.created_at,
            "updated_at": row.updated_at,
        })),
        Ok(None) => err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_mcp_server(
    State(s): State<AppState>,
    Json(req): Json<UpsertMcpServerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if transport_is_stdio(&req.transport) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "stdio transport is not allowed through the HTTP API by default"
        );
    }
    let record = request_to_record(req);
    match McpServerRepo::insert(&s.pool, &record).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn update_mcp_server(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpsertMcpServerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if transport_is_stdio(&req.transport) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "stdio transport is not allowed through the HTTP API by default"
        );
    }
    let mut record = request_to_record(req);
    record.id = id;
    match McpServerRepo::update(&s.pool, &record).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(sqlx::Error::RowNotFound) => err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn delete_mcp_server(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match McpServerRepo::delete(&s.pool, &id).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn connect_mcp_server(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = match McpServerRepo::get(&s.pool, &id).await {
        Ok(Some(row)) => row,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if transport_is_stdio(&record.transport) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "stdio transport is not allowed through the HTTP API by default"
        );
    }
    let config = match record_to_config(&record) {
        Ok(cfg) => cfg,
        Err(e) => return err!(StatusCode::UNPROCESSABLE_ENTITY, e),
    };
    let real = RealMcpClient::from_manager((*s.mcp_manager).clone()).await;
    match real.add(config.clone()).await {
        Ok(info) => {
            let tools = s
                .mcp_manager
                .as_ref()
                .list_tools(&id)
                .await
                .unwrap_or_default();
            let _ = McpServerRepo::set_status(&s.pool, &id, "connected", None).await;
            let _ = McpServerRepo::set_tools(
                &s.pool,
                &id,
                &serde_json::to_string(&tools).unwrap_or_else(|_| "[]".into()),
            )
            .await;
            ok!(serde_json::json!({"ok": true, "server": info}))
        }
        Err(e) => {
            let _ = McpServerRepo::set_status(&s.pool, &id, "error", Some(&e.to_string())).await;
            err!(StatusCode::BAD_GATEWAY, e.to_string())
        }
    }
}

pub async fn disconnect_mcp_server(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = match McpServerRepo::get(&s.pool, &id).await {
        Ok(Some(row)) => row,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    if s.mcp_manager.is_connected(&id).await {
        if let Err(e) = s.mcp_manager.disconnect(&id).await {
            return err!(StatusCode::BAD_GATEWAY, e.to_string());
        }
    }

    let next_status = if record.enabled {
        "unknown"
    } else {
        "disabled"
    };
    match McpServerRepo::set_status(&s.pool, &id, next_status, None).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(sqlx::Error::RowNotFound) => err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn test_mcp_server(
    State(s): State<AppState>,
    Json(req): Json<UpsertMcpServerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if transport_is_stdio(&req.transport) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "stdio transport is not allowed through the HTTP API by default"
        );
    }
    let record = request_to_record(req);
    let config = match record_to_config(&record) {
        Ok(cfg) => cfg,
        Err(e) => return err!(StatusCode::UNPROCESSABLE_ENTITY, e),
    };
    let real = RealMcpClient::from_manager((*s.mcp_manager).clone()).await;
    match real.add(config).await {
        Ok(info) => {
            let _ = s.mcp_manager.disconnect(&info.id).await;
            ok!(serde_json::json!({"ok": true, "server": info}))
        }
        Err(e) => err!(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

pub async fn list_mcp_server_tools(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = match McpServerRepo::get(&s.pool, &id).await {
        Ok(Some(row)) => row,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if transport_is_stdio(&record.transport) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "stdio transport is not allowed through the HTTP API by default"
        );
    }
    let config = match record_to_config(&record) {
        Ok(cfg) => cfg,
        Err(e) => return err!(StatusCode::UNPROCESSABLE_ENTITY, e),
    };
    let real = RealMcpClient::from_manager((*s.mcp_manager).clone()).await;
    match real.add(config).await {
        Ok(_) => match s.mcp_manager.as_ref().list_tools(&id).await {
            Ok(tools) => ok!(serde_json::json!({"server_id": id, "tools": tools})),
            Err(e) => err!(StatusCode::BAD_GATEWAY, e.to_string()),
        },
        Err(e) => err!(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

pub async fn sync_enabled_mcp_servers(state: &AppState) -> Result<(), String> {
    let rows = McpServerRepo::list_enabled(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    for row in rows {
        if transport_is_stdio(&row.transport) {
            tracing::warn!(
                server_id = %row.id,
                "skipping stdio MCP server during startup sync because HTTP-managed stdio transport is disabled"
            );
            continue;
        }
        let config = record_to_config(&row)?;
        state
            .mcp_manager
            .connect(config)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use coevo_store::migrate::run_migrations;
    use coevo_store::pool::create_test_pool;

    #[tokio::test]
    async fn mcp_server_crud_persists_rows() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let create = create_mcp_server(
            State(state.clone()),
            Json(UpsertMcpServerRequest {
                id: "srv-1".into(),
                name: "files".into(),
                transport: "http".into(),
                command: None,
                args_json: None,
                env_json: None,
                url: Some("http://127.0.0.1:7777/mcp".into()),
                headers_json: None,
                enabled: true,
            }),
        )
        .await;
        assert_eq!(create.0, StatusCode::OK);
        let list = list_mcp_servers(State(state.clone())).await;
        assert_eq!(list.0, StatusCode::OK);
        let body: serde_json::Value = serde_json::from_value(list.1 .0).unwrap();
        assert_eq!(body.as_array().unwrap().len(), 1);

        let update = update_mcp_server(
            State(state.clone()),
            Path("srv-1".into()),
            Json(UpsertMcpServerRequest {
                id: "srv-1".into(),
                name: "files-updated".into(),
                transport: "http".into(),
                command: None,
                args_json: None,
                env_json: None,
                url: Some("http://127.0.0.1:7777/mcp".into()),
                headers_json: None,
                enabled: false,
            }),
        )
        .await;
        assert_eq!(update.0, StatusCode::OK);

        let got = get_mcp_server(State(state.clone()), Path("srv-1".into())).await;
        assert_eq!(got.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn disconnect_mcp_server_resets_status_even_when_not_connected() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let _ = create_mcp_server(
            State(state.clone()),
            Json(UpsertMcpServerRequest {
                id: "srv-2".into(),
                name: "filesystem".into(),
                transport: "http".into(),
                command: None,
                args_json: None,
                env_json: Some("{}".into()),
                url: Some("http://127.0.0.1:7777/mcp".into()),
                headers_json: Some("{}".into()),
                enabled: true,
            }),
        )
        .await;
        McpServerRepo::set_status(&pool, "srv-2", "connected", None)
            .await
            .unwrap();

        let response = disconnect_mcp_server(State(state.clone()), Path("srv-2".into())).await;
        assert_eq!(response.0, StatusCode::OK);

        let row = McpServerRepo::get(&pool, "srv-2").await.unwrap().unwrap();
        assert_eq!(row.status, "unknown");
        assert_eq!(row.last_error, None);
    }

    #[tokio::test]
    async fn list_and_get_mcp_server_redact_secret_material() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let create = create_mcp_server(
            State(state.clone()),
            Json(UpsertMcpServerRequest {
                id: "srv-secrets".into(),
                name: "secrets".into(),
                transport: "http".into(),
                command: None,
                args_json: None,
                env_json: Some(r#"{"API_KEY":"super-secret","MODE":"prod"}"#.into()),
                url: Some("http://127.0.0.1:7777/mcp".into()),
                headers_json: Some(
                    r#"{"Authorization":"Bearer super-secret","X-Trace":"trace-value"}"#.into(),
                ),
                enabled: true,
            }),
        )
        .await;
        assert_eq!(create.0, StatusCode::OK);

        let list = list_mcp_servers(State(state.clone())).await;
        assert_eq!(list.0, StatusCode::OK);
        let list_body: serde_json::Value = serde_json::from_value(list.1 .0).unwrap();
        let list_item = &list_body.as_array().unwrap()[0];
        assert!(!list_item["env_json"]
            .as_str()
            .unwrap_or_default()
            .contains("super-secret"));
        assert!(!list_item["headers_json"]
            .as_str()
            .unwrap_or_default()
            .contains("super-secret"));

        let get = get_mcp_server(State(state), Path("srv-secrets".into())).await;
        assert_eq!(get.0, StatusCode::OK);
        let get_body: serde_json::Value = serde_json::from_value(get.1 .0).unwrap();
        assert!(!get_body["env_json"]
            .as_str()
            .unwrap_or_default()
            .contains("super-secret"));
        assert!(!get_body["headers_json"]
            .as_str()
            .unwrap_or_default()
            .contains("super-secret"));
    }

    #[tokio::test]
    async fn http_mcp_routes_reject_stdio_transport_by_default() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());

        let create = create_mcp_server(
            State(state),
            Json(UpsertMcpServerRequest {
                id: "srv-stdio-blocked".into(),
                name: "files".into(),
                transport: "stdio".into(),
                command: Some("node".into()),
                args_json: Some(r#"["server.js"]"#.into()),
                env_json: None,
                url: None,
                headers_json: None,
                enabled: true,
            }),
        )
        .await;

        assert_eq!(create.0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn http_mcp_routes_reject_stdio_transport_updates_by_default() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());

        let _ = create_mcp_server(
            State(state.clone()),
            Json(UpsertMcpServerRequest {
                id: "srv-update-target".into(),
                name: "files".into(),
                transport: "http".into(),
                command: None,
                args_json: None,
                env_json: None,
                url: Some("http://127.0.0.1:7777/mcp".into()),
                headers_json: None,
                enabled: true,
            }),
        )
        .await;

        let update = update_mcp_server(
            State(state),
            Path("srv-update-target".into()),
            Json(UpsertMcpServerRequest {
                id: "srv-update-target".into(),
                name: "files".into(),
                transport: "stdio".into(),
                command: Some("node".into()),
                args_json: Some(r#"["server.js"]"#.into()),
                env_json: None,
                url: None,
                headers_json: None,
                enabled: true,
            }),
        )
        .await;

        assert_eq!(update.0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn http_mcp_routes_reject_stdio_transport_connect_and_test_by_default() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        McpServerRepo::insert(
            &pool,
            &McpServerRecord {
                id: "srv-connect-target".into(),
                name: "files".into(),
                transport: "stdio".into(),
                command: Some("node".into()),
                args_json: r#"["server.js"]"#.into(),
                env_json: "{}".into(),
                url: None,
                headers_json: "{}".into(),
                enabled: true,
                status: "unknown".into(),
                last_error: None,
                tools_json: "[]".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let connect =
            connect_mcp_server(State(state.clone()), Path("srv-connect-target".into())).await;
        assert_eq!(connect.0, StatusCode::UNPROCESSABLE_ENTITY);

        let test = test_mcp_server(
            State(state),
            Json(UpsertMcpServerRequest {
                id: "srv-connect-target".into(),
                name: "files".into(),
                transport: "stdio".into(),
                command: Some("node".into()),
                args_json: Some(r#"["server.js"]"#.into()),
                env_json: None,
                url: None,
                headers_json: None,
                enabled: true,
            }),
        )
        .await;
        assert_eq!(test.0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn http_mcp_routes_reject_stdio_transport_list_tools_by_default() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        McpServerRepo::insert(
            &pool,
            &McpServerRecord {
                id: "srv-list-tools-target".into(),
                name: "files".into(),
                transport: "stdio".into(),
                command: Some("node".into()),
                args_json: r#"["server.js"]"#.into(),
                env_json: "{}".into(),
                url: None,
                headers_json: "{}".into(),
                enabled: true,
                status: "unknown".into(),
                last_error: None,
                tools_json: "[]".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let response =
            list_mcp_server_tools(State(state), Path("srv-list-tools-target".into())).await;
        assert_eq!(response.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
