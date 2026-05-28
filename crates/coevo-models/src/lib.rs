//! coevo-models: Model Gateway abstraction — Mock + OpenAI-compatible providers.
//! Models provide cognition, NOT authorization. All model output is governed
//! by MCL, RiskGate, Cognitive Customs, and SkillVerifier.
pub mod gateway;
pub mod mock;
pub mod openai;
pub mod types;
