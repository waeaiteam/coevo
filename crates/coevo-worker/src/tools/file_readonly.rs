use crate::error::WorkerError;
use async_trait::async_trait;
use std::io::Read;
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
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> {
        Ok(serde_json::json!({"online":true}))
    }
    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let path = input["path"].as_str().unwrap_or("");
        if path.is_empty() {
            return Err(WorkerError::FileReadDenied);
        }
        Ok(serde_json::json!({"dry_run":true,"path":path}))
    }
    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let action = input["action"].as_str().unwrap_or("ReadFile");
        let path_str = input["path"].as_str().unwrap_or("");
        let max_bytes = input["max_bytes"].as_u64().unwrap_or(200_000) as usize;
        let allowed = canonicalize_scope(
            input["allowed_paths"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        );
        let denied = canonicalize_scope(
            input["denied_paths"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        );
        // Default forbidden patterns
        let forbidden = [
            ".env",
            ".pem",
            ".key",
            "id_rsa",
            "id_ed25519",
            "credentials",
            "secrets",
            "token",
        ];
        let canonical = std::fs::canonicalize(path_str).map_err(|_| WorkerError::FileReadDenied)?;
        let canon_str = canonical.to_string_lossy().to_string();
        let canon_lower = canon_str.to_lowercase();
        for f in &forbidden {
            if canon_lower.contains(f)
                || Path::new(path_str)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase().contains(f))
                    .unwrap_or(false)
            {
                return Err(WorkerError::FileReadDenied);
            }
        }
        if denied.iter().any(|d| canonical.starts_with(d)) {
            return Err(WorkerError::PathTraversalDenied);
        }
        if !allowed.is_empty() && !allowed.iter().any(|a| canonical.starts_with(a)) {
            return Err(WorkerError::FileReadDenied);
        }
        match action {
            "ReadFile" => {
                let file =
                    std::fs::File::open(&canonical).map_err(|_| WorkerError::FileReadDenied)?;
                let mut buffer = Vec::with_capacity(max_bytes.saturating_add(1).min(65_536));
                file.take(max_bytes.saturating_add(1) as u64)
                    .read_to_end(&mut buffer)
                    .map_err(|_| WorkerError::FileReadDenied)?;
                let truncated = buffer.len() > max_bytes;
                if truncated {
                    buffer.truncate(max_bytes);
                }
                let bytes_read = buffer.len();
                let display = String::from_utf8_lossy(&buffer).to_string();
                Ok(
                    serde_json::json!({"path":canon_str,"action":"ReadFile","content":display,"truncated":truncated,"bytes_read":bytes_read}),
                )
            }
            "ListDirectory" => {
                let entries: Vec<String> = std::fs::read_dir(&canonical)
                    .map_err(|_| WorkerError::FileReadDenied)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path().display().to_string())
                    .collect();
                Ok(
                    serde_json::json!({"path":canon_str,"action":"ListDirectory","content":entries,"truncated":false,"bytes_read":entries.len()}),
                )
            }
            _ => Err(WorkerError::ToolDeniedByPolicy),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_file_only_decodes_within_max_bytes_window() {
        let root =
            std::env::temp_dir().join(format!("coevo-file-readonly-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let file_path = root.join("notes.txt");
        std::fs::write(&file_path, b"hello\xffworld").unwrap();

        let tool = FileReadonlyTool;
        let response = tool
            .execute(serde_json::json!({
                "action": "ReadFile",
                "path": file_path,
                "allowed_paths": [root.to_string_lossy().to_string()],
                "max_bytes": 5,
            }))
            .await;

        let body = response.expect("bounded reads should not decode bytes beyond max_bytes");
        assert_eq!(body["content"], "hello");
        assert_eq!(body["truncated"], true);
        assert_eq!(body["bytes_read"], 5);

        std::fs::remove_dir_all(root).ok();
    }
}
