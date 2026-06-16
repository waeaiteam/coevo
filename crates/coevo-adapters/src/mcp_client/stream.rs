//! Newline-delimited JSON-RPC framing over any `AsyncRead`/`AsyncWrite` pair,
//! plus the MCP stdio transport (child process) built on top of it.

use super::jsonrpc::{
    self, InboundMessage, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RequestId,
};
use crate::traits::AdapterError;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

pub(crate) const TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";

/// Server-initiated notification state shared between transports and client.
#[derive(Debug, Default)]
pub(crate) struct NotificationState {
    /// Set when the server announced `notifications/tools/list_changed`.
    pub tools_dirty: AtomicBool,
}

pub(crate) fn handle_notification(n: &JsonRpcNotification, state: &NotificationState, label: &str) {
    if n.method == TOOLS_LIST_CHANGED {
        state.tools_dirty.store(true, Ordering::SeqCst);
        debug!(server = %label, "MCP server reported tools/list_changed");
    } else {
        // Log method only; params could contain anything.
        debug!(server = %label, method = %n.method, "ignoring MCP server notification");
    }
}

struct PendingMap {
    map: HashMap<i64, oneshot::Sender<JsonRpcResponse>>,
    closed: bool,
}

/// JSON-RPC peer over a byte stream: writes newline-delimited requests, and a
/// background task routes responses to per-request oneshot channels by id.
pub(crate) struct StreamPeer {
    writer: Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    pending: Arc<StdMutex<PendingMap>>,
    next_id: AtomicI64,
    reader_task: JoinHandle<()>,
}

impl StreamPeer {
    pub fn new<R, W>(
        read: R,
        write: W,
        notifications: Arc<NotificationState>,
        label: String,
    ) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let pending = Arc::new(StdMutex::new(PendingMap {
            map: HashMap::new(),
            closed: false,
        }));
        let reader_task = tokio::spawn(read_loop(read, Arc::clone(&pending), notifications, label));
        Self {
            writer: Mutex::new(Box::new(write)),
            pending,
            next_id: AtomicI64::new(1),
            reader_task,
        }
    }

    /// True while the background reader is still attached to a live stream.
    pub fn is_alive(&self) -> bool {
        !self.pending.lock().expect("pending lock").closed
    }

    pub async fn request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, AdapterError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().expect("pending lock");
            if pending.closed {
                return Err(AdapterError::Unavailable);
            }
            pending.map.insert(id, tx);
        }
        let req = JsonRpcRequest::new(RequestId::Number(id), method, params);
        let line = serde_json::to_string(&req)
            .map_err(|e| AdapterError::McpError(format!("failed to encode request: {e}")))?;
        if let Err(e) = self.send_line(&line).await {
            self.pending.lock().expect("pending lock").map.remove(&id);
            // A failed write means the peer is gone (child exited, pipe
            // closed): report Unavailable so callers can reconnect.
            warn!(error = %e, "failed to write request to MCP server");
            return Err(AdapterError::Unavailable);
        }
        match rx.await {
            Ok(resp) => resp.into_result(),
            // Sender dropped: connection closed while waiting.
            Err(_) => Err(AdapterError::Unavailable),
        }
    }

    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), AdapterError> {
        let n = JsonRpcNotification::new(method, params);
        let line = serde_json::to_string(&n)
            .map_err(|e| AdapterError::McpError(format!("failed to encode notification: {e}")))?;
        self.send_line(&line).await.map_err(|e| {
            warn!(error = %e, "failed to write notification to MCP server");
            AdapterError::Unavailable
        })
    }

    async fn send_line(&self, line: &str) -> std::io::Result<()> {
        let mut writer = self.writer.lock().await;
        writer.write_all(line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await
    }

    /// Close the write half (for stdio this closes the child's stdin).
    pub async fn close(&self) {
        let mut writer = self.writer.lock().await;
        let _ = writer.shutdown().await;
    }
}

impl Drop for StreamPeer {
    fn drop(&mut self) {
        self.reader_task.abort();
    }
}

async fn read_loop<R>(
    read: R,
    pending: Arc<StdMutex<PendingMap>>,
    notifications: Arc<NotificationState>,
    label: String,
) where
    R: AsyncRead + Send + Unpin + 'static,
{
    let mut lines = BufReader::new(read).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match jsonrpc::parse_inbound(line) {
                    Ok(InboundMessage::Response(resp)) => match resp.id.clone() {
                        Some(RequestId::Number(id)) => {
                            let sender = pending.lock().expect("pending lock").map.remove(&id);
                            match sender {
                                Some(tx) => {
                                    let _ = tx.send(resp);
                                }
                                None => warn!(
                                    server = %label,
                                    id,
                                    "MCP response for unknown/expired request id"
                                ),
                            }
                        }
                        other => warn!(
                            server = %label,
                            id = ?other,
                            "MCP response with non-numeric or missing id"
                        ),
                    },
                    Ok(InboundMessage::Notification(n)) => {
                        handle_notification(&n, &notifications, &label)
                    }
                    Ok(InboundMessage::Request(req)) => warn!(
                        server = %label,
                        method = %req.method,
                        "ignoring server-initiated MCP request (tools-only client)"
                    ),
                    Err(e) => warn!(server = %label, error = %e, "unparseable MCP message"),
                }
            }
            Ok(None) => break,
            Err(e) => {
                warn!(server = %label, error = %e, "MCP stream read error");
                break;
            }
        }
    }
    let mut pending = pending.lock().expect("pending lock");
    pending.closed = true;
    // Dropping the senders wakes all in-flight requests with Unavailable.
    pending.map.clear();
    debug!(server = %label, "MCP stream closed");
}

/// MCP stdio transport: a child process speaking newline-delimited JSON-RPC
/// on stdin/stdout, with stderr forwarded to tracing.
pub(crate) struct StdioTransport {
    pub(crate) peer: StreamPeer,
    child: Mutex<Child>,
    stderr_task: Option<JoinHandle<()>>,
}

impl StdioTransport {
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        notifications: Arc<NotificationState>,
        label: String,
    ) -> Result<Self, AdapterError> {
        if command.trim().is_empty() {
            return Err(AdapterError::McpError(
                "stdio MCP server command must not be empty".to_string(),
            ));
        }
        // Direct exec, never `sh -c`: no shell interpretation of args.
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| {
            AdapterError::McpError(format!("failed to spawn MCP server '{command}': {e}"))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AdapterError::McpError("child process stdin unavailable".to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AdapterError::McpError("child process stdout unavailable".to_string())
        })?;
        let stderr = child.stderr.take();

        let stderr_task = stderr.map(|stderr| {
            let label = label.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    warn!(server = %label, "mcp stderr: {line}");
                }
            })
        });

        let peer = StreamPeer::new(stdout, stdin, notifications, label);
        Ok(Self {
            peer,
            child: Mutex::new(child),
            stderr_task,
        })
    }

    pub async fn is_alive(&self) -> bool {
        if !self.peer.is_alive() {
            return false;
        }
        let mut child = self.child.lock().await;
        matches!(child.try_wait(), Ok(None))
    }

    /// Graceful shutdown: close stdin, give the child a moment to exit, then kill.
    pub async fn shutdown(&self) {
        self.peer.close().await;
        let mut child = self.child.lock().await;
        match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
            Ok(Ok(status)) => debug!(?status, "MCP stdio server exited"),
            Ok(Err(e)) => warn!(error = %e, "error waiting for MCP stdio server"),
            Err(_) => {
                warn!("MCP stdio server did not exit after stdin close; killing");
                let _ = child.kill().await;
            }
        }
        if let Some(task) = &self.stderr_task {
            task.abort();
        }
    }
}
