use crate::error::WorkerError;
use crate::tools::file_readonly::FileReadonlyTool;
use crate::tools::github_readonly::{GitHubReadonlyTool, ToolHandler};
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
        r.register(
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
                permission_boundary_json: serde_json::json!({}),
                requires_credential: false,
                credential_ref: None,
                enabled: true,
            },
            Box::new(GitHubReadonlyTool),
        );
        r.register(
            Tool {
                tool_id: "file-readonly".into(),
                name: "File Readonly".into(),
                tool_type: ToolType::FileReadonly,
                risk_ceiling: 0.3,
                supported_actions: vec!["ReadFile".into(), "ListDirectory".into()],
                permission_boundary_json: serde_json::json!({}),
                requires_credential: false,
                credential_ref: None,
                enabled: true,
            },
            Box::new(FileReadonlyTool),
        );
        r
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
