//! Real local-subprocess executor.
//!
//! Runs the command from the passport's `runtime_endpoint` in a child process,
//! capturing stdout/stderr/exit-code/duration and enforcing a timeout. A
//! registry of live children keyed by `run_id` lets [`cancel`] kill an
//! in-flight run.
//!
//! [`cancel`]: ExternalExecutorAdapter::cancel

use crate::config;
use crate::traits::*;
use async_trait::async_trait;
use coevo_core::lease::EmergencyLease;
use coevo_core::opc::{ExternalExecutorPassport, WorkOrder};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

/// Default wall-clock limit for a single subprocess execution.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Registry of running children keyed by `run_id` so `cancel` can kill them.
type ChildRegistry = Arc<Mutex<HashMap<String, Child>>>;

/// Executes a command in a subprocess per the passport's command template.
pub struct LocalProcessExecutor {
    passport: ExternalExecutorPassport,
    timeout: Duration,
    running: ChildRegistry,
}

impl LocalProcessExecutor {
    pub fn new(passport: ExternalExecutorPassport) -> Self {
        Self {
            passport,
            timeout: DEFAULT_TIMEOUT,
            running: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Override the per-execution timeout (used by tests).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build the `Command` for this work order without spawning it. Shared by
    /// `execute` and exposed for unit testing of argument construction.
    fn build_command(&self, work_order: &WorkOrder) -> Result<Command, ExecutorError> {
        let parsed =
            config::parse_process_command(&self.passport.runtime_endpoint).ok_or_else(|| {
                ExecutorError::Internal(format!(
                    "LocalProcess executor '{}' has no command in runtime_endpoint '{}'",
                    self.passport.executor_id, self.passport.runtime_endpoint
                ))
            })?;
        let mut cmd = Command::new(&parsed.program);
        cmd.args(&parsed.args);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (k, v) in config::task_env(work_order) {
            cmd.env(k, v);
        }
        if let Some(dir) = config::working_dir(&self.passport) {
            cmd.current_dir(dir);
        }
        Ok(cmd)
    }
}

#[async_trait]
impl ExternalExecutorAdapter for LocalProcessExecutor {
    fn passport(&self) -> &ExternalExecutorPassport {
        &self.passport
    }

    async fn health_check(&self) -> Result<ExecutorHealth, ExecutorError> {
        let start = Instant::now();
        let parsed = match config::parse_process_command(&self.passport.runtime_endpoint) {
            Some(p) => p,
            None => {
                return Ok(ExecutorHealth {
                    online: false,
                    latency_ms: start.elapsed().as_millis() as u64,
                    version: "no command configured".to_string(),
                })
            }
        };
        // Liveness = the program resolves on PATH. Cheap and side-effect free.
        match resolve_binary(&parsed.program).await {
            Some(path) => Ok(ExecutorHealth {
                online: true,
                latency_ms: start.elapsed().as_millis() as u64,
                version: format!("local-process:{path}"),
            }),
            None => Ok(ExecutorHealth {
                online: false,
                latency_ms: start.elapsed().as_millis() as u64,
                version: format!("binary '{}' not found on PATH", parsed.program),
            }),
        }
    }

    async fn describe_capabilities(&self) -> Result<Vec<String>, ExecutorError> {
        Ok(self.passport.capabilities.clone())
    }

    async fn dry_run(&self, _work_order: &WorkOrder) -> Result<DryRunResult, ExecutorError> {
        let mut warnings = Vec::new();
        let parsed = match config::parse_process_command(&self.passport.runtime_endpoint) {
            Some(p) => p,
            None => {
                return Ok(DryRunResult {
                    passed: false,
                    estimated_cost_usd: 0.0,
                    estimated_duration_ms: 0,
                    warnings: vec![format!(
                        "no command configured in runtime_endpoint '{}'",
                        self.passport.runtime_endpoint
                    )],
                })
            }
        };
        // Validate the binary resolves WITHOUT executing it.
        let resolved = resolve_binary(&parsed.program).await;
        if resolved.is_none() {
            warnings.push(format!("binary '{}' not found on PATH", parsed.program));
        }
        if let Some(dir) = config::working_dir(&self.passport) {
            if !dir.is_dir() {
                warnings.push(format!("working dir '{}' does not exist", dir.display()));
            }
        }
        Ok(DryRunResult {
            passed: resolved.is_some(),
            estimated_cost_usd: 0.0,
            estimated_duration_ms: 0,
            warnings,
        })
    }

    async fn execute(
        &self,
        work_order: &WorkOrder,
        _lease: Option<&EmergencyLease>,
    ) -> Result<ExecutorResult, ExecutorError> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let mut cmd = self.build_command(work_order)?;
        let start = Instant::now();
        let mut child = cmd
            .spawn()
            .map_err(|e| ExecutorError::Internal(format!("spawn failed: {e}")))?;

        // Take stdout/stderr handles before registering, so cancel only holds
        // the Child (and its kill handle).
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        {
            let mut reg = self.running.lock().unwrap();
            reg.insert(run_id.clone(), child);
        }

        let collected = tokio::time::timeout(
            self.timeout,
            collect_output(&self.running, &run_id, stdout, stderr),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        let output = match collected {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                self.running.lock().unwrap().remove(&run_id);
                return Err(ExecutorError::Internal(e));
            }
            Err(_elapsed) => {
                // Timed out: kill the child if still registered.
                if let Some(mut child) = self.running.lock().unwrap().remove(&run_id) {
                    let _ = child.start_kill();
                }
                return Err(ExecutorError::Timeout);
            }
        };

        let success = output.exit_code == Some(0);
        let result_output = serde_json::json!({
            "executor_id": self.passport.executor_id,
            "source_type": self.passport.source_type,
            "work_order_id": work_order.work_order_id,
            "exit_code": output.exit_code,
            "stdout": output.stdout,
            "stderr": output.stderr,
            "duration_ms": duration_ms,
        });
        Ok(ExecutorResult {
            run_id: run_id.clone(),
            success,
            output: result_output,
            audit_trace: format!(
                "local-process:{}:{}:exit={:?}",
                self.passport.executor_id, run_id, output.exit_code
            ),
            cost_usd: 0.0,
        })
    }

    async fn cancel(&self, run_id: &str) -> Result<(), ExecutorError> {
        let child = self.running.lock().unwrap().remove(run_id);
        match child {
            Some(mut child) => {
                child
                    .start_kill()
                    .map_err(|e| ExecutorError::Internal(format!("kill failed: {e}")))?;
                Ok(())
            }
            None => Err(ExecutorError::Internal(format!(
                "no running local-process execution with run_id '{run_id}'"
            ))),
        }
    }

    async fn fetch_audit(&self, run_id: &str) -> Result<String, ExecutorError> {
        Ok(format!(
            "local-process:audit:{}:{}",
            self.passport.executor_id, run_id
        ))
    }
}

struct CollectedOutput {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Wait for the child (looked up from the registry) to exit and gather its
/// output. Removes the child from the registry on completion.
async fn collect_output(
    registry: &ChildRegistry,
    run_id: &str,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
) -> Result<CollectedOutput, String> {
    use tokio::io::AsyncReadExt;

    let mut out_buf = Vec::new();
    let mut err_buf = Vec::new();
    if let Some(mut s) = stdout {
        let _ = s.read_to_end(&mut out_buf).await;
    }
    if let Some(mut s) = stderr {
        let _ = s.read_to_end(&mut err_buf).await;
    }

    // Reclaim the child to await its exit status. If cancel already removed it,
    // report that explicitly.
    let mut child = registry
        .lock()
        .unwrap()
        .remove(run_id)
        .ok_or_else(|| "execution was cancelled".to_string())?;
    let status = child
        .wait()
        .await
        .map_err(|e| format!("wait failed: {e}"))?;
    Ok(CollectedOutput {
        exit_code: status.code(),
        stdout: String::from_utf8_lossy(&out_buf).to_string(),
        stderr: String::from_utf8_lossy(&err_buf).to_string(),
    })
}

/// Resolve a program name to an absolute path using the platform PATH lookup
/// (`where` on Windows, `which` elsewhere), WITHOUT executing the program. An
/// absolute/relative path that already exists is returned as-is.
async fn resolve_binary(program: &str) -> Option<String> {
    let p = std::path::Path::new(program);
    if p.is_absolute() || program.contains('/') || program.contains('\\') {
        return if p.is_file() {
            Some(program.to_string())
        } else {
            None
        };
    }
    let locator = if cfg!(windows) { "where" } else { "which" };
    let output = Command::new(locator)
        .arg(program)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{test_passport, test_work_order};
    use coevo_core::opc::ExecutorSourceType;

    fn local_passport(runtime_endpoint: &str) -> ExternalExecutorPassport {
        let mut p = test_passport(ExecutorSourceType::LocalProcess, "Local Process");
        p.runtime_endpoint = runtime_endpoint.to_string();
        p
    }

    /// A command that exists on both Windows and Unix CI: `cargo --version`.
    fn ok_command() -> &'static str {
        "proc:cargo --version"
    }

    #[tokio::test]
    async fn execute_runs_command_and_captures_output() {
        let exec = LocalProcessExecutor::new(local_passport(ok_command()));
        let result = exec
            .execute(&test_work_order(), None)
            .await
            .expect("execute should succeed");
        assert!(result.success, "cargo --version should exit 0");
        let stdout = result.output["stdout"].as_str().unwrap_or("");
        assert!(
            stdout.contains("cargo"),
            "stdout should mention cargo, got: {stdout}"
        );
        assert_eq!(result.output["exit_code"], serde_json::json!(0));
    }

    #[tokio::test]
    async fn dry_run_validates_binary_without_executing() {
        let exec = LocalProcessExecutor::new(local_passport(ok_command()));
        let dry = exec.dry_run(&test_work_order()).await.unwrap();
        assert!(dry.passed);
        assert!(dry.warnings.is_empty());
    }

    #[tokio::test]
    async fn dry_run_fails_for_missing_binary() {
        let exec =
            LocalProcessExecutor::new(local_passport("proc:this-binary-does-not-exist-coevo-xyz"));
        let dry = exec.dry_run(&test_work_order()).await.unwrap();
        assert!(!dry.passed);
        assert!(dry.warnings.iter().any(|w| w.contains("not found")));
    }

    #[tokio::test]
    async fn health_check_reports_offline_for_missing_binary() {
        let exec =
            LocalProcessExecutor::new(local_passport("proc:this-binary-does-not-exist-coevo-xyz"));
        let health = exec.health_check().await.unwrap();
        assert!(!health.online);
    }

    #[tokio::test]
    async fn execute_maps_nonzero_exit_to_failure() {
        // `cargo --frobnicate-nonexistent` exits non-zero but resolves the binary.
        let exec = LocalProcessExecutor::new(local_passport("proc:cargo --frobnicate-nonexistent"));
        let result = exec.execute(&test_work_order(), None).await.unwrap();
        assert!(!result.success, "unknown cargo flag should exit non-zero");
        assert_ne!(result.output["exit_code"], serde_json::json!(0));
    }

    #[tokio::test]
    async fn execute_enforces_timeout() {
        // A long sleep, cross-platform, that should be killed by the 100ms timeout.
        let cmd = if cfg!(windows) {
            // ping with a delay is a portable "sleep" on Windows without a shell.
            "proc:ping -n 30 127.0.0.1"
        } else {
            "proc:sleep 30"
        };
        let exec =
            LocalProcessExecutor::new(local_passport(cmd)).with_timeout(Duration::from_millis(150));
        let result = exec.execute(&test_work_order(), None).await;
        assert!(matches!(result, Err(ExecutorError::Timeout)));
    }

    #[tokio::test]
    async fn cancel_without_running_run_is_explicit_error() {
        let exec = LocalProcessExecutor::new(local_passport(ok_command()));
        let err = exec.cancel("nonexistent-run").await.unwrap_err();
        assert!(matches!(err, ExecutorError::Internal(_)));
    }
}
