//! Test-only helpers shared across adapter unit tests: passport/work-order
//! builders and a tiny dependency-free HTTP stub server (std `TcpListener` on a
//! tokio task), so the HTTP-runtime tests need no axum/hyper dev-dependency.

use coevo_core::opc::*;

/// A minimal registered passport for a given source type.
pub fn test_passport(
    source_type: ExecutorSourceType,
    display_name: &str,
) -> ExternalExecutorPassport {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    ExternalExecutorPassport {
        executor_id: format!("exec-{}", uuid::Uuid::new_v4()),
        display_name: display_name.to_string(),
        source_type,
        runtime_endpoint: String::new(),
        capabilities: vec![
            "executor.inspect".to_string(),
            "executor.execute".to_string(),
        ],
        required_credentials: vec![],
        permission_boundary: PermissionBoundary {
            max_risk_score: 0.5,
            can_write_fact: false,
            can_write_decision: false,
            can_access_network: true,
            can_access_filesystem: true,
            can_call_external_executor: true,
            can_propose_skill: false,
        },
        file_scope: vec![],
        network_scope: vec![],
        memory_scope: MemoryScope::Executor,
        risk_ceiling: 0.6,
        supported_actions: vec!["inspect".to_string(), "execute".to_string()],
        sandbox_level: SandboxLevel::Process,
        health_check_url: String::new(),
        audit_callback_url: String::new(),
        status: ExecutorStatus::Registered,
        created_at_ms: now,
        updated_at_ms: now,
    }
}

/// A trivial green-track work order for execute/dry_run tests.
pub fn test_work_order() -> WorkOrder {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    WorkOrder {
        work_order_id: format!("wo-{}", uuid::Uuid::new_v4()),
        conversation_id: None,
        contract_hash: "a".repeat(64),
        plan_hash: "b".repeat(64),
        user_id: "default-founder".to_string(),
        opc_id: "default-opc".to_string(),
        mission_intent: "echo a friendly greeting".to_string(),
        selected_agents: vec![],
        selected_executors: vec![],
        required_skills: vec![],
        track: "green".to_string(),
        status: WorkOrderStatus::Planned,
        allowed_actions: vec!["read".to_string(), "execute".to_string()],
        restricted_actions: vec![],
        risk_summary: "low".to_string(),
        governance_proposal: None,
        governance_verdict: None,
        created_at_ms: now,
        updated_at_ms: now,
    }
}

/// A canned HTTP response the stub server returns for every request.
#[derive(Clone)]
pub struct StubResponse {
    pub status: u16,
    pub body: String,
}

/// A running HTTP stub bound to an ephemeral localhost port.
pub struct StubServer {
    port: u16,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl StubServer {
    /// The base URL (`http://127.0.0.1:<port>`) to point an executor at.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Signal the accept loop to stop and abort the task.
    pub fn shutdown(mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

impl Drop for StubServer {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

/// Start a stub HTTP server that replies to every request with `response`.
///
/// Uses a blocking `std::net::TcpListener` driven on a `spawn_blocking` task; it
/// reads (and discards) the request headers/body up to the blank line, then
/// writes a fixed HTTP/1.1 response with `Connection: close`. Sufficient for the
/// request/response *mapping* the adapters need to be tested against.
pub async fn stub_server(response: StubResponse) -> StubServer {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    let handle = tokio::task::spawn_blocking(move || {
        while !shutdown_clone.load(std::sync::atomic::Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _addr)) => {
                    // Read what is immediately available; we don't need the full
                    // body, just to let the client finish writing.
                    let mut buf = [0u8; 2048];
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(200)));
                    let _ = stream.read(&mut buf);

                    let reason = match response.status {
                        200 => "OK",
                        404 => "Not Found",
                        500 => "Internal Server Error",
                        _ => "Status",
                    };
                    let http = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.status,
                        reason,
                        response.body.as_bytes().len(),
                        response.body
                    );
                    let _ = stream.write_all(http.as_bytes());
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    StubServer {
        port,
        shutdown,
        handle: Some(handle),
    }
}
