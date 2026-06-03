use crate::error::WorkerError;
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Command;

use super::github_readonly::ToolHandler;

pub struct WorkspaceShellTool;

fn workspace_root(input: &serde_json::Value) -> Result<PathBuf, WorkerError> {
    let root = input["workspace_root"].as_str().unwrap_or("");
    if root.is_empty() {
        return Err(WorkerError::ToolDeniedByPolicy);
    }
    let root = PathBuf::from(root);
    if !root.exists() || !root.is_dir() {
        return Err(WorkerError::ToolDeniedByPolicy);
    }
    Ok(root)
}

#[async_trait]
impl ToolHandler for WorkspaceShellTool {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> {
        Ok(serde_json::json!({"online": true}))
    }

    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let root = workspace_root(&input)?;
        let command = input["command"].as_str().unwrap_or("");
        if command.is_empty() {
            return Err(WorkerError::ToolDeniedByPolicy);
        }
        Ok(serde_json::json!({
            "dry_run": true,
            "workspace_root": root.to_string_lossy().to_string(),
            "command": command
        }))
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let root = workspace_root(&input)?;
        let command = input["command"].as_str().unwrap_or("");
        if command.is_empty() {
            return Err(WorkerError::ToolDeniedByPolicy);
        }

        #[cfg(windows)]
        let output = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(command)
            .current_dir(&root)
            .output()
            .map_err(|e| WorkerError::Internal(e.to_string()))?;

        #[cfg(not(windows))]
        let output = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .current_dir(&root)
            .output()
            .map_err(|e| WorkerError::Internal(e.to_string()))?;

        Ok(serde_json::json!({
            "workspace_root": root.to_string_lossy().to_string(),
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
            "status": output.status.code()
        }))
    }
}
