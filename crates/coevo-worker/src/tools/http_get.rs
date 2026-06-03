use crate::error::WorkerError;
use async_trait::async_trait;

use super::github_readonly::ToolHandler;

pub struct HttpGetTool;

#[async_trait]
impl ToolHandler for HttpGetTool {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> {
        Ok(serde_json::json!({"online": true}))
    }

    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let url = input["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return Err(WorkerError::ToolDeniedByPolicy);
        }
        Ok(serde_json::json!({"dry_run": true, "url": url}))
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let url = input["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return Err(WorkerError::ToolDeniedByPolicy);
        }
        let max_bytes = input["max_bytes"].as_u64().unwrap_or(200_000) as usize;
        let response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .map_err(|e| WorkerError::Internal(e.to_string()))?;
        let status_code = response.status().as_u16();
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
            "status_code": status_code,
            "body": body,
            "truncated": truncated
        }))
    }
}
