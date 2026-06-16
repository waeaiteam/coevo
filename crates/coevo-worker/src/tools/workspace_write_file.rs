use crate::error::WorkerError;
use async_trait::async_trait;
use std::path::Component;
use std::path::PathBuf;

use super::github_readonly::ToolHandler;

pub struct WorkspaceWriteFileTool;

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

fn ensure_existing_ancestor_within_root(
    root: &std::path::Path,
    path: &std::path::Path,
) -> Result<(), WorkerError> {
    let mut ancestor = path.to_path_buf();
    while !ancestor.exists() {
        if !ancestor.pop() {
            return Err(WorkerError::PathTraversalDenied);
        }
    }

    let canonical_ancestor =
        std::fs::canonicalize(&ancestor).map_err(|_| WorkerError::PathTraversalDenied)?;
    if path_within_root(&canonical_ancestor, root) {
        Ok(())
    } else {
        Err(WorkerError::PathTraversalDenied)
    }
}

fn resolve_target(input: &serde_json::Value) -> Result<(PathBuf, PathBuf), WorkerError> {
    let path = input["path"].as_str().unwrap_or("");
    if path.is_empty() {
        return Err(WorkerError::ToolDeniedByPolicy);
    }

    let trusted_root = trusted_workspace_root()?;
    let requested_target = PathBuf::from(path);
    let target = if is_clean_relative_path(&requested_target) {
        trusted_root.join(&requested_target)
    } else if is_clean_absolute_path(&requested_target) {
        requested_target
    } else {
        return Err(WorkerError::PathTraversalDenied);
    };

    if !path_within_root(&target, &trusted_root) {
        return Err(WorkerError::PathTraversalDenied);
    }
    let parent = target.parent().ok_or(WorkerError::PathTraversalDenied)?;
    ensure_existing_ancestor_within_root(&trusted_root, parent)?;
    Ok((trusted_root, target))
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
        let canonical_parent =
            std::fs::canonicalize(parent).map_err(|_| WorkerError::PathTraversalDenied)?;
        if !path_within_root(&canonical_parent, &root) {
            return Err(WorkerError::PathTraversalDenied);
        }
        let target =
            canonical_parent.join(target.file_name().ok_or(WorkerError::PathTraversalDenied)?);
        std::fs::write(&target, content).map_err(|e| WorkerError::Internal(e.to_string()))?;
        Ok(serde_json::json!({
            "workspace_root": root.to_string_lossy().to_string(),
            "path": target.to_string_lossy().to_string(),
            "bytes_written": content.len()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_workspace() -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("coevo-wsf-{}", uuid::Uuid::new_v4()));
        let trusted_root = base.join("workspace");
        std::fs::create_dir_all(&trusted_root).unwrap();
        let malicious_root = base.clone();
        std::env::set_var("COEVO_WORKSPACE_DIR", &trusted_root);
        (base, trusted_root, malicious_root)
    }

    #[tokio::test]
    async fn execute_allows_absolute_path_within_trusted_root() {
        let (base, trusted_root, malicious_root) = setup_workspace();
        let tool = WorkspaceWriteFileTool;
        let target = trusted_root.join("nested").join("ok.txt");

        let response = tool
            .execute(serde_json::json!({
                    "workspace_root": malicious_root,
                    "path": target,
                    "content": "hello"
            }))
            .await
            .expect("absolute path inside the trusted workspace should be allowed");

        assert_eq!(response["bytes_written"], 5);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(base).ok();
    }

    #[tokio::test]
    async fn dry_run_rejects_absolute_path_outside_trusted_root() {
        let (base, trusted_root, malicious_root) = setup_workspace();
        let tool = WorkspaceWriteFileTool;
        let path = malicious_root.join("escape.txt");

        let err = tool
            .dry_run(serde_json::json!({
                    "workspace_root": trusted_root,
                    "path": path,
                    "content": "hello"
            }))
            .await
            .expect_err("absolute path outside the trusted workspace must be denied");

        assert!(matches!(err, WorkerError::PathTraversalDenied));
        std::env::remove_var("COEVO_WORKSPACE_DIR");
        std::fs::remove_dir_all(base).ok();
    }
}
