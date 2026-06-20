use crate::error::WorkerError;
use crate::tools::file_readonly::FileReadonlyTool;
use crate::tools::github_readonly::{GitHubReadonlyTool, ToolHandler};
use crate::tools::http_get::HttpGetTool;
use crate::tools::workspace_shell::WorkspaceShellTool;
use crate::tools::workspace_write_file::WorkspaceWriteFileTool;
use crate::types::*;
use std::collections::HashMap;

pub struct ToolRegistry {
    handlers: HashMap<String, Box<dyn ToolHandler>>,
    tools: Vec<Tool>,
    next_id: u32,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            tools: vec![],
            next_id: 0,
        }
    }
    pub fn register(&mut self, tool: Tool, handler: Box<dyn ToolHandler>) {
        self.tools.push(tool);
        self.handlers
            .insert(self.tools.last().unwrap().tool_id.clone(), handler);
        self.next_id += 1;
    }

    /// Import every operation from an OpenAPI 3 JSON document and register each as a tool.
    /// Returns the registered tool ids. `base_url_override` wins over the spec's server URL.
    pub fn register_openapi_spec(
        &mut self,
        spec_json: &str,
        base_url_override: Option<&str>,
        risk_ceiling: f64,
    ) -> Result<Vec<String>, WorkerError> {
        let operations = crate::openapi_import::import_openapi_tools(
            spec_json,
            base_url_override,
            risk_ceiling,
        )?;
        let mut ids = Vec::with_capacity(operations.len());
        for op in operations {
            ids.push(op.tool.tool_id.clone());
            self.register(op.tool, Box::new(op.handler));
        }
        Ok(ids)
    }
    pub fn list(&self) -> &[Tool] {
        &self.tools
    }
    pub fn get(&self, id: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.tool_id == id)
    }
    pub async fn execute(
        &self,
        tool_id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, WorkerError> {
        let handler = self
            .handlers
            .get(tool_id)
            .ok_or(WorkerError::ToolUnavailable(tool_id.into()))?;
        handler.execute(input).await
    }
    pub fn default_registry() -> Self {
        let mut r = Self::new();
        for (tool, handler) in default_tool_specs() {
            r.register(tool, handler);
        }
        r
    }
}

fn workspace_shell_enabled() -> bool {
    std::env::var("COEVO_ENABLE_WORKSPACE_SHELL")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn workspace_shell_spec() -> (Tool, Box<dyn ToolHandler>) {
    (
        Tool {
            tool_id: "workspace-shell".into(),
            name: "Workspace Shell".into(),
            tool_type: ToolType::LocalProcessSandbox,
            risk_ceiling: 0.6,
            supported_actions: vec!["RunShell".into()],
            permission_boundary_json: serde_json::json!({
                "scope": "workspace-shell",
                "writes": true,
                "enabled_by": "COEVO_ENABLE_WORKSPACE_SHELL",
            }),
            requires_credential: false,
            credential_ref: None,
            enabled: true,
        },
        Box::new(WorkspaceShellTool),
    )
}

fn default_tool_specs() -> Vec<(Tool, Box<dyn ToolHandler>)> {
    let mut tools: Vec<(Tool, Box<dyn ToolHandler>)> = vec![
        (
            Tool {
                tool_id: "github-readonly".into(),
                name: "GitHub Readonly".into(),
                tool_type: ToolType::GitHubReadonly,
                risk_ceiling: 0.4,
                supported_actions: vec![
                    "ReadRepositoryMetadata".into(),
                    "ReadReadme".into(),
                    "ListRecentCommits".into(),
                ],
                permission_boundary_json: serde_json::json!({
                    "scope": "repository-readonly",
                    "writes": false,
                }),
                requires_credential: false,
                credential_ref: None,
                enabled: true,
            },
            Box::new(GitHubReadonlyTool),
        ),
        (
            Tool {
                tool_id: "file-readonly".into(),
                name: "File Readonly".into(),
                tool_type: ToolType::FileReadonly,
                risk_ceiling: 0.3,
                supported_actions: vec!["ReadFile".into(), "ListDirectory".into()],
                permission_boundary_json: serde_json::json!({
                    "scope": "workspace-files-readonly",
                    "writes": false,
                }),
                requires_credential: false,
                credential_ref: None,
                enabled: true,
            },
            Box::new(FileReadonlyTool),
        ),
        (
            Tool {
                tool_id: "http-get".into(),
                name: "HTTP GET".into(),
                tool_type: ToolType::GitHubReadonly,
                risk_ceiling: 0.3,
                supported_actions: vec!["HttpGet".into()],
                permission_boundary_json: serde_json::json!({
                    "scope": "network-readonly",
                    "writes": false,
                }),
                requires_credential: false,
                credential_ref: None,
                enabled: true,
            },
            Box::new(HttpGetTool),
        ),
        (
            Tool {
                tool_id: "workspace-write-file".into(),
                name: "Workspace Write File".into(),
                tool_type: ToolType::LocalProcessSandbox,
                risk_ceiling: 0.6,
                supported_actions: vec!["WriteFile".into()],
                permission_boundary_json: serde_json::json!({
                    "scope": "workspace-files-write",
                    "writes": true,
                }),
                requires_credential: false,
                credential_ref: None,
                enabled: true,
            },
            Box::new(WorkspaceWriteFileTool),
        ),
    ];

    if workspace_shell_enabled() {
        tools.push(workspace_shell_spec());
    }

    tools
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn registry_env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn default_registry_hides_workspace_shell_by_default() {
        let _guard = registry_env_lock();
        std::env::remove_var("COEVO_ENABLE_WORKSPACE_SHELL");

        let registry = ToolRegistry::default_registry();

        assert!(registry.get("workspace-shell").is_none());
        assert!(registry.get("file-readonly").is_some());
        assert!(registry.get("workspace-write-file").is_some());
    }

    #[test]
    fn default_registry_exposes_workspace_shell_when_explicitly_enabled() {
        let _guard = registry_env_lock();
        std::env::set_var("COEVO_ENABLE_WORKSPACE_SHELL", "1");

        let registry = ToolRegistry::default_registry();
        let shell = registry.get("workspace-shell").unwrap();

        assert_eq!(shell.supported_actions, vec!["RunShell"]);
        assert_eq!(shell.permission_boundary_json["scope"], "workspace-shell");

        std::env::remove_var("COEVO_ENABLE_WORKSPACE_SHELL");
    }

    #[test]
    fn register_openapi_spec_adds_one_tool_per_operation() {
        let spec = r#"{
            "openapi": "3.0.0",
            "servers": [{ "url": "https://api.example.com" }],
            "paths": {
                "/ping": { "get": { "operationId": "ping" } },
                "/items": { "post": { "operationId": "createItem" } }
            }
        }"#;
        let mut registry = ToolRegistry::new();
        let ids = registry.register_openapi_spec(spec, None, 0.4).unwrap();
        assert_eq!(ids.len(), 2);
        assert!(registry.get("openapi-ping").is_some());
        assert!(registry.get("openapi-createitem").is_some());
    }

    #[test]
    fn register_openapi_spec_rejects_invalid_document() {
        let mut registry = ToolRegistry::new();
        assert!(registry
            .register_openapi_spec("not json", None, 0.4)
            .is_err());
    }
}
