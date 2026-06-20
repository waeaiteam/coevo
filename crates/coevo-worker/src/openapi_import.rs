//! OpenAPI importer: turn a third-party OpenAPI 3 document into governed worker tools.
//!
//! Each operation (a `{method, path}` pair) under `paths` becomes one [`Tool`] plus an
//! [`OpenApiToolHandler`] that performs the real HTTP request at execution time. This is
//! the missing "import third-party spec → tools" path (distinct from the server's own
//! utoipa self-exposed OpenAPI doc).
//!
//! Scope: OpenAPI 3.x JSON. Per-operation `operationId` becomes the tool id; path and
//! query parameters are declared so the governance layer and the model both see the call
//! shape. Bodies are passed through as JSON for non-GET methods.

use crate::error::WorkerError;
use crate::tools::github_readonly::ToolHandler;
use crate::types::{Tool, ToolType};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::BTreeMap;

/// A parsed operation ready to be registered as a tool.
pub struct ImportedOperation {
    pub tool: Tool,
    pub handler: OpenApiToolHandler,
}

#[derive(Debug, Deserialize)]
struct RawSpec {
    #[serde(default)]
    servers: Vec<RawServer>,
    #[serde(default)]
    paths: BTreeMap<String, BTreeMap<String, RawOperation>>,
}

#[derive(Debug, Deserialize)]
struct RawServer {
    url: String,
}

#[derive(Debug, Deserialize, Default)]
struct RawOperation {
    #[serde(rename = "operationId")]
    operation_id: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    #[serde(default)]
    parameters: Vec<RawParameter>,
}

#[derive(Debug, Deserialize)]
struct RawParameter {
    name: String,
    #[serde(rename = "in")]
    location: String,
    #[serde(default)]
    required: bool,
}

/// HTTP methods we import. Anything else under a path is ignored.
const HTTP_METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// Parse an OpenAPI 3 JSON document into a set of importable operations.
///
/// `base_url_override` wins over the spec's `servers[0].url`; one of the two must be a
/// non-empty absolute URL or import fails (a tool with no base URL can't make a request).
pub fn import_openapi_tools(
    spec_json: &str,
    base_url_override: Option<&str>,
    risk_ceiling: f64,
) -> Result<Vec<ImportedOperation>, WorkerError> {
    let spec: RawSpec = serde_json::from_str(spec_json)
        .map_err(|e| WorkerError::Internal(format!("invalid OpenAPI document: {e}")))?;

    let base_url = base_url_override
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| spec.servers.first().map(|s| s.url.clone()))
        .map(|s| s.trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            WorkerError::Internal(
                "OpenAPI import requires a base URL (spec servers[0].url or an override)".into(),
            )
        })?;

    let mut imported = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for (path, methods) in &spec.paths {
        for (method, op) in methods {
            let method_lower = method.to_lowercase();
            if !HTTP_METHODS.contains(&method_lower.as_str()) {
                continue;
            }
            let raw_id = op
                .operation_id
                .clone()
                .unwrap_or_else(|| derive_operation_id(&method_lower, path));
            let tool_id = sanitize_tool_id(&raw_id);
            if tool_id.is_empty() || !seen_ids.insert(tool_id.clone()) {
                // Skip empties and duplicate operationIds (first one wins).
                continue;
            }

            let path_params: Vec<String> = op
                .parameters
                .iter()
                .filter(|p| p.location == "path")
                .map(|p| p.name.clone())
                .collect();
            let query_params: Vec<RestQueryParam> = op
                .parameters
                .iter()
                .filter(|p| p.location == "query")
                .map(|p| RestQueryParam {
                    name: p.name.clone(),
                    required: p.required,
                })
                .collect();

            let description = op
                .summary
                .clone()
                .or_else(|| op.description.clone())
                .unwrap_or_else(|| format!("{} {}", method_lower.to_uppercase(), path));

            let handler = OpenApiToolHandler {
                base_url: base_url.clone(),
                method: method_lower.to_uppercase(),
                path_template: path.clone(),
                path_params: path_params.clone(),
                query_params,
            };

            let tool = Tool {
                tool_id: tool_id.clone(),
                name: description.clone(),
                tool_type: ToolType::Mcp,
                risk_ceiling,
                supported_actions: vec![method_lower.to_uppercase()],
                permission_boundary_json: serde_json::json!({
                    "scope": "openapi-import",
                    "writes": method_lower != "get",
                    "base_url": base_url,
                    "path": path,
                    "method": method_lower.to_uppercase(),
                }),
                requires_credential: false,
                credential_ref: None,
                enabled: true,
            };

            imported.push(ImportedOperation { tool, handler });
        }
    }

    if imported.is_empty() {
        return Err(WorkerError::Internal(
            "OpenAPI document declared no importable operations".into(),
        ));
    }
    Ok(imported)
}

fn derive_operation_id(method: &str, path: &str) -> String {
    let cleaned: String = path
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    format!("{method}-{}", cleaned.trim_matches('-'))
}

fn sanitize_tool_id(raw: &str) -> String {
    let lowered = raw.trim().to_lowercase();
    let mut out = String::with_capacity(lowered.len() + 8);
    let mut prev_dash = false;
    for c in lowered.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        trimmed
    } else {
        format!("openapi-{trimmed}")
    }
}

struct RestQueryParam {
    name: String,
    required: bool,
}

/// Handler that performs the real HTTP request for one imported operation.
pub struct OpenApiToolHandler {
    base_url: String,
    method: String,
    path_template: String,
    path_params: Vec<String>,
    query_params: Vec<RestQueryParam>,
}

impl OpenApiToolHandler {
    /// Build the absolute request URL from the input, substituting `{param}` path
    /// placeholders and appending declared query parameters.
    fn build_url(&self, input: &serde_json::Value) -> Result<String, WorkerError> {
        let mut path = self.path_template.clone();
        let path_values = input.get("path_params");
        for param in &self.path_params {
            let value = path_values
                .and_then(|p| p.get(param))
                .map(value_to_string)
                .ok_or_else(|| {
                    WorkerError::ToolUnavailable(format!("missing path parameter '{param}'"))
                })?;
            path = path.replace(&format!("{{{param}}}"), &urlencode(&value));
        }
        let mut url = format!("{}{}", self.base_url, path);
        let query_values = input.get("query");
        let mut pairs = Vec::new();
        for q in &self.query_params {
            if let Some(v) = query_values.and_then(|qq| qq.get(&q.name)) {
                pairs.push(format!(
                    "{}={}",
                    urlencode(&q.name),
                    urlencode(&value_to_string(v))
                ));
            } else if q.required {
                return Err(WorkerError::ToolUnavailable(format!(
                    "missing required query parameter '{}'",
                    q.name
                )));
            }
        }
        if !pairs.is_empty() {
            url.push('?');
            url.push_str(&pairs.join("&"));
        }
        Ok(url)
    }
}

fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Minimal percent-encoding for path/query values (RFC 3986 unreserved kept as-is).
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[async_trait]
impl ToolHandler for OpenApiToolHandler {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> {
        Ok(serde_json::json!({
            "base_url": self.base_url,
            "method": self.method,
            "path": self.path_template,
        }))
    }

    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let url = self.build_url(&input)?;
        Ok(serde_json::json!({
            "dry_run": true,
            "method": self.method,
            "url": url,
        }))
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let url = self.build_url(&input)?;
        let client = reqwest::Client::new();
        let mut request = match self.method.as_str() {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            "DELETE" => client.delete(&url),
            other => {
                return Err(WorkerError::Internal(format!(
                    "unsupported HTTP method: {other}"
                )))
            }
        };
        if self.method != "GET" {
            if let Some(body) = input.get("body") {
                request = request.json(body);
            }
        }
        let response = request
            .send()
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let status_code = response.status().as_u16();
        let max_bytes = input["max_bytes"].as_u64().unwrap_or(200_000) as usize;
        let mut body = response
            .text()
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let truncated = body.len() > max_bytes;
        if truncated {
            body.truncate(max_bytes);
        }
        Ok(serde_json::json!({
            "url": url,
            "method": self.method,
            "status_code": status_code,
            "body": body,
            "truncated": truncated,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    const SAMPLE_SPEC: &str = r#"{
        "openapi": "3.0.0",
        "servers": [{ "url": "https://api.example.com/v1" }],
        "paths": {
            "/users/{id}": {
                "get": {
                    "operationId": "getUser",
                    "summary": "Fetch a user",
                    "parameters": [
                        { "name": "id", "in": "path", "required": true },
                        { "name": "expand", "in": "query", "required": false }
                    ]
                }
            },
            "/users": {
                "post": { "operationId": "createUser", "summary": "Create a user" }
            }
        }
    }"#;

    #[test]
    fn imports_one_tool_per_operation() {
        let ops = import_openapi_tools(SAMPLE_SPEC, None, 0.4).unwrap();
        assert_eq!(ops.len(), 2);
        let ids: Vec<_> = ops.iter().map(|o| o.tool.tool_id.clone()).collect();
        assert!(ids.contains(&"openapi-getuser".to_string()));
        assert!(ids.contains(&"openapi-createuser".to_string()));
    }

    #[test]
    fn get_operation_is_readonly_post_is_write() {
        let ops = import_openapi_tools(SAMPLE_SPEC, None, 0.4).unwrap();
        let get = ops
            .iter()
            .find(|o| o.tool.tool_id == "openapi-getuser")
            .unwrap();
        let post = ops
            .iter()
            .find(|o| o.tool.tool_id == "openapi-createuser")
            .unwrap();
        assert_eq!(get.tool.permission_boundary_json["writes"], false);
        assert_eq!(post.tool.permission_boundary_json["writes"], true);
        assert_eq!(get.tool.supported_actions, vec!["GET"]);
    }

    #[test]
    fn base_url_override_wins_over_spec() {
        let ops = import_openapi_tools(SAMPLE_SPEC, Some("http://localhost:9999"), 0.4).unwrap();
        assert_eq!(
            ops[0].tool.permission_boundary_json["base_url"],
            "http://localhost:9999"
        );
    }

    #[test]
    fn missing_base_url_is_rejected() {
        let spec = r#"{"openapi":"3.0.0","paths":{"/x":{"get":{"operationId":"x"}}}}"#;
        assert!(import_openapi_tools(spec, None, 0.4).is_err());
    }

    #[test]
    fn empty_spec_is_rejected() {
        let spec = r#"{"openapi":"3.0.0","servers":[{"url":"http://x"}],"paths":{}}"#;
        assert!(import_openapi_tools(spec, None, 0.4).is_err());
    }

    #[test]
    fn build_url_substitutes_path_and_query() {
        let ops = import_openapi_tools(SAMPLE_SPEC, None, 0.4).unwrap();
        let get = ops
            .iter()
            .find(|o| o.tool.tool_id == "openapi-getuser")
            .unwrap();
        let url = get
            .handler
            .build_url(&serde_json::json!({
                "path_params": { "id": "42" },
                "query": { "expand": "profile" }
            }))
            .unwrap();
        assert_eq!(url, "https://api.example.com/v1/users/42?expand=profile");
    }

    #[test]
    fn build_url_errors_on_missing_path_param() {
        let ops = import_openapi_tools(SAMPLE_SPEC, None, 0.4).unwrap();
        let get = ops
            .iter()
            .find(|o| o.tool.tool_id == "openapi-getuser")
            .unwrap();
        assert!(get.handler.build_url(&serde_json::json!({})).is_err());
    }

    // Spin up a one-shot localhost HTTP server and confirm the imported handler makes a
    // real request against it (no external network, no new test deps).
    #[tokio::test]
    async fn execute_makes_a_real_http_call() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let body = "{\"ok\":true}";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });

        let base = format!("http://{addr}");
        let ops = import_openapi_tools(SAMPLE_SPEC, Some(&base), 0.4).unwrap();
        let get = ops
            .into_iter()
            .find(|o| o.tool.tool_id == "openapi-getuser")
            .unwrap();
        let result = get
            .handler
            .execute(serde_json::json!({ "path_params": { "id": "7" } }))
            .await
            .unwrap();

        assert_eq!(result["status_code"], 200);
        assert_eq!(result["body"], "{\"ok\":true}");
        let _ = server.join();
    }
}
