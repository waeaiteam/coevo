use crate::types::*;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct FileToolPolicy {
    pub allowed_tools: Vec<String>,
    pub risk_ceiling: Option<f64>,
}

fn normalize_action(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn is_read_action(action: &str) -> bool {
    action.starts_with("read") || action.starts_with("list") || action == "httpget"
}

fn is_write_action(action: &str) -> bool {
    action.starts_with("write") || action.starts_with("create") || action.starts_with("update")
}

fn is_execute_action(action: &str) -> bool {
    action.starts_with("execute") || action.starts_with("run") || action == "runshell"
}

fn action_match(allowed: &str, supported: &str) -> bool {
    let allowed = normalize_action(allowed);
    let supported = normalize_action(supported);
    if allowed.is_empty() || supported.is_empty() {
        return false;
    }
    if allowed == supported {
        return true;
    }
    match allowed.as_str() {
        "read" | "analyze" => is_read_action(&supported),
        "list" => supported.starts_with("list"),
        "write" | "mutate" => is_write_action(&supported),
        "execute" | "run" | "runshell" | "shell" => is_execute_action(&supported),
        "httpget" => supported == "httpget",
        _ => false,
    }
}

fn actions_cover_all_supported(allowed_actions: &[String], supported_actions: &[String]) -> bool {
    supported_actions.iter().all(|supported| {
        allowed_actions
            .iter()
            .any(|allowed| action_match(allowed, supported))
    })
}
pub struct ToolPolicyEngine;
impl ToolPolicyEngine {
    fn track_risk(track: &str) -> f64 {
        match track {
            "red" => 0.9,
            "yellow" => 0.6,
            _ => 0.3,
        }
    }

    pub fn evaluate(
        tool: &Tool,
        track: &str,
        allowed_actions: &[String],
        restricted_actions: &[String],
    ) -> ToolPolicyDecision {
        if !tool.enabled {
            return ToolPolicyDecision {
                allowed: false,
                reason: "Tool disabled".into(),
                hidden_from_model: true,
                required_approval: false,
            };
        }
        if track == "red"
            && matches!(
                tool.tool_type,
                ToolType::ExternalExecutor | ToolType::LocalProcessSandbox
            )
        {
            return ToolPolicyDecision {
                allowed: false,
                reason: "Red Track blocks execution tools".into(),
                hidden_from_model: true,
                required_approval: false,
            };
        }
        // Write/execute-class local tools (workspace-write-file, workspace-shell) require
        // the workspace_write sandbox tier, which only the Yellow track grants. Green maps
        // to a read-only sandbox tier (SandboxProfile::from_track), so a Green task must not
        // be able to reach these tools — otherwise it could mutate the workspace while the
        // OS write-deny guard is only armed for read-only runs.
        if track != "yellow" && matches!(tool.tool_type, ToolType::LocalProcessSandbox) {
            return ToolPolicyDecision {
                allowed: false,
                reason: "Workspace write/execute tools require the Yellow track".into(),
                hidden_from_model: true,
                required_approval: false,
            };
        }
        let risk = Self::track_risk(track);
        if tool.risk_ceiling < risk {
            return ToolPolicyDecision {
                allowed: false,
                reason: format!(
                    "Tool risk ceiling {} < track risk {}",
                    tool.risk_ceiling, risk
                ),
                hidden_from_model: true,
                required_approval: false,
            };
        }
        if restricted_actions.iter().any(|a| {
            tool.tool_id.to_lowercase().contains(&a.to_lowercase())
                || tool
                    .supported_actions
                    .iter()
                    .any(|sa| sa.to_lowercase().contains(&a.to_lowercase()))
        }) {
            return ToolPolicyDecision {
                allowed: false,
                reason: "Tool in restricted actions".into(),
                hidden_from_model: true,
                required_approval: false,
            };
        }
        if !allowed_actions.is_empty()
            && !actions_cover_all_supported(allowed_actions, &tool.supported_actions)
        {
            return ToolPolicyDecision {
                allowed: false,
                reason: "No overlap with allowed actions".into(),
                hidden_from_model: true,
                required_approval: false,
            };
        }
        if tool.requires_credential && tool.credential_ref.is_none() {
            return ToolPolicyDecision {
                allowed: false,
                reason: "Credential missing".into(),
                hidden_from_model: true,
                required_approval: false,
            };
        }
        if track == "yellow"
            && matches!(
                tool.tool_type,
                ToolType::ExternalExecutor | ToolType::BrowserMock
            )
        {
            return ToolPolicyDecision {
                allowed: true,
                reason: "Yellow Track: external tool requires approval".into(),
                hidden_from_model: false,
                required_approval: true,
            };
        }
        ToolPolicyDecision {
            allowed: true,
            reason: "OK".into(),
            hidden_from_model: false,
            required_approval: false,
        }
    }

    pub fn filter<'a>(
        tools: &'a [Tool],
        track: &str,
        allowed_actions: &[String],
        restricted_actions: &[String],
    ) -> Vec<&'a Tool> {
        tools
            .iter()
            .filter(|t| Self::evaluate(t, track, allowed_actions, restricted_actions).allowed)
            .collect()
    }

    pub fn filter_with_file_policy<'a>(
        tools: &'a [Tool],
        track: &str,
        allowed_actions: &[String],
        restricted_actions: &[String],
        file_policy: &FileToolPolicy,
    ) -> Vec<&'a Tool> {
        Self::filter(tools, track, allowed_actions, restricted_actions)
            .into_iter()
            .filter(|tool| {
                if let Some(limit) = file_policy.risk_ceiling {
                    if tool.risk_ceiling > limit {
                        return false;
                    }
                }
                if file_policy.allowed_tools.is_empty() {
                    return true;
                }
                file_policy.allowed_tools.iter().any(|allowed| {
                    let allowed = allowed.trim();
                    if allowed.is_empty() {
                        return false;
                    }
                    allowed.eq_ignore_ascii_case(&tool.tool_id)
                        || actions_cover_all_supported(
                            &[allowed.to_string()],
                            &tool.supported_actions,
                        )
                        || matches_tool_scope(allowed, tool)
                })
            })
            .collect()
    }
}

fn matches_tool_scope(scope: &str, tool: &Tool) -> bool {
    match scope {
        "urn:coevo:tool:read" => tool
            .supported_actions
            .iter()
            .all(|action| action_match("read", action) || action_match("list", action)),
        "urn:coevo:tool:write" => tool
            .supported_actions
            .iter()
            .any(|action| action_match("write", action)),
        "urn:coevo:tool:execute" => tool
            .supported_actions
            .iter()
            .any(|action| action_match("execute", action) || action_match("runshell", action)),
        _ => false,
    }
}

pub fn parse_file_tool_policy(value: &Value) -> FileToolPolicy {
    let allowed_tools = value
        .get("allowed_tools")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let risk_ceiling = value.get("risk_ceiling").and_then(|v| v.as_f64());
    FileToolPolicy {
        allowed_tools,
        risk_ceiling,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_tool(
        id: &str,
        risk: f64,
        enabled: bool,
        cred: bool,
        ttype: ToolType,
        actions: Vec<&str>,
    ) -> Tool {
        Tool {
            tool_id: id.into(),
            name: id.into(),
            tool_type: ttype,
            risk_ceiling: risk,
            supported_actions: actions.iter().map(|s| s.to_string()).collect(),
            permission_boundary_json: serde_json::json!({}),
            requires_credential: cred,
            credential_ref: if cred { None } else { Some("cred".into()) },
            enabled,
        }
    }
    #[test]
    fn disabled_hidden() {
        let t = make_tool(
            "t1",
            0.5,
            false,
            false,
            ToolType::GitHubReadonly,
            vec!["ReadReadme"],
        );
        assert!(ToolPolicyEngine::evaluate(&t, "green", &[], &[]).hidden_from_model);
    }
    #[test]
    fn restricted_hidden() {
        let t = make_tool(
            "t1",
            0.5,
            true,
            false,
            ToolType::GitHubReadonly,
            vec!["ReadReadme"],
        );
        assert!(!ToolPolicyEngine::evaluate(&t, "green", &[], &["ReadReadme".into()]).allowed);
    }
    #[test]
    fn red_hides_exec() {
        let t = make_tool(
            "t1",
            0.5,
            true,
            false,
            ToolType::ExternalExecutor,
            vec!["execute"],
        );
        assert!(!ToolPolicyEngine::evaluate(&t, "red", &[], &[]).allowed);
    }
    #[test]
    fn green_blocks_workspace_write_tools() {
        // A Green-track run maps to a read-only sandbox tier, so local write/execute
        // tools must be hidden — otherwise the workspace could be mutated with no guard.
        let t = make_tool(
            "workspace-write-file",
            0.3,
            true,
            false,
            ToolType::LocalProcessSandbox,
            vec!["WriteFile"],
        );
        let decision = ToolPolicyEngine::evaluate(&t, "green", &[], &[]);
        assert!(!decision.allowed);
        assert!(decision.hidden_from_model);
    }
    #[test]
    fn yellow_allows_workspace_write_tools() {
        let t = make_tool(
            "workspace-write-file",
            0.6,
            true,
            false,
            ToolType::LocalProcessSandbox,
            vec!["WriteFile"],
        );
        assert!(ToolPolicyEngine::evaluate(&t, "yellow", &[], &[]).allowed);
    }
    #[test]
    fn credential_missing() {
        let t = make_tool(
            "t1",
            0.5,
            true,
            true,
            ToolType::GitHubReadonly,
            vec!["ReadReadme"],
        );
        assert!(!ToolPolicyEngine::evaluate(&t, "green", &[], &[]).allowed);
    }

    #[test]
    fn file_policy_can_whitelist_specific_tool_id() {
        let read = make_tool(
            "file-readonly",
            0.3,
            true,
            false,
            ToolType::FileReadonly,
            vec!["ReadFile", "ListDirectory"],
        );
        let github = make_tool(
            "github-readonly",
            0.4,
            true,
            false,
            ToolType::GitHubReadonly,
            vec!["ReadReadme"],
        );
        let tools = vec![read.clone(), github.clone()];
        let filtered = ToolPolicyEngine::filter_with_file_policy(
            &tools,
            "green",
            &["read".into()],
            &[],
            &FileToolPolicy {
                allowed_tools: vec!["file-readonly".into()],
                risk_ceiling: Some(0.3),
            },
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].tool_id, "file-readonly");
    }

    #[test]
    fn parse_file_tool_policy_extracts_allowed_tools_and_risk_ceiling() {
        let parsed = parse_file_tool_policy(&serde_json::json!({
            "allowed_tools": ["file-readonly", "urn:coevo:tool:read"],
            "risk_ceiling": 0.3
        }));
        assert_eq!(
            parsed.allowed_tools,
            vec!["file-readonly", "urn:coevo:tool:read"]
        );
        assert_eq!(parsed.risk_ceiling, Some(0.3));
    }
    #[test]
    fn read_action_does_not_match_embedded_substrings() {
        assert!(!action_match("read", "ThreadDelete"));
    }

    #[test]
    fn read_allowed_action_does_not_admit_execute_capable_tool() {
        let t = make_tool(
            "mcp-mixed-tool",
            0.3,
            true,
            false,
            ToolType::Mcp,
            vec!["ReadFile", "ExecuteProcess"],
        );
        let decision = ToolPolicyEngine::evaluate(&t, "green", &["read".into()], &[]);
        assert!(!decision.allowed);
    }
}
