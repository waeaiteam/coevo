use crate::types::*;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct FileToolPolicy {
    pub allowed_tools: Vec<String>,
    pub risk_ceiling: Option<f64>,
}

fn action_match(allowed: &str, supported: &str) -> bool {
    let a = allowed.to_lowercase();
    let s = supported.to_lowercase();
    let a_norm: String = a.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let s_norm: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if s.contains(&a) || a.contains(&s) {
        return true;
    }
    if s_norm.contains(&a_norm) || a_norm.contains(&s_norm) {
        return true;
    }
    // alias: read matches ReadReadme, ReadFile, ReadRepositoryMetadata, ListFiles
    if a == "read" && (s.contains("read") || s.contains("list")) {
        return true;
    }
    if a == "list" && s.contains("list") {
        return true;
    }
    if a == "analyze" && (s.contains("read") || s.contains("list")) {
        return true;
    }
    if a == "http_get" && s_norm.contains("httpget") {
        return true;
    }
    false
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
            && !allowed_actions
                .iter()
                .any(|a| tool.supported_actions.iter().any(|sa| action_match(a, sa)))
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
                        || tool
                            .supported_actions
                            .iter()
                            .any(|action| action_match(allowed, action))
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
}
