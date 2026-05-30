use crate::error::WorkerError;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

use super::github_readonly::ToolHandler;

pub struct FileReadonlyTool;

fn canonicalize_scope(paths: Vec<String>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter_map(|p| std::fs::canonicalize(p).ok())
        .collect()
}

#[async_trait]
impl ToolHandler for FileReadonlyTool {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> { Ok(serde_json::json!({"online":true})) }
    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let path = input["path"].as_str().unwrap_or("");
        if path.is_empty() { return Err(WorkerError::FileReadDenied); }
        Ok(serde_json::json!({"dry_run":true,"path":path}))
    }
    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let action = input["action"].as_str().unwrap_or("ReadFile");
        let path_str = input["path"].as_str().unwrap_or("");
        let max_bytes = input["max_bytes"].as_u64().unwrap_or(200_000) as usize;
        let allowed = canonicalize_scope(input["allowed_paths"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default());
        let denied = canonicalize_scope(input["denied_paths"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default());
        // Default forbidden patterns
        let forbidden = [".env",".pem",".key","id_rsa","id_ed25519","credentials","secrets","token"];
        let canonical = std::fs::canonicalize(path_str).map_err(|_| WorkerError::FileReadDenied)?;
        let canon_str = canonical.to_string_lossy().to_string();
        let canon_lower = canon_str.to_lowercase();
        for f in &forbidden { if canon_lower.contains(f) || Path::new(path_str).file_name().map(|n| n.to_string_lossy().to_lowercase().contains(f)).unwrap_or(false) { return Err(WorkerError::FileReadDenied); } }
        if denied.iter().any(|d| canonical.starts_with(d)) { return Err(WorkerError::PathTraversalDenied); }
        if !allowed.is_empty() && !allowed.iter().any(|a| canonical.starts_with(a)) { return Err(WorkerError::FileReadDenied); }
        match action {
            "ReadFile" => {
                let content = std::fs::read_to_string(&canonical).map_err(|_| WorkerError::FileReadDenied)?;
                let truncated = content.len() > max_bytes;
                let display = if truncated { content.chars().take(max_bytes).collect::<String>() } else { content };
                Ok(serde_json::json!({"path":canon_str,"action":"ReadFile","content":display,"truncated":truncated,"bytes_read":display.len()}))
            }
            "ListDirectory" => {
                let entries: Vec<String> = std::fs::read_dir(&canonical).map_err(|_| WorkerError::FileReadDenied)?.filter_map(|e| e.ok()).map(|e| e.path().display().to_string()).collect();
                Ok(serde_json::json!({"path":canon_str,"action":"ListDirectory","content":entries,"truncated":false,"bytes_read":entries.len()}))
            }
            _ => Err(WorkerError::ToolDeniedByPolicy),
        }
    }
}
