//! Real Docker executor backed by the `docker` CLI.
//!
//! Runs a container with `docker run --rm --name <run> [mounts] <image> [cmd]`
//! via [`tokio::process`], enforces a timeout, captures logs, and maps the exit
//! code. Using the CLI (rather than the `bollard` crate) keeps the crate
//! dependency-free and portable across the Docker / Podman-with-docker-alias
//! setups developers actually run.
//!
//! If `docker` is not installed, `health_check`/`dry_run` report that honestly
//! (`online: false` + reason) instead of pretending success.

use crate::config;
use crate::traits::*;
use async_trait::async_trait;
use coevo_core::lease::EmergencyLease;
use coevo_core::opc::{ExternalExecutorPassport, WorkOrder};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
const DOCKER_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
const DOCKER_BIN: &str = "docker";

/// Tracks the container name per run so `cancel` can `docker kill` it.
type ContainerRegistry = Arc<Mutex<HashMap<String, String>>>;

/// Executes work orders inside a Docker container.
pub struct DockerExecutor {
    passport: ExternalExecutorPassport,
    timeout: Duration,
    running: ContainerRegistry,
}

impl DockerExecutor {
    pub fn new(passport: ExternalExecutorPassport) -> Self {
        Self {
            passport,
            timeout: DEFAULT_TIMEOUT,
            running: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Construct the full `docker run` argument vector for a given run id, WITHOUT
    /// executing it. Exposed for unit-testing argument construction.
    pub fn build_run_args(
        &self,
        work_order: &WorkOrder,
        container_name: &str,
    ) -> Result<Vec<String>, ExecutorError> {
        let spec = config::parse_docker_spec(&self.passport.runtime_endpoint).ok_or_else(|| {
            ExecutorError::Internal(format!(
                "Docker executor '{}' has no image in runtime_endpoint '{}'",
                self.passport.executor_id, self.passport.runtime_endpoint
            ))
        })?;
        let mut args: Vec<String> = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--name".to_string(),
            container_name.to_string(),
        ];
        append_default_safety_args(&mut args, &self.passport);
        // Mount the working dir per sandbox level.
        if let Some(dir) = config::working_dir(&self.passport) {
            let mode = if config::is_read_only_sandbox(self.passport.sandbox_level) {
                "ro"
            } else {
                "rw"
            };
            args.push("-v".to_string());
            args.push(format!("{}:/workspace:{}", dir.display(), mode));
            args.push("-w".to_string());
            args.push("/workspace".to_string());
        }
        // Task env (COEVO_*).
        for (k, v) in config::task_env(work_order) {
            args.push("-e".to_string());
            args.push(format!("{k}={v}"));
        }
        args.push(spec.image);
        args.extend(spec.command);
        Ok(args)
    }
}

fn append_default_safety_args(args: &mut Vec<String>, passport: &ExternalExecutorPassport) {
    if !docker_network_allowed(passport) {
        args.push("--network".to_string());
        args.push("none".to_string());
    }
    args.push("--memory".to_string());
    args.push("512m".to_string());
    args.push("--cpus".to_string());
    args.push("1.0".to_string());
    args.push("--pids-limit".to_string());
    args.push("128".to_string());
    args.push("--cap-drop".to_string());
    args.push("ALL".to_string());
    args.push("--security-opt".to_string());
    args.push("no-new-privileges".to_string());
    args.push("--read-only".to_string());
    args.push("--tmpfs".to_string());
    args.push("/tmp:rw,noexec,nosuid,size=64m".to_string());
}

fn docker_network_allowed(passport: &ExternalExecutorPassport) -> bool {
    passport.capabilities.iter().any(|cap| {
        let normalized = cap.trim().to_ascii_lowercase();
        matches!(
            normalized.as_str(),
            "network" | "network-access" | "network_access" | "allow-network" | "allow_network"
        )
    })
}

#[async_trait]
impl ExternalExecutorAdapter for DockerExecutor {
    fn passport(&self) -> &ExternalExecutorPassport {
        &self.passport
    }

    async fn health_check(&self) -> Result<ExecutorHealth, ExecutorError> {
        let start = Instant::now();
        match docker_server_version().await {
            Ok(version) => Ok(ExecutorHealth {
                online: true,
                latency_ms: start.elapsed().as_millis() as u64,
                version: format!("docker {version}"),
            }),
            Err(reason) => Ok(ExecutorHealth {
                online: false,
                latency_ms: start.elapsed().as_millis() as u64,
                version: reason,
            }),
        }
    }

    async fn describe_capabilities(&self) -> Result<Vec<String>, ExecutorError> {
        Ok(self.passport.capabilities.clone())
    }

    async fn dry_run(&self, _work_order: &WorkOrder) -> Result<DryRunResult, ExecutorError> {
        let mut warnings = Vec::new();

        // 1. Docker must be available.
        if let Err(reason) = docker_server_version().await {
            return Ok(DryRunResult {
                passed: false,
                estimated_cost_usd: 0.0,
                estimated_duration_ms: 0,
                warnings: vec![reason],
            });
        }

        // 2. The image reference must parse.
        let spec = match config::parse_docker_spec(&self.passport.runtime_endpoint) {
            Some(s) => s,
            None => {
                return Ok(DryRunResult {
                    passed: false,
                    estimated_cost_usd: 0.0,
                    estimated_duration_ms: 0,
                    warnings: vec![format!(
                        "no image in runtime_endpoint '{}'",
                        self.passport.runtime_endpoint
                    )],
                })
            }
        };

        // 3. Inspect the image locally WITHOUT running the container. A missing
        //    image is a warning (it may be pullable), not a hard failure.
        let inspected =
            run_docker(&["image", "inspect", &spec.image], Duration::from_secs(20)).await;
        let image_present = matches!(&inspected, Ok(out) if out.status_success);
        if !image_present {
            warnings.push(format!(
                "image '{}' not present locally (will require pull)",
                spec.image
            ));
        }
        Ok(DryRunResult {
            passed: true,
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
        let container_name = format!("coevo-{}", &run_id);
        let args = self.build_run_args(work_order, &container_name)?;

        {
            self.running
                .lock()
                .unwrap()
                .insert(run_id.clone(), container_name.clone());
        }

        let start = Instant::now();
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let result = run_docker(&arg_refs, self.timeout).await;
        let duration_ms = start.elapsed().as_millis() as u64;
        self.running.lock().unwrap().remove(&run_id);

        let out = match result {
            Ok(out) => out,
            Err(DockerError::Timeout) => {
                // Ensure the container is gone.
                let _ = run_docker(&["kill", &container_name], Duration::from_secs(10)).await;
                return Err(ExecutorError::Timeout);
            }
            Err(DockerError::NotInstalled) => {
                return Err(ExecutorError::Internal(
                    "docker is not installed or not on PATH".to_string(),
                ))
            }
            Err(DockerError::Io(e)) => return Err(ExecutorError::Internal(e)),
        };

        let success = out.exit_code == Some(0);
        let output = serde_json::json!({
            "executor_id": self.passport.executor_id,
            "source_type": self.passport.source_type,
            "work_order_id": work_order.work_order_id,
            "container": container_name,
            "exit_code": out.exit_code,
            "stdout": out.stdout,
            "stderr": out.stderr,
            "stdout_truncated": out.stdout_truncated,
            "stderr_truncated": out.stderr_truncated,
            "duration_ms": duration_ms,
        });
        Ok(ExecutorResult {
            run_id: run_id.clone(),
            success,
            output,
            audit_trace: format!(
                "docker:{}:{}:exit={:?}",
                self.passport.executor_id, run_id, out.exit_code
            ),
            cost_usd: 0.0,
        })
    }

    async fn cancel(&self, run_id: &str) -> Result<(), ExecutorError> {
        let container = self.running.lock().unwrap().get(run_id).cloned();
        match container {
            Some(name) => {
                let out = run_docker(&["kill", &name], Duration::from_secs(15)).await;
                match out {
                    Ok(o) if o.status_success => {
                        self.running.lock().unwrap().remove(run_id);
                        Ok(())
                    }
                    Ok(o) => Err(ExecutorError::Internal(format!(
                        "docker kill failed: {}",
                        o.stderr.trim()
                    ))),
                    Err(DockerError::Timeout) => Err(ExecutorError::Timeout),
                    Err(DockerError::NotInstalled) => Err(ExecutorError::Internal(
                        "docker is not installed".to_string(),
                    )),
                    Err(DockerError::Io(e)) => Err(ExecutorError::Internal(e)),
                }
            }
            None => Err(ExecutorError::Internal(format!(
                "no running docker container for run_id '{run_id}'"
            ))),
        }
    }

    async fn fetch_audit(&self, run_id: &str) -> Result<String, ExecutorError> {
        Ok(format!(
            "docker:audit:{}:{}",
            self.passport.executor_id, run_id
        ))
    }
}

struct DockerOutput {
    status_success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

#[derive(Debug)]
enum DockerError {
    NotInstalled,
    Timeout,
    Io(String),
}

/// Run a `docker` subcommand with a timeout, capturing bounded output.
/// Distinguishes a missing docker binary from other IO errors so callers can
/// report honestly.
async fn run_docker(args: &[&str], timeout: Duration) -> Result<DockerOutput, DockerError> {
    run_command_with_output_limit(DOCKER_BIN, args, timeout).await
}

async fn run_command_with_output_limit(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<DockerOutput, DockerError> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DockerError::NotInstalled
        } else {
            DockerError::Io(e.to_string())
        }
    })?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::spawn(read_limited_output(stdout));
    let stderr_task = tokio::spawn(read_limited_output(stderr));

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return Err(DockerError::Io(e.to_string())),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(DockerError::Timeout);
        }
    };

    let (stdout, stdout_truncated) = stdout_task
        .await
        .map_err(|e| DockerError::Io(format!("stdout task failed: {e}")))??;
    let (stderr, stderr_truncated) = stderr_task
        .await
        .map_err(|e| DockerError::Io(format!("stderr task failed: {e}")))??;

    Ok(DockerOutput {
        status_success: status.success(),
        exit_code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).to_string(),
        stderr: String::from_utf8_lossy(&stderr).to_string(),
        stdout_truncated,
        stderr_truncated,
    })
}

async fn read_limited_output<R>(reader: Option<R>) -> Result<(Vec<u8>, bool), DockerError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let Some(mut reader) = reader else {
        return Ok((Vec::new(), false));
    };
    let mut out = Vec::with_capacity(DOCKER_OUTPUT_LIMIT_BYTES.min(8192));
    let mut buf = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let n = reader
            .read(&mut buf)
            .await
            .map_err(|e| DockerError::Io(format!("read failed: {e}")))?;
        if n == 0 {
            break;
        }
        let remaining = DOCKER_OUTPUT_LIMIT_BYTES.saturating_sub(out.len());
        if remaining > 0 {
            let keep = remaining.min(n);
            out.extend_from_slice(&buf[..keep]);
            if keep < n {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }
    Ok((out, truncated))
}

/// `docker version` server version string, or an honest offline reason.
async fn docker_server_version() -> Result<String, String> {
    let out = run_docker(
        &["version", "--format", "{{.Server.Version}}"],
        Duration::from_secs(15),
    )
    .await
    .map_err(|e| match e {
        DockerError::NotInstalled => "docker not installed or not on PATH".to_string(),
        DockerError::Timeout => "docker version timed out".to_string(),
        DockerError::Io(s) => format!("docker version error: {s}"),
    })?;
    if !out.status_success {
        // Daemon not running typically: stderr has "Cannot connect to the Docker daemon".
        return Err(format!(
            "docker daemon unreachable: {}",
            out.stderr.trim().lines().next().unwrap_or("unknown error")
        ));
    }
    let v = out.stdout.trim();
    if v.is_empty() {
        Ok("unknown".to_string())
    } else {
        Ok(v.to_string())
    }
}

/// True when a local docker daemon answers `docker version`. Used by tests to
/// skip real-docker cases on CI hosts without docker.
pub async fn docker_available() -> bool {
    docker_server_version().await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{test_passport, test_work_order};
    use coevo_core::opc::{ExecutorSourceType, SandboxLevel};

    fn docker_passport(endpoint: &str) -> ExternalExecutorPassport {
        let mut p = test_passport(ExecutorSourceType::Docker, "Docker Runtime");
        p.runtime_endpoint = endpoint.to_string();
        p
    }

    #[test]
    fn build_run_args_includes_rm_name_image_and_command() {
        let exec = DockerExecutor::new(docker_passport("docker:alpine:3.20 echo hi"));
        let args = exec
            .build_run_args(&test_work_order(), "coevo-test")
            .unwrap();
        assert_eq!(&args[0], "run");
        assert!(args.contains(&"--rm".to_string()));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--name" && w[1] == "coevo-test"));
        // image then command appear in order at the end.
        let image_idx = args.iter().position(|a| a == "alpine:3.20").unwrap();
        assert_eq!(args[image_idx + 1], "echo");
        assert_eq!(args[image_idx + 2], "hi");
        // task env injected.
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-e" && w[1].starts_with("COEVO_WORK_ORDER_ID=")));
    }

    #[test]
    fn build_run_args_mounts_read_only_for_container_sandbox() {
        let mut p = docker_passport("docker:busybox");
        p.sandbox_level = SandboxLevel::Container;
        p.file_scope = vec!["/tmp/work".to_string()];
        let exec = DockerExecutor::new(p);
        let args = exec.build_run_args(&test_work_order(), "c1").unwrap();
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-v" && w[1] == "/tmp/work:/workspace:ro"));
    }

    #[test]
    fn build_run_args_mounts_read_write_for_process_sandbox() {
        let mut p = docker_passport("docker:busybox");
        p.sandbox_level = SandboxLevel::Process;
        p.file_scope = vec!["/tmp/work".to_string()];
        let exec = DockerExecutor::new(p);
        let args = exec.build_run_args(&test_work_order(), "c1").unwrap();
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-v" && w[1] == "/tmp/work:/workspace:rw"));
    }

    #[test]
    fn build_run_args_adds_default_container_safety_flags() {
        let exec = DockerExecutor::new(docker_passport("docker:busybox echo hi"));
        let args = exec
            .build_run_args(&test_work_order(), "coevo-test")
            .unwrap();

        assert!(args
            .windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--memory" && w[1] == "512m"));
        assert!(args.windows(2).any(|w| w[0] == "--cpus" && w[1] == "1.0"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--pids-limit" && w[1] == "128"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--cap-drop" && w[1] == "ALL"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--security-opt" && w[1] == "no-new-privileges"));
        assert!(args.contains(&"--read-only".to_string()));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--tmpfs" && w[1].starts_with("/tmp:")));
    }

    #[test]
    fn build_run_args_preserves_network_when_capability_declares_it() {
        let mut passport = docker_passport("docker:busybox wget https://example.com");
        passport.capabilities.push("network-access".to_string());
        let exec = DockerExecutor::new(passport);
        let args = exec
            .build_run_args(&test_work_order(), "coevo-test")
            .unwrap();

        assert!(!args
            .windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none"));
    }
    #[tokio::test]
    async fn command_output_is_capped_without_waiting_for_full_buffer() {
        let args: Vec<&str> = if cfg!(windows) {
            vec![
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::Out.Write(('x' * 200000))",
            ]
        } else {
            vec!["-c", "yes x | head -c 200000"]
        };
        let program = if cfg!(windows) {
            "powershell.exe"
        } else {
            "/bin/sh"
        };

        let out = run_command_with_output_limit(program, &args, Duration::from_secs(10))
            .await
            .expect("test command should run");

        assert!(out.status_success);
        assert!(out.stdout.len() <= DOCKER_OUTPUT_LIMIT_BYTES);
        assert!(out.stdout_truncated);
    }
    #[test]
    fn build_run_args_errors_without_image() {
        let exec = DockerExecutor::new(docker_passport("docker:"));
        assert!(exec.build_run_args(&test_work_order(), "c1").is_err());
    }

    #[tokio::test]
    async fn cancel_without_running_container_is_explicit_error() {
        let exec = DockerExecutor::new(docker_passport("docker:busybox"));
        let err = exec.cancel("nope").await.unwrap_err();
        assert!(matches!(err, ExecutorError::Internal(_)));
    }

    #[tokio::test]
    async fn health_and_dry_run_are_honest_when_docker_missing() {
        // Only meaningful where docker is absent; where present, assert online.
        let exec = DockerExecutor::new(docker_passport("docker:busybox"));
        let health = exec.health_check().await.unwrap();
        if docker_available().await {
            assert!(health.online);
        } else {
            assert!(!health.online);
            assert!(!health.version.is_empty());
            let dry = exec.dry_run(&test_work_order()).await.unwrap();
            assert!(!dry.passed, "dry_run must not pass without docker");
        }
    }

    #[tokio::test]
    async fn execute_runs_real_container_when_docker_present() {
        if !docker_available().await {
            eprintln!("skipping: docker not available");
            return;
        }
        // busybox echo — small, ubiquitous.
        let exec = DockerExecutor::new(docker_passport("docker:busybox echo coevo-ok"))
            .with_timeout(Duration::from_secs(120));
        let result = exec.execute(&test_work_order(), None).await.unwrap();
        assert!(result.success);
        assert!(result.output["stdout"]
            .as_str()
            .unwrap_or("")
            .contains("coevo-ok"));
    }
}
