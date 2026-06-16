//! Factory: map an [`ExternalExecutorPassport`] to the right real adapter.
//!
//! This replaces the old mock factory. The server and worker build adapters
//! from a passport (e.g. one loaded by `ExecutorRepo`) and get back a boxed
//! [`ExternalExecutorAdapter`] trait object.
//!
//! ## Source-type → adapter map
//!
//! | `ExecutorSourceType`            | adapter                              |
//! |---------------------------------|--------------------------------------|
//! | `LocalProcess`                  | [`LocalProcessExecutor`]             |
//! | `Docker`                        | [`DockerExecutor`]                   |
//! | `Hermes`, `OpenClaw`, `Local302AI`, `Browser`, `Custom` | [`HttpRuntimeExecutor`] |
//! | `MCP`                           | [`McpExecutorSource`] (tool-path)    |
//!
//! `Browser` is treated as an HTTP runtime: a browser-automation runtime (e.g. a
//! Playwright/Chromedriver service) is reached over HTTP exactly like the other
//! remote runtimes. If a future build ships an in-process browser driver, give it
//! its own arm here.

use crate::docker::DockerExecutor;
use crate::http_runtime::HttpRuntimeExecutor;
use crate::local_process::LocalProcessExecutor;
use crate::mcp::McpExecutorSource;
use crate::traits::ExternalExecutorAdapter;
use coevo_core::opc::{ExecutorSourceType, ExternalExecutorPassport};

/// Build a real adapter for the given passport.
///
/// Total over [`ExecutorSourceType`] — every variant maps to a real adapter, so
/// (unlike the old mock factory) there is no `None` case for the caller to
/// paper over. Construction never performs IO; the first network/process call
/// happens when a trait method is invoked.
pub fn build_executor(passport: ExternalExecutorPassport) -> Box<dyn ExternalExecutorAdapter> {
    match passport.source_type {
        ExecutorSourceType::LocalProcess => Box::new(LocalProcessExecutor::new(passport)),
        ExecutorSourceType::Docker => Box::new(DockerExecutor::new(passport)),
        ExecutorSourceType::MCP => Box::new(McpExecutorSource::new(passport)),
        ExecutorSourceType::Hermes
        | ExecutorSourceType::OpenClaw
        | ExecutorSourceType::Local302AI
        | ExecutorSourceType::Browser
        | ExecutorSourceType::Custom => Box::new(HttpRuntimeExecutor::new(passport)),
    }
}

/// Borrowing convenience: build from a `&passport` by cloning. Handy for callers
/// (like the server's `make_executor`) that hold a reference loaded from the DB.
pub fn build_executor_ref(passport: &ExternalExecutorPassport) -> Box<dyn ExternalExecutorAdapter> {
    build_executor(passport.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_passport;

    fn assert_source(p: ExternalExecutorPassport, expect_id: &str) {
        let adapter = build_executor(p);
        assert_eq!(adapter.passport().executor_id, expect_id);
    }

    #[test]
    fn factory_covers_every_source_type() {
        for st in [
            ExecutorSourceType::Hermes,
            ExecutorSourceType::OpenClaw,
            ExecutorSourceType::MCP,
            ExecutorSourceType::Local302AI,
            ExecutorSourceType::LocalProcess,
            ExecutorSourceType::Browser,
            ExecutorSourceType::Docker,
            ExecutorSourceType::Custom,
        ] {
            let mut p = test_passport(st, "x");
            p.executor_id = format!("exec-{st:?}");
            assert_source(p.clone(), &format!("exec-{st:?}"));
        }
    }

    #[test]
    fn build_executor_ref_clones() {
        let p = test_passport(ExecutorSourceType::LocalProcess, "Local");
        let adapter = build_executor_ref(&p);
        assert_eq!(
            adapter.passport().source_type,
            ExecutorSourceType::LocalProcess
        );
    }
}
