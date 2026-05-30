//! Model Gateway trait.

use async_trait::async_trait;
use crate::types::*;

#[async_trait]
pub trait ModelGateway: Send + Sync {
    async fn test_connection(&self, config: &ModelProviderConfig) -> Result<ModelResponse, ModelError>;
    async fn discover_models(&self, config: &ModelProviderConfig) -> Result<ModelDiscoveryResponse, ModelError>;
    async fn chat(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError>;
    async fn structured(&self, request: &ModelRequest, schema_json: &serde_json::Value) -> Result<ModelResponse, ModelError>;
}

/// Select the appropriate gateway for a given provider kind.
pub fn select_gateway(kind: ModelProviderKind) -> Box<dyn ModelGateway> {
    match kind {
        ModelProviderKind::Mock => Box::new(crate::mock::MockModelGateway),
        _ => Box::new(crate::openai::OpenAICompatibleGateway),
    }
}
