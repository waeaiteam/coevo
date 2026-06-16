//! JSON-RPC 2.0 message types and (de)serialization helpers used by the MCP client.

use crate::traits::AdapterError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSONRPC_VERSION: &str = "2.0";

/// JSON-RPC request/response id. We always generate numeric ids, but servers
/// are allowed to echo back string ids, so both are supported when parsing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::Number(n) => write!(f, "{n}"),
            RequestId::String(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: RequestId, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn into_adapter_error(self) -> AdapterError {
        AdapterError::McpRpc {
            code: self.code,
            message: self.message,
            data: self.data,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    /// `null` id is allowed for protocol-level errors.
    pub id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Map a JSON-RPC response to `Ok(result)` or a typed `AdapterError`.
    pub fn into_result(self) -> Result<Value, AdapterError> {
        if let Some(err) = self.error {
            return Err(err.into_adapter_error());
        }
        self.result.ok_or_else(|| {
            AdapterError::McpError("JSON-RPC response carried neither result nor error".to_string())
        })
    }
}

/// Any message a server can send us.
#[derive(Debug, Clone)]
pub enum InboundMessage {
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
    /// Server-initiated request (sampling, roots, ...). We are a tools-only
    /// client and do not service these.
    Request(JsonRpcRequest),
}

/// Parse one raw JSON-RPC message (a single line / SSE data payload).
pub fn parse_inbound(raw: &str) -> Result<InboundMessage, AdapterError> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|e| AdapterError::McpError(format!("invalid JSON-RPC message: {e}")))?;
    classify_inbound(value)
}

pub fn classify_inbound(value: Value) -> Result<InboundMessage, AdapterError> {
    let obj = value
        .as_object()
        .ok_or_else(|| AdapterError::McpError("JSON-RPC message is not an object".to_string()))?;
    let parse_err =
        |e: serde_json::Error| AdapterError::McpError(format!("malformed JSON-RPC message: {e}"));
    if obj.contains_key("method") {
        if obj.get("id").map(|id| !id.is_null()).unwrap_or(false) {
            Ok(InboundMessage::Request(
                serde_json::from_value(value).map_err(parse_err)?,
            ))
        } else {
            Ok(InboundMessage::Notification(
                serde_json::from_value(value).map_err(parse_err)?,
            ))
        }
    } else {
        Ok(InboundMessage::Response(
            serde_json::from_value(value).map_err(parse_err)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trip() {
        let req = JsonRpcRequest::new(
            RequestId::Number(7),
            "tools/list",
            Some(json!({"cursor": "abc"})),
        );
        let line = serde_json::to_string(&req).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed.jsonrpc, "2.0");
        assert_eq!(parsed.id, RequestId::Number(7));
        assert_eq!(parsed.method, "tools/list");
        assert_eq!(parsed.params, Some(json!({"cursor": "abc"})));
    }

    #[test]
    fn request_without_params_omits_field() {
        let req = JsonRpcRequest::new(RequestId::Number(1), "ping", None);
        let line = serde_json::to_string(&req).unwrap();
        assert!(!line.contains("params"));
    }

    #[test]
    fn notification_round_trip() {
        let n = JsonRpcNotification::new("notifications/initialized", None);
        let line = serde_json::to_string(&n).unwrap();
        match parse_inbound(&line).unwrap() {
            InboundMessage::Notification(parsed) => {
                assert_eq!(parsed.method, "notifications/initialized");
            }
            other => panic!("expected notification, got {other:?}"),
        }
    }

    #[test]
    fn response_with_result_classifies_and_unwraps() {
        let raw = r#"{"jsonrpc":"2.0","id":3,"result":{"ok":true}}"#;
        match parse_inbound(raw).unwrap() {
            InboundMessage::Response(resp) => {
                assert_eq!(resp.id, Some(RequestId::Number(3)));
                assert_eq!(resp.into_result().unwrap(), json!({"ok": true}));
            }
            other => panic!("expected response, got {other:?}"),
        }
    }

    #[test]
    fn response_with_string_id_parses() {
        let raw = r#"{"jsonrpc":"2.0","id":"abc","result":{}}"#;
        match parse_inbound(raw).unwrap() {
            InboundMessage::Response(resp) => {
                assert_eq!(resp.id, Some(RequestId::String("abc".to_string())));
            }
            other => panic!("expected response, got {other:?}"),
        }
    }

    #[test]
    fn error_response_maps_to_typed_adapter_error() {
        let raw = r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32601,"message":"method not found","data":{"method":"x"}}}"#;
        let InboundMessage::Response(resp) = parse_inbound(raw).unwrap() else {
            panic!("expected response");
        };
        match resp.into_result() {
            Err(AdapterError::McpRpc {
                code,
                message,
                data,
            }) => {
                assert_eq!(code, -32601);
                assert_eq!(message, "method not found");
                assert_eq!(data, Some(json!({"method": "x"})));
            }
            other => panic!("expected McpRpc error, got {other:?}"),
        }
    }

    #[test]
    fn server_request_classifies_as_request() {
        let raw = r#"{"jsonrpc":"2.0","id":9,"method":"sampling/createMessage","params":{}}"#;
        assert!(matches!(
            parse_inbound(raw).unwrap(),
            InboundMessage::Request(_)
        ));
    }

    #[test]
    fn garbage_line_is_an_error() {
        assert!(parse_inbound("not json").is_err());
        assert!(parse_inbound("[1,2,3]").is_err());
    }
}
