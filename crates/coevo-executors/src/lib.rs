//! coevo-executors: real External Executor adapters (Hermes, OpenClaw, MCP,
//! 302AI, Browser, Local Process, Docker).
//!
//! Every executor must be registered, risk-checked, and governed by
//! MCL/RiskGate/ADR-A. The adapters here implement [`traits::ExternalExecutorAdapter`].
//!
//! ## Real adapters
//!
//! * [`local_process::LocalProcessExecutor`] — subprocess via `tokio::process`.
//! * [`http_runtime::HttpRuntimeExecutor`] — HTTP runtimes (Hermes / OpenClaw /
//!   302AI / Browser / Custom) via `reqwest`.
//! * [`docker::DockerExecutor`] — containers via the `docker` CLI.
//! * [`mcp::McpExecutorSource`] — thin adapter; MCP runs via the MCP tool path
//!   (`RealMcpClient` in `coevo-adapters`), not as an executor.
//!
//! Build one from a passport with [`factory::build_executor`].
//!
//! ## Mock
//!
//! A deterministic [`MockExecutor`](adapters::MockExecutor) lives in
//! [`adapters`], gated behind `cfg(test)` / the `mock-adapters` feature, so it
//! is never built into a default release. Production code constructs adapters
//! only through [`factory::build_executor`].

pub mod config;
pub mod docker;
pub mod factory;
pub mod http_runtime;
pub mod local_process;
pub mod mcp;
pub mod traits;

// Test-only / opt-in mock adapter (gated inside the module file too).
#[cfg(any(test, feature = "mock-adapters"))]
pub mod adapters;

#[cfg(test)]
mod test_support;

// ---- Public surface for the server & worker ----
pub use factory::{build_executor, build_executor_ref};
pub use traits::{
    DryRunResult, ExecutorError, ExecutorHealth, ExecutorResult, ExternalExecutorAdapter,
};

// Concrete adapters, for callers that want to construct one directly.
pub use docker::DockerExecutor;
pub use http_runtime::HttpRuntimeExecutor;
pub use local_process::LocalProcessExecutor;
pub use mcp::McpExecutorSource;
