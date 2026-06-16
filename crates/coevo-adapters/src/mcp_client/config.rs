//! Configuration for connecting to MCP servers.
//!
//! [`McpServerConfig`] is the persisted, serializable description of a single
//! MCP server. Its fields mirror the `mcp_servers` table the store layer owns:
//!
//! | struct field | column      | notes                                        |
//! |--------------|-------------|----------------------------------------------|
//! | `id`         | `id`        | stable registry id (uuid/text) — primary key |
//! | `name`       | `name`      | short routing name, must be URN-safe         |
//! | `transport`  | `transport` | `'stdio'` or `'http'`                         |
//! | (stdio)      | `command`   | executable                                   |
//! | (stdio)      | `args`      | JSON array of strings                        |
//! | (stdio)      | `env`       | JSON object string->string                   |
//! | (http)       | `url`       | base URL for JSON-RPC POSTs                  |
//! | (http)       | `headers`   | JSON object string->string (secrets)         |
//!
//! `args`/`env`/`headers` are stored as JSON-encoded text columns. The
//! [`McpServerConfig::from_row`] / [`McpServerConfig::to_row`] helpers convert
//! between the flat [`McpServerRow`] (what the store layer reads/writes) and
//! this struct so the store never has to know the transport rules.
//!
//! [`McpServerConfig`]'s `Debug` impl deliberately **redacts** header and env
//! *values* (they carry tokens/secrets) while keeping keys, command, args, and
//! url visible for diagnostics. Never log a raw header/env map elsewhere.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Transport kind discriminator, matching the `transport` column values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    Stdio,
    Http,
}

impl TransportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TransportKind::Stdio => "stdio",
            TransportKind::Http => "http",
        }
    }
}

/// Transport-specific connection parameters.
///
/// Serialized with an internal `transport` tag so a [`McpServerConfig`]
/// flattens to `{ "transport": "stdio", "command": ... }` etc.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub enum McpTransportConfig {
    /// Spawn a child process and speak line-delimited JSON-RPC over its
    /// stdin/stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    /// POST JSON-RPC to a base URL (streamable HTTP transport).
    Http {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
}

impl McpTransportConfig {
    pub fn kind(&self) -> TransportKind {
        match self {
            McpTransportConfig::Stdio { .. } => TransportKind::Stdio,
            McpTransportConfig::Http { .. } => TransportKind::Http,
        }
    }
}

/// Redacting Debug: show structure and keys, never secret values.
impl std::fmt::Debug for McpTransportConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpTransportConfig::Stdio { command, args, env } => f
                .debug_struct("Stdio")
                .field("command", command)
                .field("args", args)
                .field("env_keys", &env.keys().collect::<Vec<_>>())
                .finish(),
            McpTransportConfig::Http { url, headers } => f
                .debug_struct("Http")
                .field("url", url)
                .field("header_names", &headers.keys().collect::<Vec<_>>())
                .finish(),
        }
    }
}

/// A fully-described MCP server the client can connect to.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Stable registry id (primary key in `mcp_servers`). Used to key the
    /// [`super::McpClientManager`].
    pub id: String,
    /// Short, URN-safe routing name. Used to build fully-qualified tool ids
    /// (`urn:mcp:{name}:{tool}`). Must be unique across configured servers and
    /// free of `:` and whitespace.
    pub name: String,
    /// Transport + its parameters.
    #[serde(flatten)]
    pub transport: McpTransportConfig,
}

/// Redacting Debug (delegates to [`McpTransportConfig`]'s redacting impl).
impl std::fmt::Debug for McpServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServerConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("transport", &self.transport)
            .finish()
    }
}

impl McpServerConfig {
    /// Build a stdio server config.
    pub fn stdio(
        id: impl Into<String>,
        name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            transport: McpTransportConfig::Stdio {
                command: command.into(),
                args,
                env: HashMap::new(),
            },
        }
    }

    /// Build an HTTP server config.
    pub fn http(id: impl Into<String>, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            transport: McpTransportConfig::Http {
                url: url.into(),
                headers: HashMap::new(),
            },
        }
    }

    pub fn transport_kind(&self) -> TransportKind {
        self.transport.kind()
    }

    /// Validate the config. The routing `name` becomes part of a
    /// `urn:mcp:{name}:{tool}` id, so it must be non-empty and free of `:` and
    /// whitespace; the transport must have its required field.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("server name must not be empty".to_string());
        }
        if self.name.contains(':') || self.name.chars().any(char::is_whitespace) {
            return Err(format!(
                "server name '{}' must not contain ':' or whitespace (it is used in tool URNs)",
                self.name
            ));
        }
        match &self.transport {
            McpTransportConfig::Stdio { command, .. } if command.trim().is_empty() => {
                Err("stdio transport requires a non-empty command".to_string())
            }
            McpTransportConfig::Http { url, .. } if url.trim().is_empty() => {
                Err("http transport requires a non-empty url".to_string())
            }
            _ => Ok(()),
        }
    }

    /// Build a config from a flat database row. `args`, `env`, and `headers`
    /// are the raw JSON text columns (nullable -> `None`/empty).
    pub fn from_row(row: McpServerRow) -> Result<Self, String> {
        let transport = match row.transport.as_str() {
            "stdio" => {
                let command = row
                    .command
                    .ok_or_else(|| "stdio row missing 'command'".to_string())?;
                let args = parse_json_array(row.args.as_deref())?;
                let env = parse_json_map(row.env.as_deref())?;
                McpTransportConfig::Stdio { command, args, env }
            }
            "http" => {
                let url = row
                    .url
                    .ok_or_else(|| "http row missing 'url'".to_string())?;
                let headers = parse_json_map(row.headers.as_deref())?;
                McpTransportConfig::Http { url, headers }
            }
            other => return Err(format!("unknown transport '{other}'")),
        };
        let cfg = Self {
            id: row.id,
            name: row.name,
            transport,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Flatten this config into a database row. JSON columns are `None` when
    /// they would be empty / not applicable for the transport.
    pub fn to_row(&self) -> McpServerRow {
        let mut row = McpServerRow {
            id: self.id.clone(),
            name: self.name.clone(),
            transport: self.transport.kind().as_str().to_string(),
            command: None,
            args: None,
            env: None,
            url: None,
            headers: None,
        };
        match &self.transport {
            McpTransportConfig::Stdio { command, args, env } => {
                row.command = Some(command.clone());
                if !args.is_empty() {
                    row.args = Some(serde_json::to_string(args).unwrap_or_else(|_| "[]".into()));
                }
                if !env.is_empty() {
                    row.env = Some(json_map_to_string(env));
                }
            }
            McpTransportConfig::Http { url, headers } => {
                row.url = Some(url.clone());
                if !headers.is_empty() {
                    row.headers = Some(json_map_to_string(headers));
                }
            }
        }
        row
    }
}

/// Flat row representation of the `mcp_servers` table. The store layer maps its
/// SQLite row to/from this; the client maps this to/from [`McpServerConfig`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct McpServerRow {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    /// JSON-encoded array of strings.
    pub args: Option<String>,
    /// JSON-encoded object string->string.
    pub env: Option<String>,
    pub url: Option<String>,
    /// JSON-encoded object string->string.
    pub headers: Option<String>,
}

fn parse_json_array(raw: Option<&str>) -> Result<Vec<String>, String> {
    match raw {
        None => Ok(Vec::new()),
        Some(s) if s.trim().is_empty() => Ok(Vec::new()),
        Some(s) => serde_json::from_str(s).map_err(|e| format!("invalid args JSON: {e}")),
    }
}

fn parse_json_map(raw: Option<&str>) -> Result<HashMap<String, String>, String> {
    match raw {
        None => Ok(HashMap::new()),
        Some(s) if s.trim().is_empty() => Ok(HashMap::new()),
        Some(s) => serde_json::from_str(s).map_err(|e| format!("invalid map JSON: {e}")),
    }
}

/// Serialize a string map deterministically (sorted keys) so `to_row` output is
/// stable across runs — handy for change detection and tests.
fn json_map_to_string(map: &HashMap<String, String>) -> String {
    let sorted: std::collections::BTreeMap<&String, &String> = map.iter().collect();
    serde_json::to_string(&sorted).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stdio_round_trips_through_row() {
        let mut env = HashMap::new();
        env.insert("TOKEN".to_string(), "abc".to_string());
        let cfg = McpServerConfig {
            id: "srv-1".into(),
            name: "files".into(),
            transport: McpTransportConfig::Stdio {
                command: "python".into(),
                args: vec!["-m".into(), "server".into()],
                env,
            },
        };
        let row = cfg.to_row();
        assert_eq!(row.transport, "stdio");
        assert_eq!(row.command.as_deref(), Some("python"));
        assert!(row.url.is_none());
        let back = McpServerConfig::from_row(row).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn http_round_trips_through_row() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer x".to_string());
        let cfg = McpServerConfig {
            id: "srv-2".into(),
            name: "weather".into(),
            transport: McpTransportConfig::Http {
                url: "https://example.com/mcp".into(),
                headers,
            },
        };
        let row = cfg.to_row();
        assert_eq!(row.transport, "http");
        assert_eq!(row.url.as_deref(), Some("https://example.com/mcp"));
        assert!(row.command.is_none());
        let back = McpServerConfig::from_row(row).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn empty_args_and_env_are_null_columns() {
        let cfg = McpServerConfig::stdio("id", "name", "cmd", vec![]);
        let row = cfg.to_row();
        assert!(row.args.is_none());
        assert!(row.env.is_none());
    }

    #[test]
    fn validate_rejects_colon_in_name() {
        let cfg = McpServerConfig::stdio("id", "bad:name", "cmd", vec![]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_whitespace_in_name() {
        let cfg = McpServerConfig::http("id", "bad name", "http://x");
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_empty_command() {
        let cfg = McpServerConfig::stdio("id", "name", "   ", vec![]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn from_row_rejects_unknown_transport() {
        let row = McpServerRow {
            id: "x".into(),
            name: "n".into(),
            transport: "carrier-pigeon".into(),
            ..Default::default()
        };
        assert!(McpServerConfig::from_row(row).is_err());
    }

    #[test]
    fn config_serde_flattens_transport_tag() {
        let cfg = McpServerConfig::http("id", "name", "http://x");
        let v = serde_json::to_value(&cfg).unwrap();
        assert_eq!(v["transport"], json!("http"));
        assert_eq!(v["url"], json!("http://x"));
    }

    #[test]
    fn config_deserializes_from_flat_json() {
        let v = json!({
            "id": "s",
            "name": "files",
            "transport": "stdio",
            "command": "node",
            "args": ["server.js"]
        });
        let cfg: McpServerConfig = serde_json::from_value(v).unwrap();
        assert_eq!(cfg.id, "s");
        match cfg.transport {
            McpTransportConfig::Stdio { command, args, .. } => {
                assert_eq!(command, "node");
                assert_eq!(args, vec!["server.js"]);
            }
            _ => panic!("expected stdio"),
        }
    }
}
