//! MCP Streamable HTTP transport: JSON-RPC over POST, with responses arriving
//! either as a plain JSON body or as an SSE stream, plus `Mcp-Session-Id`
//! session handling.

use super::jsonrpc::{self, InboundMessage, JsonRpcNotification, JsonRpcRequest, RequestId};
use super::stream::{handle_notification, NotificationState};
use crate::traits::AdapterError;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tracing::{debug, warn};

pub(crate) const MCP_SESSION_HEADER: &str = "Mcp-Session-Id";

/// Outcome of sending one JSON-RPC request over HTTP.
pub(crate) enum HttpOutcome {
    /// The server answered with a JSON-RPC response for our id.
    Response(Result<Value, AdapterError>),
    /// HTTP 404 while we held a session id: the session expired and the
    /// caller should re-initialize once, then retry.
    SessionExpired,
}

pub(crate) struct HttpTransport {
    client: reqwest::Client,
    url: String,
    /// Custom headers (auth etc.). Values are secrets: never logged.
    headers: HashMap<String, String>,
    session: StdMutex<Option<String>>,
    notifications: Arc<NotificationState>,
    next_id: AtomicI64,
    label: String,
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("url", &self.url)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    pub fn new(
        url: String,
        headers: HashMap<String, String>,
        notifications: Arc<NotificationState>,
        label: String,
    ) -> Result<Self, AdapterError> {
        if url.trim().is_empty() {
            return Err(AdapterError::McpError(
                "HTTP MCP server URL must not be empty".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| AdapterError::McpError(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            client,
            url,
            headers,
            session: StdMutex::new(None),
            notifications,
            next_id: AtomicI64::new(1),
            label,
        })
    }

    fn session(&self) -> Option<String> {
        self.session.lock().expect("session lock").clone()
    }

    pub fn clear_session(&self) {
        *self.session.lock().expect("session lock") = None;
    }

    fn build_post(&self, body: &Value) -> reqwest::RequestBuilder {
        let mut req = self
            .client
            .post(&self.url)
            .header("Accept", "application/json, text/event-stream")
            .json(body);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        if let Some(session) = self.session() {
            req = req.header(MCP_SESSION_HEADER, session);
        }
        req
    }

    /// POST one JSON-RPC request and resolve its response from either a JSON
    /// body or an SSE stream.
    pub async fn request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<HttpOutcome, AdapterError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(RequestId::Number(id), method, params);
        let body = serde_json::to_value(&req)
            .map_err(|e| AdapterError::McpError(format!("failed to encode request: {e}")))?;
        let had_session = self.session().is_some();

        let resp = self.build_post(&body).send().await.map_err(|e| {
            AdapterError::McpError(format!("HTTP request to MCP server failed: {e}"))
        })?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND && had_session {
            debug!(server = %self.label, "MCP session expired (HTTP 404)");
            self.clear_session();
            return Ok(HttpOutcome::SessionExpired);
        }

        // Capture / refresh the session id (the spec sets it on the
        // initialize response; tolerate it on any response).
        if let Some(session) = resp
            .headers()
            .get(MCP_SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
        {
            *self.session.lock().expect("session lock") = Some(session.to_string());
        }

        if !resp.status().is_success() {
            return Err(AdapterError::McpError(format!(
                "MCP server returned HTTP {}",
                resp.status()
            )));
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();

        if content_type.starts_with("text/event-stream") {
            self.resolve_from_sse(resp, id)
                .await
                .map(HttpOutcome::Response)
        } else {
            // Single JSON body mode.
            let raw = resp.text().await.map_err(|e| {
                AdapterError::McpError(format!("failed to read MCP response body: {e}"))
            })?;
            match jsonrpc::parse_inbound(&raw)? {
                InboundMessage::Response(r) => Ok(HttpOutcome::Response(r.into_result())),
                other => Err(AdapterError::McpError(format!(
                    "expected JSON-RPC response, got {}",
                    inbound_kind(&other)
                ))),
            }
        }
    }

    async fn resolve_from_sse(
        &self,
        mut resp: reqwest::Response,
        id: i64,
    ) -> Result<Result<Value, AdapterError>, AdapterError> {
        let mut parser = SseParser::new();
        loop {
            let chunk = resp
                .chunk()
                .await
                .map_err(|e| AdapterError::McpError(format!("MCP SSE stream error: {e}")))?;
            let Some(chunk) = chunk else {
                return Err(AdapterError::McpError(
                    "MCP SSE stream ended without a response for our request".to_string(),
                ));
            };
            for event in parser.feed(&chunk) {
                if let Some(result) = self.handle_sse_event(&event, id) {
                    return Ok(result);
                }
            }
        }
    }

    /// Returns `Some` when the event resolved our pending request.
    fn handle_sse_event(&self, event: &SseEvent, id: i64) -> Option<Result<Value, AdapterError>> {
        match jsonrpc::parse_inbound(&event.data) {
            Ok(InboundMessage::Response(r)) if r.id == Some(RequestId::Number(id)) => {
                Some(r.into_result())
            }
            Ok(InboundMessage::Response(r)) => {
                warn!(server = %self.label, id = ?r.id, "SSE response for foreign request id");
                None
            }
            Ok(InboundMessage::Notification(n)) => {
                handle_notification(&n, &self.notifications, &self.label);
                None
            }
            Ok(InboundMessage::Request(req)) => {
                warn!(
                    server = %self.label,
                    method = %req.method,
                    "ignoring server-initiated MCP request (tools-only client)"
                );
                None
            }
            Err(e) => {
                warn!(server = %self.label, error = %e, "unparseable SSE data payload");
                None
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected; 202 is typical).
    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), AdapterError> {
        let n = JsonRpcNotification::new(method, params);
        let body = serde_json::to_value(&n)
            .map_err(|e| AdapterError::McpError(format!("failed to encode notification: {e}")))?;
        let resp = self.build_post(&body).send().await.map_err(|e| {
            AdapterError::McpError(format!("HTTP request to MCP server failed: {e}"))
        })?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(AdapterError::McpError(format!(
                "MCP server returned HTTP {} for notification",
                resp.status()
            )))
        }
    }
}

fn inbound_kind(msg: &InboundMessage) -> &'static str {
    match msg {
        InboundMessage::Response(_) => "response",
        InboundMessage::Notification(_) => "notification",
        InboundMessage::Request(_) => "request",
    }
}

/// One parsed SSE event (only the fields we care about).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Incremental Server-Sent Events parser. Feed it raw bytes as they arrive;
/// it yields complete events. Handles `\n` and `\r\n` line endings, multi-line
/// `data:` fields, comments, and events split across chunk boundaries.
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    buf: Vec<u8>,
    data: Vec<String>,
    event: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line_bytes: Vec<u8> = self.buf.drain(..=pos).collect();
            line_bytes.pop(); // trailing \n
            if line_bytes.last() == Some(&b'\r') {
                line_bytes.pop();
            }
            let line = String::from_utf8_lossy(&line_bytes).into_owned();
            if let Some(event) = self.consume_line(&line) {
                out.push(event);
            }
        }
        out
    }

    fn consume_line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            // Blank line: dispatch the accumulated event (if any data).
            let event = self.event.take();
            if self.data.is_empty() {
                return None;
            }
            let data = std::mem::take(&mut self.data).join("\n");
            return Some(SseEvent { event, data });
        }
        if line.starts_with(':') {
            return None; // comment / keep-alive
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "data" => self.data.push(value.to_string()),
            "event" => self.event = Some(value.to_string()),
            // "id" and "retry" are irrelevant for our use; ignore the rest.
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(chunks: &[&[u8]]) -> Vec<SseEvent> {
        let mut parser = SseParser::new();
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend(parser.feed(chunk));
        }
        out
    }

    #[test]
    fn parses_single_event() {
        let events = collect(&[b"data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n"]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
        assert_eq!(events[0].event, None);
    }

    #[test]
    fn parses_event_split_across_chunks() {
        let events = collect(&[
            b"data: {\"jsonrpc\":\"2.0\",",
            b"\"id\":7,\"result\":{}}\n",
            b"\n",
        ]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, r#"{"jsonrpc":"2.0","id":7,"result":{}}"#);
    }

    #[test]
    fn parses_multiple_events_and_event_field() {
        let events = collect(&[b"event: message\ndata: one\n\ndata: two\n\n"]);
        assert_eq!(
            events,
            vec![
                SseEvent {
                    event: Some("message".to_string()),
                    data: "one".to_string()
                },
                SseEvent {
                    event: None,
                    data: "two".to_string()
                },
            ]
        );
    }

    #[test]
    fn joins_multi_line_data_with_newline() {
        let events = collect(&[b"data: line1\ndata: line2\n\n"]);
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[test]
    fn handles_crlf_and_comments_and_id_field() {
        let events = collect(&[b": keep-alive\r\nid: 4\r\ndata: hello\r\n\r\n"]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn blank_lines_without_data_emit_nothing() {
        let events = collect(&[b"\n\n: ping\n\n"]);
        assert!(events.is_empty());
    }

    #[test]
    fn data_without_space_after_colon() {
        let events = collect(&[b"data:{\"x\":1}\n\n"]);
        assert_eq!(events[0].data, r#"{"x":1}"#);
    }
}
