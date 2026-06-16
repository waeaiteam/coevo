//! coevo-models: Model Gateway abstraction — Mock + OpenAI-compatible providers.
//! Models provide cognition, NOT authorization. All model output is governed
//! by MCL, RiskGate, Cognitive Customs, and SkillVerifier.
pub mod anthropic;
pub mod gateway;
mod http;
pub mod mock;
pub mod openai;
pub mod pricing;
pub mod router;
pub mod types;
