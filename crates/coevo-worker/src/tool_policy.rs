use crate::types::*;
pub struct ToolPolicyEngine;
impl ToolPolicyEngine {
    pub fn evaluate(_tool: &Tool, _risk: f64) -> ToolPolicyDecision { ToolPolicyDecision{allowed:true,reason:"stub".into(),hidden_from_model:false,required_approval:false} }
}
