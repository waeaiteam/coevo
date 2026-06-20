use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use coevo_adapters::mcp_client::{McpServerConfig, McpServerRow};
use coevo_store::repos::mcp_server_repo::{McpServerRecord, McpServerRepo};
use serde::Deserialize;

use crate::handlers::identifiers::is_plain_identifier;
use crate::state::AppState;

macro_rules! ok {
    ($v:expr) => {
        (StatusCode::OK, Json($v))
    };
}
macro_rules! err { ($code:expr, $msg:expr) => { ($code, Json(serde_json::json!({"error":$msg}))) } }

const LEGACY_OPC_ID_HEADER: &str = "x-coevo-opc-id";

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
            format!("legacy /opc MCP endpoint requires {LEGACY_OPC_ID_HEADER}")
        )
    })
}

fn runtime_mcp_server_id(opc_id: &str, id: &str) -> String {
    format!("{opc_id}:{id}")
}

fn record_to_config(record: &McpServerRecord) -> Result<McpServerConfig, String> {
    McpServerConfig::from_row(McpServerRow {
        id: runtime_mcp_server_id(&record.opc_id, &record.id),
        name: record.name.clone(),
        transport: record.transport.clone(),
        command: record.command.clone(),
        args: Some(record.args_json.clone()),
        env: Some(record.env_json.clone()),
        url: record.url.clone(),
        headers: Some(record.headers_json.clone()),
    })
}

fn request_to_record(req: UpsertMcpServerRequest, opc_id: &str) -> McpServerRecord {
    let now = chrono::Utc::now().to_rfc3339();
    McpServerRecord {
        opc_id: opc_id.to_string(),
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

fn row_to_json(row: McpServerRecord) -> serde_json::Value {
    serde_json::json!({
        "opc_id": row.opc_id,
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
}

pub async fn list_mcp_servers(
    headers: HeaderMap,
    State(s): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    match McpServerRepo::list_for_opc(&s.pool, &opc_id).await {
        Ok(rows) => ok!(serde_json::Value::Array(
            rows.into_iter().map(row_to_json).collect::<Vec<_>>()
        )),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get_mcp_server(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    match McpServerRepo::get_for_opc(&s.pool, &opc_id, &id).await {
        Ok(Some(row)) => ok!(row_to_json(row)),
        Ok(None) => err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn create_mcp_server(
    headers: HeaderMap,
    State(s): State<AppState>,
    Json(req): Json<UpsertMcpServerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    if transport_is_stdio(&req.transport) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "stdio transport is not allowed through the HTTP API by default"
        );
    }
    let record = request_to_record(req, &opc_id);
    match McpServerRepo::insert(&s.pool, &record).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn update_mcp_server(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpsertMcpServerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    if transport_is_stdio(&req.transport) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "stdio transport is not allowed through the HTTP API by default"
        );
    }
    let mut record = request_to_record(req, &opc_id);
    record.id = id;
    match McpServerRepo::update_for_opc(&s.pool, &opc_id, &record).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(sqlx::Error::RowNotFound) => err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn delete_mcp_server(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    let runtime_id = runtime_mcp_server_id(&opc_id, &id);
    if s.mcp_manager.is_connected(&runtime_id).await {
        let _ = s.mcp_manager.disconnect(&runtime_id).await;
    }
    match McpServerRepo::delete_for_opc(&s.pool, &opc_id, &id).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn connect_mcp_server(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    let record = match McpServerRepo::get_for_opc(&s.pool, &opc_id, &id).await {
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
    let runtime_id = config.id.clone();
    match s.mcp_manager.connect(config).await {
        Ok(info) => {
            let tools = s
                .mcp_manager
                .as_ref()
                .list_tools(&runtime_id)
                .await
                .unwrap_or_default();
            let _ =
                McpServerRepo::set_status_for_opc(&s.pool, &opc_id, &id, "connected", None).await;
            let _ = McpServerRepo::set_tools_for_opc(
                &s.pool,
                &opc_id,
                &id,
                &serde_json::to_string(&tools).unwrap_or_else(|_| "[]".into()),
            )
            .await;
            ok!(serde_json::json!({"ok": true, "server": info}))
        }
        Err(e) => {
            let _ = McpServerRepo::set_status_for_opc(
                &s.pool,
                &opc_id,
                &id,
                "error",
                Some(&e.to_string()),
            )
            .await;
            err!(StatusCode::BAD_GATEWAY, e.to_string())
        }
    }
}

pub async fn disconnect_mcp_server(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    let record = match McpServerRepo::get_for_opc(&s.pool, &opc_id, &id).await {
        Ok(Some(row)) => row,
        Ok(None) => return err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => return err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let runtime_id = runtime_mcp_server_id(&opc_id, &id);
    if s.mcp_manager.is_connected(&runtime_id).await {
        if let Err(e) = s.mcp_manager.disconnect(&runtime_id).await {
            return err!(StatusCode::BAD_GATEWAY, e.to_string());
        }
    }

    let next_status = if record.enabled {
        "unknown"
    } else {
        "disabled"
    };
    match McpServerRepo::set_status_for_opc(&s.pool, &opc_id, &id, next_status, None).await {
        Ok(()) => ok!(serde_json::json!({"ok": true})),
        Err(sqlx::Error::RowNotFound) => err!(StatusCode::NOT_FOUND, "MCP server not found"),
        Err(e) => err!(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn test_mcp_server(
    headers: HeaderMap,
    State(s): State<AppState>,
    Json(req): Json<UpsertMcpServerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    if transport_is_stdio(&req.transport) {
        return err!(
            StatusCode::UNPROCESSABLE_ENTITY,
            "stdio transport is not allowed through the HTTP API by default"
        );
    }
    let record = request_to_record(req, &opc_id);
    let config = match record_to_config(&record) {
        Ok(cfg) => cfg,
        Err(e) => return err!(StatusCode::UNPROCESSABLE_ENTITY, e),
    };
    let runtime_id = config.id.clone();
    match s.mcp_manager.connect(config).await {
        Ok(info) => {
            let _ = s.mcp_manager.disconnect(&runtime_id).await;
            ok!(serde_json::json!({"ok": true, "server": info}))
        }
        Err(e) => err!(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

pub async fn list_mcp_server_tools(
    headers: HeaderMap,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let opc_id = match require_legacy_opc_id(&headers) {
        Ok(opc_id) => opc_id,
        Err(response) => return response,
    };
    let record = match McpServerRepo::get_for_opc(&s.pool, &opc_id, &id).await {
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
    let runtime_id = config.id.clone();
    match s.mcp_manager.connect(config).await {
        Ok(_) => match s.mcp_manager.as_ref().list_tools(&runtime_id).await {
            Ok(tools) => {
                ok!(serde_json::json!({"server_id": id, "opc_id": opc_id, "tools": tools}))
            }
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
                opc_id = %row.opc_id,
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

    fn headers(opc_id: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_OPC_ID_HEADER, opc_id.parse().unwrap());
        headers
    }

    fn http_req(id: &str, name: &str) -> UpsertMcpServerRequest {
        UpsertMcpServerRequest {
            id: id.into(),
            name: name.into(),
            transport: "http".into(),
            command: None,
            args_json: None,
            env_json: None,
            url: Some("http://127.0.0.1:7777/mcp".into()),
            headers_json: None,
            enabled: true,
        }
    }

    fn stdio_record(opc_id: &str, id: &str, name: &str) -> McpServerRecord {
        McpServerRecord {
            opc_id: opc_id.into(),
            id: id.into(),
            name: name.into(),
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
        }
    }

    #[tokio::test]
    async fn mcp_http_routes_require_opc_id_header() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());

        let response = list_mcp_servers(HeaderMap::new(), State(state.clone())).await;
        assert_eq!(response.0, StatusCode::BAD_REQUEST);

        let create = create_mcp_server(
            HeaderMap::new(),
            State(state),
            Json(http_req("srv", "files")),
        )
        .await;
        assert_eq!(create.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn mcp_server_crud_persists_rows_inside_opc_scope() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let create = create_mcp_server(
            headers("opc-alpha"),
            State(state.clone()),
            Json(http_req("srv-1", "files")),
        )
        .await;
        assert_eq!(create.0, StatusCode::OK);
        let list = list_mcp_servers(headers("opc-alpha"), State(state.clone())).await;
        assert_eq!(list.0, StatusCode::OK);
        let body: serde_json::Value = serde_json::from_value(list.1 .0).unwrap();
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body.as_array().unwrap()[0]["opc_id"], "opc-alpha");

        let update = update_mcp_server(
            headers("opc-alpha"),
            State(state.clone()),
            Path("srv-1".into()),
            Json(UpsertMcpServerRequest {
                enabled: false,
                ..http_req("srv-1", "files-updated")
            }),
        )
        .await;
        assert_eq!(update.0, StatusCode::OK);

        let got = get_mcp_server(
            headers("opc-alpha"),
            State(state.clone()),
            Path("srv-1".into()),
        )
        .await;
        assert_eq!(got.0, StatusCode::OK);

        let missing_in_other_company =
            get_mcp_server(headers("opc-beta"), State(state), Path("srv-1".into())).await;
        assert_eq!(missing_in_other_company.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn mcp_routes_allow_same_id_and_name_in_different_companies() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool, std::env::temp_dir());

        let alpha = create_mcp_server(
            headers("opc-alpha"),
            State(state.clone()),
            Json(http_req("shared", "same-name")),
        )
        .await;
        assert_eq!(alpha.0, StatusCode::OK);
        let beta = create_mcp_server(
            headers("opc-beta"),
            State(state.clone()),
            Json(http_req("shared", "same-name")),
        )
        .await;
        assert_eq!(beta.0, StatusCode::OK);

        let alpha_list = list_mcp_servers(headers("opc-alpha"), State(state.clone())).await;
        let beta_list = list_mcp_servers(headers("opc-beta"), State(state)).await;
        let alpha_body: serde_json::Value = serde_json::from_value(alpha_list.1 .0).unwrap();
        let beta_body: serde_json::Value = serde_json::from_value(beta_list.1 .0).unwrap();
        assert_eq!(alpha_body.as_array().unwrap().len(), 1);
        assert_eq!(beta_body.as_array().unwrap().len(), 1);
        assert_eq!(alpha_body.as_array().unwrap()[0]["opc_id"], "opc-alpha");
        assert_eq!(beta_body.as_array().unwrap()[0]["opc_id"], "opc-beta");
    }

    #[tokio::test]
    async fn disconnect_mcp_server_resets_status_even_when_not_connected() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let _ = create_mcp_server(
            headers("opc-alpha"),
            State(state.clone()),
            Json(UpsertMcpServerRequest {
                env_json: Some("{}".into()),
                headers_json: Some("{}".into()),
                ..http_req("srv-2", "filesystem")
            }),
        )
        .await;
        McpServerRepo::set_status_for_opc(&pool, "opc-alpha", "srv-2", "connected", None)
            .await
            .unwrap();

        let response = disconnect_mcp_server(
            headers("opc-alpha"),
            State(state.clone()),
            Path("srv-2".into()),
        )
        .await;
        assert_eq!(response.0, StatusCode::OK);

        let row = McpServerRepo::get_for_opc(&pool, "opc-alpha", "srv-2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "unknown");
        assert_eq!(row.last_error, None);
    }

    #[tokio::test]
    async fn list_and_get_mcp_server_redact_secret_material() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::new(pool.clone(), std::env::temp_dir());

        let create = create_mcp_server(
            headers("opc-alpha"),
            State(state.clone()),
            Json(UpsertMcpServerRequest {
                env_json: Some(r#"{"API_KEY":"super-secret","MODE":"prod"}"#.into()),
                headers_json: Some(
                    r#"{"Authorization":"Bearer super-secret","X-Trace":"trace-value"}"#.into(),
                ),
                ..http_req("srv-secrets", "secrets")
            }),
        )
        .await;
        assert_eq!(create.0, StatusCode::OK);

        let list = list_mcp_servers(headers("opc-alpha"), State(state.clone())).await;
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

        let get = get_mcp_server(
            headers("opc-alpha"),
            State(state),
            Path("srv-secrets".into()),
        )
        .await;
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
            headers("opc-alpha"),
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
            headers("opc-alpha"),
            State(state.clone()),
            Json(http_req("srv-update-target", "files")),
        )
        .await;

        let update = update_mcp_server(
            headers("opc-alpha"),
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
            &stdio_record("opc-alpha", "srv-connect-target", "files"),
        )
        .await
        .unwrap();

        let connect = connect_mcp_server(
            headers("opc-alpha"),
            State(state.clone()),
            Path("srv-connect-target".into()),
        )
        .await;
        assert_eq!(connect.0, StatusCode::UNPROCESSABLE_ENTITY);

        let test = test_mcp_server(
            headers("opc-alpha"),
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
            &stdio_record("opc-alpha", "srv-list-tools-target", "files"),
        )
        .await
        .unwrap();

        let response = list_mcp_server_tools(
            headers("opc-alpha"),
            State(state),
            Path("srv-list-tools-target".into()),
        )
        .await;
        assert_eq!(response.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
