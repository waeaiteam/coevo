use crate::error::WorkerError;
use async_trait::async_trait;
use std::path::PathBuf;

use super::github_readonly::ToolHandler;

pub struct WorkspaceWriteFileTool;

fn resolve_target(input: &serde_json::Value) -> Result<(PathBuf, PathBuf), WorkerError> {
    let workspace_root = input["workspace_root"].as_str().unwrap_or("");
    let path = input["path"].as_str().unwrap_or("");
    if workspace_root.is_empty() || path.is_empty() {
        return Err(WorkerError::ToolDeniedByPolicy);
    }
    let root = PathBuf::from(workspace_root);
    if !root.exists() || !root.is_dir() {
        return Err(WorkerError::ToolDeniedByPolicy);
    }
    let target = PathBuf::from(path);
    let target = if target.is_absolute() { target } else { root.join(target) };
    if !target.starts_with(&root) {
        return Err(WorkerError::PathTraversalDenied);
    }
    Ok((root, target))
}

#[async_trait]
impl ToolHandler for WorkspaceWriteFileTool {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> {
        Ok(serde_json::json!({"online": true}))
    }

    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let (root, target) = resolve_target(&input)?;
        Ok(serde_json::json!({
            "dry_run": true,
            "workspace_root": root.to_string_lossy().to_string(),
            "path": target.to_string_lossy().to_string()
        }))
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let (root, target) = resolve_target(&input)?;
        let content = input["content"].as_str().unwrap_or("");
        let parent = target.parent().ok_or(WorkerError::PathTraversalDenied)?;
        std::fs::create_dir_all(parent).map_err(|e| WorkerError::Internal(e.to_string()))?;
        std::fs::write(&target, content).map_err(|e| WorkerError::Internal(e.to_string()))?;
        Ok(serde_json::json!({
            "workspace_root": root.to_string_lossy().to_string(),
            "path": target.to_string_lossy().to_string(),
            "bytes_written": content.len()
        }))
    }
}
