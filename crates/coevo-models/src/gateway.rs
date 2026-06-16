//! Model Gateway trait.

use crate::types::*;
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;

pub type ModelStreamFuture<'a> = Pin<Box<dyn Future<Output = Result<(), ModelError>> + Send + 'a>>;
pub type ModelStreamHandler<'a> = dyn FnMut(ModelStreamEvent) -> ModelStreamFuture<'a> + Send + 'a;

#[async_trait]
pub trait ModelGateway: Send + Sync {
    async fn test_connection(
        &self,
        config: &ModelProviderConfig,
    ) -> Result<ModelResponse, ModelError>;
    async fn discover_models(
        &self,
        config: &ModelProviderConfig,
    ) -> Result<ModelDiscoveryResponse, ModelError>;
    async fn chat(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError>;
    async fn structured(
        &self,
        request: &ModelRequest,
        schema_json: &serde_json::Value,
    ) -> Result<ModelResponse, ModelError>;
    async fn stream(
        &self,
        request: &ModelRequest,
        schema_json: Option<&serde_json::Value>,
        on_event: &mut ModelStreamHandler<'_>,
    ) -> Result<ModelResponse, ModelError>;
}

/// Select the appropriate gateway for a given provider kind.
pub fn select_gateway(kind: ModelProviderKind) -> Box<dyn ModelGateway> {
    match kind {
        ModelProviderKind::Mock => Box::new(crate::mock::MockModelGateway),
        ModelProviderKind::Anthropic => Box::new(crate::anthropic::AnthropicGateway),
        _ => Box::new(crate::openai::OpenAICompatibleGateway),
    }
}
