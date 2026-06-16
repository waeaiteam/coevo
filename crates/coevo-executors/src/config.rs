//! Passport → executor configuration conventions.
//!
//! The [`ExternalExecutorPassport`](coevo_core::opc::ExternalExecutorPassport)
//! has no free-form metadata map, so every adapter derives its runtime
//! configuration from the *existing* passport fields using the conventions in
//! this module. Keeping the parsing here (rather than in each adapter) means the
//! conventions are documented and tested in one place.
//!
//! ## `runtime_endpoint` conventions (per source type)
//!
//! * **LocalProcess** — a command template. An optional `proc:` prefix is
//!   stripped. The first whitespace-separated token is the program; the rest are
//!   default arguments. Example: `proc:cargo --version` or `python worker.py`.
//! * **Hermes / OpenClaw / Local302AI / Custom (HTTP runtimes)** — an `http://`
//!   or `https://` base URL that accepts a JSON `POST` of the task. A trailing
//!   slash is trimmed.
//! * **Docker** — `docker:<image>[ <cmd>...]` or `image:<ref>` or a bare image
//!   reference. The first token after the prefix is the image; the rest is the
//!   in-container command (overriding the image entrypoint).
//! * **MCP** — handled via the MCP tool path, not here (see [`crate::mcp`]); the
//!   endpoint is only used for a best-effort HTTP liveness probe.
//!
//! ## Credentials (`required_credentials`)
//!
//! Credentials are **references**, never literal secrets. Supported forms, each
//! resolved from a process environment variable so secrets stay out of the DB:
//!
//! * `env:VAR`            → `Authorization: Bearer $VAR`
//! * `bearer-env:VAR`     → `Authorization: Bearer $VAR` (explicit)
//! * `header:Name=env:VAR`→ `Name: $VAR` (arbitrary header)
//! * `apikey-env:VAR`     → `x-api-key: $VAR`
//!
//! Anything that does not start with one of those tags is ignored for header
//! construction (it may be a `keyring:` ref consumed elsewhere). A missing env
//! var is surfaced as a warning by `dry_run`, not a hard failure at parse time.
//!
//! ## Working directory & sandbox
//!
//! The working directory is the first entry of `file_scope` when present. The
//! [`SandboxLevel`](coevo_core::opc::SandboxLevel) governs read-only vs
//! read-write container mounts and whether a process executor must be confined.

use coevo_core::opc::{ExternalExecutorPassport, SandboxLevel};
use std::collections::HashMap;
use std::path::PathBuf;

/// A single auth header resolved from a credential reference.
#[derive(Debug, Clone)]
pub struct AuthHeader {
    pub name: String,
    pub value: String,
}

/// Parsed view over the credential references in a passport.
#[derive(Debug, Clone, Default)]
pub struct ResolvedCredentials {
    /// Headers that resolved successfully (env var was present).
    pub headers: Vec<AuthHeader>,
    /// Credential refs whose env var was missing — reported by `dry_run`.
    pub missing: Vec<String>,
    /// Credential refs we did not recognise as header sources (e.g. keyring).
    pub unrecognised: Vec<String>,
}

/// Resolve `required_credentials` into concrete HTTP headers, reading secret
/// values from the process environment. Never logs the values.
pub fn resolve_credentials(refs: &[String]) -> ResolvedCredentials {
    let mut out = ResolvedCredentials::default();
    for raw in refs {
        let cref = raw.trim();
        if cref.is_empty() {
            continue;
        }
        if let Some(var) = cref
            .strip_prefix("bearer-env:")
            .or_else(|| cref.strip_prefix("env:"))
        {
            match read_env(var) {
                Some(v) => out.headers.push(AuthHeader {
                    name: "Authorization".to_string(),
                    value: format!("Bearer {v}"),
                }),
                None => out.missing.push(cref.to_string()),
            }
        } else if let Some(var) = cref.strip_prefix("apikey-env:") {
            match read_env(var) {
                Some(v) => out.headers.push(AuthHeader {
                    name: "x-api-key".to_string(),
                    value: v,
                }),
                None => out.missing.push(cref.to_string()),
            }
        } else if let Some(spec) = cref.strip_prefix("header:") {
            // header:Name=env:VAR
            match spec.split_once('=') {
                Some((name, src)) => {
                    let var = src.trim().strip_prefix("env:").unwrap_or(src.trim());
                    match read_env(var) {
                        Some(v) => out.headers.push(AuthHeader {
                            name: name.trim().to_string(),
                            value: v,
                        }),
                        None => out.missing.push(cref.to_string()),
                    }
                }
                None => out.unrecognised.push(cref.to_string()),
            }
        } else {
            // e.g. keyring: refs are consumed by other layers, not HTTP headers.
            out.unrecognised.push(cref.to_string());
        }
    }
    out
}

fn read_env(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|v| !v.is_empty())
}

/// A local subprocess command parsed from `runtime_endpoint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// Parse the LocalProcess `runtime_endpoint` command template. Returns `None`
/// when no program token is present.
///
/// Tokenisation is whitespace-based (no shell quoting) on purpose: it stays
/// identical on Windows and Unix and avoids invoking a shell, so there are no
/// bash-isms or injection surface. Callers that need quoted args should encode
/// them differently; this is sufficient for `prog --flag value` templates.
pub fn parse_process_command(runtime_endpoint: &str) -> Option<ProcessCommand> {
    let trimmed = runtime_endpoint
        .trim()
        .strip_prefix("proc:")
        .unwrap_or(runtime_endpoint.trim())
        .trim();
    let mut tokens = trimmed.split_whitespace().map(|s| s.to_string());
    let program = tokens.next()?;
    if program.is_empty() {
        return None;
    }
    Some(ProcessCommand {
        program,
        args: tokens.collect(),
    })
}

/// A Docker image + optional in-container command parsed from `runtime_endpoint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerSpec {
    pub image: String,
    pub command: Vec<String>,
}

/// Parse the Docker `runtime_endpoint`: `docker:<image>[ cmd...]`,
/// `image:<ref>`, or a bare `<image>`.
pub fn parse_docker_spec(runtime_endpoint: &str) -> Option<DockerSpec> {
    let trimmed = runtime_endpoint.trim();
    let body = trimmed
        .strip_prefix("docker:")
        .or_else(|| trimmed.strip_prefix("image:"))
        .unwrap_or(trimmed)
        .trim();
    let mut tokens = body.split_whitespace().map(|s| s.to_string());
    let image = tokens.next()?;
    if image.is_empty() {
        return None;
    }
    Some(DockerSpec {
        image,
        command: tokens.collect(),
    })
}

/// Resolve the HTTP base URL for an HTTP runtime, trimming a trailing slash.
/// Returns `None` when the endpoint is not http(s).
pub fn parse_http_base(runtime_endpoint: &str) -> Option<String> {
    let t = runtime_endpoint.trim();
    if t.starts_with("http://") || t.starts_with("https://") {
        Some(t.trim_end_matches('/').to_string())
    } else {
        None
    }
}

/// Compute the health URL for an HTTP runtime. Prefers an explicit
/// `health_check_url` (when http(s)); otherwise appends `/health` to the base.
pub fn health_url(passport: &ExternalExecutorPassport) -> Option<String> {
    if let Some(explicit) = parse_http_base(&passport.health_check_url) {
        return Some(explicit);
    }
    parse_http_base(&passport.runtime_endpoint).map(|base| format!("{base}/health"))
}

/// The working directory for an executor: first `file_scope` entry, if any.
pub fn working_dir(passport: &ExternalExecutorPassport) -> Option<PathBuf> {
    passport
        .file_scope
        .first()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

/// Whether the sandbox level implies a read-only working directory mount.
pub fn is_read_only_sandbox(level: SandboxLevel) -> bool {
    // `None`/`Process` get read-write; container/vm/remote tiers default to
    // read-only mounts so a container cannot mutate the host workspace unless
    // explicitly widened.
    matches!(
        level,
        SandboxLevel::Container | SandboxLevel::VM | SandboxLevel::Remote
    )
}

/// Build the JSON task payload POSTed to an HTTP runtime / passed to a process.
/// Derived from the [`WorkOrder`](coevo_core::opc::WorkOrder) plus the passport
/// identity, so the runtime sees the mission and its governance envelope.
pub fn task_payload(
    passport: &ExternalExecutorPassport,
    work_order: &coevo_core::opc::WorkOrder,
    dry_run: bool,
) -> serde_json::Value {
    serde_json::json!({
        "executor_id": passport.executor_id,
        "source_type": passport.source_type,
        "work_order_id": work_order.work_order_id,
        "mission_intent": work_order.mission_intent,
        "track": work_order.track,
        "allowed_actions": work_order.allowed_actions,
        "restricted_actions": work_order.restricted_actions,
        "selected_executors": work_order.selected_executors,
        "dry_run": dry_run,
    })
}

/// Environment variables exported to a child process/container describing the
/// task. Prefixed `COEVO_` so they don't collide with the program's own env.
pub fn task_env(work_order: &coevo_core::opc::WorkOrder) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert(
        "COEVO_WORK_ORDER_ID".to_string(),
        work_order.work_order_id.clone(),
    );
    env.insert("COEVO_TRACK".to_string(), work_order.track.clone());
    env.insert(
        "COEVO_MISSION_INTENT".to_string(),
        work_order.mission_intent.clone(),
    );
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_process_command_strips_prefix_and_splits_args() {
        let cmd = parse_process_command("proc:cargo --version").unwrap();
        assert_eq!(cmd.program, "cargo");
        assert_eq!(cmd.args, vec!["--version"]);

        let bare = parse_process_command("  python  worker.py --once ").unwrap();
        assert_eq!(bare.program, "python");
        assert_eq!(bare.args, vec!["worker.py", "--once"]);

        assert!(parse_process_command("   ").is_none());
        assert!(parse_process_command("proc:").is_none());
    }

    #[test]
    fn parse_docker_spec_handles_prefixes() {
        let a = parse_docker_spec("docker:alpine:3.20 echo hi").unwrap();
        assert_eq!(a.image, "alpine:3.20");
        assert_eq!(a.command, vec!["echo", "hi"]);

        let b = parse_docker_spec("image:busybox").unwrap();
        assert_eq!(b.image, "busybox");
        assert!(b.command.is_empty());

        let c = parse_docker_spec("ghcr.io/acme/tool:latest").unwrap();
        assert_eq!(c.image, "ghcr.io/acme/tool:latest");

        assert!(parse_docker_spec("docker:").is_none());
    }

    #[test]
    fn parse_http_base_trims_slash_and_rejects_non_http() {
        assert_eq!(
            parse_http_base("https://api.example.com/"),
            Some("https://api.example.com".to_string())
        );
        assert_eq!(
            parse_http_base("http://localhost:9000"),
            Some("http://localhost:9000".to_string())
        );
        assert!(parse_http_base("coevo://executor/hermes").is_none());
    }

    #[test]
    fn resolve_credentials_reads_env_and_reports_missing() {
        std::env::set_var("COEVO_TEST_TOKEN_OK", "secret-value");
        std::env::remove_var("COEVO_TEST_TOKEN_MISSING");

        let resolved = resolve_credentials(&[
            "env:COEVO_TEST_TOKEN_OK".to_string(),
            "env:COEVO_TEST_TOKEN_MISSING".to_string(),
            "apikey-env:COEVO_TEST_TOKEN_OK".to_string(),
            "header:X-Custom=env:COEVO_TEST_TOKEN_OK".to_string(),
            "keyring:something".to_string(),
        ]);

        assert!(resolved
            .headers
            .iter()
            .any(|h| h.name == "Authorization" && h.value == "Bearer secret-value"));
        assert!(resolved
            .headers
            .iter()
            .any(|h| h.name == "x-api-key" && h.value == "secret-value"));
        assert!(resolved
            .headers
            .iter()
            .any(|h| h.name == "X-Custom" && h.value == "secret-value"));
        assert!(resolved
            .missing
            .contains(&"env:COEVO_TEST_TOKEN_MISSING".to_string()));
        assert!(resolved
            .unrecognised
            .contains(&"keyring:something".to_string()));

        std::env::remove_var("COEVO_TEST_TOKEN_OK");
    }

    #[test]
    fn read_only_sandbox_tiers() {
        assert!(!is_read_only_sandbox(SandboxLevel::None));
        assert!(!is_read_only_sandbox(SandboxLevel::Process));
        assert!(is_read_only_sandbox(SandboxLevel::Container));
        assert!(is_read_only_sandbox(SandboxLevel::VM));
        assert!(is_read_only_sandbox(SandboxLevel::Remote));
    }
}
