//! Public data types of the MCP client (tools, content, server info).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool advertised by an MCP server via `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema describing the tool's arguments (`inputSchema` per spec).
    pub input_schema: Value,
}

/// One content block of a `tools/call` result, per the MCP spec.
/// Text is fully typed; image/resource keep their payload available; anything
/// unknown (e.g. audio) is carried through as the raw JSON value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource { resource: Value },
    #[serde(untagged)]
    Other(Value),
}

impl McpContent {
    /// Text of this block, if it is a text block.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            McpContent::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// Result of a `tools/call` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolOutput {
    pub content: Vec<McpContent>,
    /// True when the tool itself reported failure (`isError`), as opposed to
    /// a protocol error (which surfaces as `AdapterError`).
    pub is_error: bool,
    /// `structuredContent` from the server, when provided.
    pub structured: Option<Value>,
}

impl McpToolOutput {
    pub(crate) fn from_result(result: Value) -> Self {
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let structured = result.get("structuredContent").cloned();
        let content = result
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .map(|block| {
                        serde_json::from_value(block.clone())
                            .unwrap_or_else(|_| McpContent::Other(block.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            content,
            is_error,
            structured,
        }
    }

    /// Concatenated text of all text content blocks.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(McpContent::as_text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Negotiated identity and capabilities of a connected MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Registry id (`McpServerConfig::id`).
    pub id: String,
    /// Display name from the local config (`McpServerConfig::name`).
    pub name: String,
    /// Protocol version agreed during `initialize`.
    pub protocol_version: String,
    /// `serverInfo.name` reported by the server.
    pub server_name: String,
    /// `serverInfo.version` reported by the server.
    pub server_version: String,
    /// Raw `capabilities` object reported by the server.
    pub capabilities: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_output_parses_text_image_resource_and_unknown() {
        let out = McpToolOutput::from_result(json!({
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "image", "data": "aGk=", "mimeType": "image/png"},
                {"type": "resource", "resource": {"uri": "file:///x", "text": "y"}},
                {"type": "audio", "data": "...", "mimeType": "audio/wav"}
            ],
            "isError": false,
            "structuredContent": {"answer": 42}
        }));
        assert!(!out.is_error);
        assert_eq!(out.structured, Some(json!({"answer": 42})));
        assert_eq!(out.content.len(), 4);
        assert_eq!(out.text(), "hello");
        assert!(matches!(out.content[1], McpContent::Image { .. }));
        assert!(matches!(out.content[2], McpContent::Resource { .. }));
        assert!(matches!(out.content[3], McpContent::Other(_)));
    }

    #[test]
    fn tool_output_defaults_when_fields_missing() {
        let out = McpToolOutput::from_result(json!({}));
        assert!(!out.is_error);
        assert!(out.content.is_empty());
        assert!(out.structured.is_none());
    }
}
