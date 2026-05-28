use crate::types::*;

pub struct ToolPolicyEngine;
impl ToolPolicyEngine {
    fn track_risk(track: &str) -> f64 { match track { "red" => 0.9, "yellow" => 0.6, _ => 0.3 } }

    pub fn evaluate(tool: &Tool, track: &str, allowed_actions: &[String], restricted_actions: &[String]) -> ToolPolicyDecision {
        if !tool.enabled {
            return ToolPolicyDecision{allowed:false,reason:"Tool disabled".into(),hidden_from_model:true,required_approval:false};
        }
        if track == "red" && matches!(tool.tool_type, ToolType::ExternalExecutor | ToolType::LocalProcessSandbox) {
            return ToolPolicyDecision{allowed:false,reason:"Red Track blocks execution tools".into(),hidden_from_model:true,required_approval:false};
        }
        let risk = Self::track_risk(track);
        if tool.risk_ceiling < risk {
            return ToolPolicyDecision{allowed:false,reason:format!("Tool risk ceiling {} < track risk {}", tool.risk_ceiling, risk),hidden_from_model:true,required_approval:false};
        }
        if restricted_actions.iter().any(|a| tool.tool_id.contains(a) || tool.supported_actions.iter().any(|sa| sa.contains(a))) {
            return ToolPolicyDecision{allowed:false,reason:"Tool in restricted actions".into(),hidden_from_model:true,required_approval:false};
        }
        if !allowed_actions.is_empty() && !allowed_actions.iter().any(|a| tool.supported_actions.iter().any(|sa| sa.contains(a) || a.contains(sa))) {
            return ToolPolicyDecision{allowed:false,reason:"No overlap with allowed actions".into(),hidden_from_model:true,required_approval:false};
        }
        if tool.requires_credential && tool.credential_ref.is_none() {
            return ToolPolicyDecision{allowed:false,reason:"Credential missing".into(),hidden_from_model:true,required_approval:false};
        }
        if track == "yellow" && matches!(tool.tool_type, ToolType::ExternalExecutor | ToolType::BrowserMock) {
            return ToolPolicyDecision{allowed:true,reason:"Yellow Track: external tool requires approval".into(),hidden_from_model:false,required_approval:true};
        }
        ToolPolicyDecision{allowed:true,reason:"OK".into(),hidden_from_model:false,required_approval:false}
    }

    pub fn filter<'a>(tools: &'a [Tool], track: &str, allowed_actions: &[String], restricted_actions: &[String]) -> Vec<&'a Tool> {
        tools.iter().filter(|t| Self::evaluate(t, track, allowed_actions, restricted_actions).allowed).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_tool(id: &str, risk: f64, enabled: bool, cred: bool, ttype: ToolType, actions: Vec<&str>) -> Tool {
        Tool{tool_id:id.into(),name:id.into(),tool_type:ttype,risk_ceiling:risk,supported_actions:actions.iter().map(|s|s.to_string()).collect(),permission_boundary_json:serde_json::json!({}),requires_credential:cred,credential_ref:if cred{None}else{Some("cred".into())},enabled}
    }
    #[test] fn disabled_hidden() {
        let t = make_tool("t1",0.5,false,false,ToolType::GitHubReadonly,vec!["ReadReadme"]);
        assert!(ToolPolicyEngine::evaluate(&t,"green",&[],&[]).hidden_from_model);
    }
    #[test] fn restricted_hidden() {
        let t = make_tool("t1",0.5,true,false,ToolType::GitHubReadonly,vec!["ReadReadme"]);
        assert!(!ToolPolicyEngine::evaluate(&t,"green",&[],&["ReadReadme".into()]).allowed);
    }
    #[test] fn red_hides_exec() {
        let t = make_tool("t1",0.5,true,false,ToolType::ExternalExecutor,vec!["execute"]);
        assert!(!ToolPolicyEngine::evaluate(&t,"red",&[],&[]).allowed);
    }
    #[test] fn credential_missing() {
        let t = make_tool("t1",0.5,true,true,ToolType::GitHubReadonly,vec!["ReadReadme"]);
        assert!(!ToolPolicyEngine::evaluate(&t,"green",&[],&[]).allowed);
    }
}
