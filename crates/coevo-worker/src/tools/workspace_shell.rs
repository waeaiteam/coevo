use crate::error::WorkerError;
use async_trait::async_trait;
use std::path::Component;
use std::path::PathBuf;
use std::process::Command;

use super::github_readonly::ToolHandler;

pub struct WorkspaceShellTool;

fn trusted_workspace_root() -> Result<PathBuf, WorkerError> {
    let root = std::env::var("COEVO_WORKSPACE_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .ok_or(WorkerError::ToolDeniedByPolicy)?;
    std::fs::canonicalize(root).map_err(|_| WorkerError::ToolDeniedByPolicy)
}

fn is_clean_relative_path(path: &std::path::Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_clean_absolute_path(path: &std::path::Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
}

fn normalize_for_comparison(path: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().replace("\\\\?\\", ""))
    }

    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn path_within_root(candidate: &std::path::Path, root: &std::path::Path) -> bool {
    normalize_for_comparison(candidate).starts_with(normalize_for_comparison(root))
}

fn resolve_workspace_root(input: &serde_json::Value) -> Result<(PathBuf, PathBuf), WorkerError> {
    let claimed_root = input["workspace_root"].as_str().unwrap_or("");
    if claimed_root.is_empty() {
        return Err(WorkerError::ToolDeniedByPolicy);
    }

    let trusted_root = trusted_workspace_root()?;
    let requested_root = PathBuf::from(claimed_root);
    let root = if is_clean_relative_path(&requested_root) {
        trusted_root.join(&requested_root)
    } else if is_clean_absolute_path(&requested_root) {
        requested_root
    } else {
        return Err(WorkerError::PathTraversalDenied);
    };

    let canonical_root =
        std::fs::canonicalize(&root).map_err(|_| WorkerError::ToolDeniedByPolicy)?;
    if !canonical_root.is_dir() {
        return Err(WorkerError::ToolDeniedByPolicy);
    }
    if !path_within_root(&canonical_root, &trusted_root) {
        return Err(WorkerError::PathTraversalDenied);
    }
    Ok((trusted_root, canonical_root))
}

#[async_trait]
impl ToolHandler for WorkspaceShellTool {
    async fn health_check(&self) -> Result<serde_json::Value, WorkerError> {
        Ok(serde_json::json!({"online": true}))
    }

    async fn dry_run(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let (trusted_root, root) = resolve_workspace_root(&input)?;
        let command = input["command"].as_str().unwrap_or("");
        if command.is_empty() {
            return Err(WorkerError::ToolDeniedByPolicy);
        }
        Ok(serde_json::json!({
            "dry_run": true,
            "trusted_workspace_root": trusted_root.to_string_lossy().to_string(),
            "workspace_root": root.to_string_lossy().to_string(),
            "command": command
        }))
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
        let (trusted_root, root) = resolve_workspace_root(&input)?;
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
            "trusted_workspace_root": trusted_root.to_string_lossy().to_string(),
            "workspace_root": root.to_string_lossy().to_string(),
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
            "status": output.status.code()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn setup_workspace() -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("coevo-wss-{}", uuid::Uuid::new_v4()));
        let trusted_root = base.join("workspace");
        let nested_dir = trusted_root.join("nested");
        std::fs::create_dir_all(&nested_dir).unwrap();
        std::env::set_var("COEVO_WORKSPACE_DIR", &trusted_root);
        (base, trusted_root, nested_dir)
    }

    fn workspace_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[tokio::test]
    async fn dry_run_rejects_workspace_root_outside_trusted_root() {
        let _guard = workspace_test_lock();
        let (base, trusted_root, _nested_dir) = setup_workspace();
        let tool = WorkspaceShellTool;
        let err = tool
            .dry_run(serde_json::json!({
                "workspace_root": base,
                "command": "echo should-not-run"
            }))
            .await
            .expect_err("shell dry_run must reject roots outside the trusted workspace");

        assert!(matches!(err, WorkerError::PathTraversalDenied));
        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(trusted_root.parent().unwrap()).ok();
    }

    #[tokio::test]
    async fn execute_allows_nested_directory_within_trusted_root() {
        let _guard = workspace_test_lock();
        let (base, _trusted_root, nested_dir) = setup_workspace();
        let tool = WorkspaceShellTool;

        #[cfg(windows)]
        let command = "Write-Output ok";
        #[cfg(not(windows))]
        let command = "printf ok";

        let response = tool
            .execute(serde_json::json!({
                "workspace_root": nested_dir,
                "command": command
            }))
            .await
            .expect("shell execute should allow nested directories within the trusted workspace");

        assert_eq!(response["status"], 0);
        assert_eq!(response["stdout"].as_str().unwrap().trim(), "ok");
        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(base).ok();
    }
}
