//! Integration tests for the MCP client against an in-memory fake MCP server
//! speaking newline-delimited JSON-RPC over a `tokio::io::duplex` stream.
//! Deterministic: no network, no child processes.

use super::client::{PROTOCOL_VERSION_FALLBACK, PROTOCOL_VERSION_LATEST};
use super::*;
use crate::traits::{AdapterError, McpToolCall};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Default, Clone, Copy)]
struct FakeServerOptions {
    /// Reject any protocolVersion except 2024-11-05 with a version-mismatch error.
    legacy_only: bool,
    /// Emit `notifications/tools/list_changed` right before answering an
    /// `echo` tools/call.
    notify_list_changed_before_echo: bool,
}

/// Spawn the fake server; returns the client and a log of methods it received.
fn start(opts: FakeServerOptions) -> (McpClient, Arc<StdMutex<Vec<String>>>) {
    let (client_side, server_side) = tokio::io::duplex(64 * 1024);
    let seen = Arc::new(StdMutex::new(Vec::new()));
    tokio::spawn(run_fake_server(server_side, opts, Arc::clone(&seen)));
    let (read, write) = tokio::io::split(client_side);
    let client = McpClient::over_stream("test-id", "test server", read, write);
    (client, seen)
}

async fn run_fake_server(
    stream: tokio::io::DuplexStream,
    opts: FakeServerOptions,
    seen: Arc<StdMutex<Vec<String>>>,
) {
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let msg: Value = serde_json::from_str(&line).expect("fake server got invalid JSON");
        let method = msg["method"].as_str().unwrap_or_default().to_string();
        seen.lock().unwrap().push(method.clone());
        let Some(id) = msg.get("id").cloned() else {
            continue; // notification — nothing to answer
        };
        let response = match method.as_str() {
            "initialize" => {
                let requested = msg["params"]["protocolVersion"]
                    .as_str()
                    .unwrap_or_default();
                if opts.legacy_only && requested != PROTOCOL_VERSION_FALLBACK {
                    json!({"jsonrpc": "2.0", "id": id, "error": {
                        "code": -32602,
                        "message": "Unsupported protocol version",
                        "data": {"supported": [PROTOCOL_VERSION_FALLBACK]}
                    }})
                } else {
                    json!({"jsonrpc": "2.0", "id": id, "result": {
                        "protocolVersion": requested,
                        "capabilities": {"tools": {"listChanged": true}},
                        "serverInfo": {"name": "fake-mcp", "version": "0.1.0"}
                    }})
                }
            }
            "tools/list" => match msg["params"]["cursor"].as_str() {
                None => json!({"jsonrpc": "2.0", "id": id, "result": {
                    "tools": [{
                        "name": "echo",
                        "description": "echoes its arguments",
                        "inputSchema": {"type": "object"}
                    }],
                    "nextCursor": "page-2"
                }}),
                Some("page-2") => json!({"jsonrpc": "2.0", "id": id, "result": {
                    "tools": [{"name": "fail"}]
                }}),
                Some(other) => json!({"jsonrpc": "2.0", "id": id, "error": {
                    "code": -32602, "message": format!("bad cursor {other}")
                }}),
            },
            "tools/call" => {
                let name = msg["params"]["name"].as_str().unwrap_or_default();
                match name {
                    "echo" => {
                        if opts.notify_list_changed_before_echo {
                            let n = json!({
                                "jsonrpc": "2.0",
                                "method": "notifications/tools/list_changed"
                            });
                            send_line(&mut write, &n).await;
                        }
                        let args = msg["params"]["arguments"].clone();
                        json!({"jsonrpc": "2.0", "id": id, "result": {
                            "content": [{"type": "text", "text": args.to_string()}],
                            "isError": false,
                            "structuredContent": {"echoed": args}
                        }})
                    }
                    "fail" => json!({"jsonrpc": "2.0", "id": id, "result": {
                        "content": [{"type": "text", "text": "boom"}],
                        "isError": true
                    }}),
                    "rpc-error" => json!({"jsonrpc": "2.0", "id": id, "error": {
                        "code": -32000, "message": "tool exploded"
                    }}),
                    "sleep" => continue, // never answers: exercises the timeout
                    other => json!({"jsonrpc": "2.0", "id": id, "error": {
                        "code": -32602, "message": format!("unknown tool {other}")
                    }}),
                }
            }
            other => json!({"jsonrpc": "2.0", "id": id, "error": {
                "code": -32601, "message": format!("method not found: {other}")
            }}),
        };
        send_line(&mut write, &response).await;
    }
}

async fn send_line<W: tokio::io::AsyncWrite + Unpin>(write: &mut W, msg: &Value) {
    let mut line = serde_json::to_vec(msg).unwrap();
    line.push(b'\n');
    write.write_all(&line).await.unwrap();
    write.flush().await.unwrap();
}

#[tokio::test]
async fn initialize_handshake_negotiates_latest_version() {
    let (client, seen) = start(FakeServerOptions::default());
    let info = client.initialize().await.unwrap();
    assert_eq!(info.protocol_version, PROTOCOL_VERSION_LATEST);
    assert_eq!(info.server_name, "fake-mcp");
    assert_eq!(info.server_version, "0.1.0");
    assert_eq!(info.capabilities["tools"]["listChanged"], json!(true));
    assert_eq!(info.id, "test-id");
    assert_eq!(info.name, "test server");

    // A follow-up request guarantees the server has consumed everything
    // written before it, including the initialized notification.
    client.list_tools().await.unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0], "initialize");
    assert_eq!(seen[1], "notifications/initialized");
}

#[tokio::test]
async fn initialize_falls_back_on_version_mismatch() {
    let (client, seen) = start(FakeServerOptions {
        legacy_only: true,
        ..Default::default()
    });
    let info = client.initialize().await.unwrap();
    assert_eq!(info.protocol_version, PROTOCOL_VERSION_FALLBACK);
    client.list_tools().await.unwrap();
    let seen = seen.lock().unwrap().clone();
    // Two initialize attempts, then the initialized notification.
    assert_eq!(
        &seen[..3],
        ["initialize", "initialize", "notifications/initialized"]
    );
}

#[tokio::test]
async fn list_tools_follows_cursor_pagination() {
    let (client, _) = start(FakeServerOptions::default());
    client.initialize().await.unwrap();
    let tools = client.list_tools().await.unwrap();
    assert_eq!(
        tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["echo", "fail"]
    );
    assert_eq!(
        tools[0].description.as_deref(),
        Some("echoes its arguments")
    );
    assert_eq!(tools[0].input_schema, json!({"type": "object"}));
    assert_eq!(tools[1].description, None);
}

#[tokio::test]
async fn call_tool_success_returns_text_and_structured_content() {
    let (client, _) = start(FakeServerOptions::default());
    client.initialize().await.unwrap();
    let out = client
        .call_tool("echo", json!({"x": 1}), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(!out.is_error);
    assert_eq!(out.text(), r#"{"x":1}"#);
    assert_eq!(out.structured, Some(json!({"echoed": {"x": 1}})));
}

#[tokio::test]
async fn call_tool_propagates_is_error() {
    let (client, _) = start(FakeServerOptions::default());
    client.initialize().await.unwrap();
    let out = client
        .call_tool("fail", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(out.is_error);
    assert_eq!(out.text(), "boom");
}

#[tokio::test]
async fn call_tool_maps_rpc_error_to_typed_variant() {
    let (client, _) = start(FakeServerOptions::default());
    client.initialize().await.unwrap();
    let err = client
        .call_tool("rpc-error", json!({}), Duration::from_secs(5))
        .await
        .unwrap_err();
    match err {
        AdapterError::McpRpc { code, message, .. } => {
            assert_eq!(code, -32000);
            assert_eq!(message, "tool exploded");
        }
        other => panic!("expected McpRpc, got {other:?}"),
    }
}

#[tokio::test]
async fn call_tool_times_out() {
    let (client, _) = start(FakeServerOptions::default());
    client.initialize().await.unwrap();
    let err = client
        .call_tool("sleep", json!({}), Duration::from_millis(100))
        .await
        .unwrap_err();
    assert!(matches!(err, AdapterError::Timeout));
    // The connection stays usable after a timeout.
    let out = client
        .call_tool("echo", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(!out.is_error);
}

#[tokio::test]
async fn tools_list_changed_notification_sets_dirty_flag() {
    let (client, _) = start(FakeServerOptions {
        notify_list_changed_before_echo: true,
        ..Default::default()
    });
    client.initialize().await.unwrap();
    assert!(!client.tools_changed());
    // The fake server sends the notification before the echo response, so the
    // reader is guaranteed to have processed it once the call returns.
    client
        .call_tool("echo", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(client.tools_changed());
    // list_tools clears the flag.
    client.list_tools().await.unwrap();
    assert!(!client.tools_changed());
}

#[tokio::test]
async fn dead_connection_reports_unavailable_and_not_alive() {
    let (client_side, server_side) = tokio::io::duplex(1024);
    drop(server_side); // server gone before we even start
    let (read, write) = tokio::io::split(client_side);
    let client = McpClient::over_stream("dead", "dead", read, write);
    let err = client.initialize().await.unwrap_err();
    assert!(matches!(err, AdapterError::Unavailable));
    // The background reader observes EOF asynchronously; poll briefly.
    for _ in 0..100 {
        if !client.is_alive().await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(!client.is_alive().await);
}

#[tokio::test]
async fn manager_rejects_unknown_ids_and_lists_nothing() {
    let manager = McpClientManager::new();
    assert!(manager.connected_ids().await.is_empty());
    assert!(!manager.is_connected("nope").await);
    assert!(manager.list_tools("nope").await.is_err());
    assert!(manager
        .call_tool("nope", "echo", json!({}), Duration::from_secs(1))
        .await
        .is_err());
    assert!(manager.disconnect("nope").await.is_err());
    assert!(manager.server_info("nope").await.is_none());
}

#[tokio::test]
async fn manager_rejects_empty_stdio_command() {
    let manager = McpClientManager::new();
    let err = manager
        .connect(McpServerConfig {
            id: "bad".to_string(),
            name: "bad".to_string(),
            transport: McpTransportConfig::Stdio {
                command: "   ".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
        })
        .await
        .unwrap_err();
    assert!(matches!(err, AdapterError::McpError(_)));
    assert!(manager.connected_ids().await.is_empty());
}

#[test]
fn config_debug_redacts_header_and_env_values() {
    let http = McpServerConfig {
        id: "h".to_string(),
        name: "h".to_string(),
        transport: McpTransportConfig::Http {
            url: "https://example.com/mcp".to_string(),
            headers: HashMap::from([(
                "Authorization".to_string(),
                "Bearer super-secret-token".to_string(),
            )]),
        },
    };
    let rendered = format!("{http:?}");
    assert!(rendered.contains("Authorization"));
    assert!(!rendered.contains("super-secret-token"));

    let stdio = McpServerConfig {
        id: "s".to_string(),
        name: "s".to_string(),
        transport: McpTransportConfig::Stdio {
            command: "my-server".to_string(),
            args: vec!["--flag".to_string()],
            env: HashMap::from([("API_KEY".to_string(), "hunter2".to_string())]),
        },
    };
    let rendered = format!("{stdio:?}");
    assert!(rendered.contains("API_KEY"));
    assert!(rendered.contains("my-server"));
    assert!(!rendered.contains("hunter2"));
}

// ---- URN scheme + integrity hash (pure) ----

#[test]
fn urn_round_trip() {
    let urn = make_tool_urn("files", "read");
    assert_eq!(urn, "urn:mcp:files:read");
    let (s, t) = parse_tool_urn(&urn).unwrap();
    assert_eq!(s, "files");
    assert_eq!(t, "read");
}

#[test]
fn urn_tool_name_may_contain_colons() {
    let urn = make_tool_urn("srv", "ns:sub:tool");
    let (s, t) = parse_tool_urn(&urn).unwrap();
    assert_eq!(s, "srv");
    assert_eq!(t, "ns:sub:tool");
}

#[test]
fn urn_rejects_bad_input() {
    assert!(parse_tool_urn("files:read").is_err());
    assert!(parse_tool_urn("urn:mcp:noseparator").is_err());
    assert!(parse_tool_urn("urn:mcp::tool").is_err());
    assert!(parse_tool_urn("urn:mcp:server:").is_err());
}

#[test]
fn integrity_hash_is_stable_and_prefixed() {
    let v = json!({"a": 1, "b": [2, 3]});
    let h1 = integrity_hash(&v);
    let h2 = integrity_hash(&v);
    assert_eq!(h1, h2);
    assert!(h1.starts_with("sha256:"));
    assert_ne!(integrity_hash(&json!({"a": 1, "b": [2, 4]})), h1);
}

// ---- RealMcpClient end-to-end over the in-memory fake server ----

/// Build a `RealMcpClient` whose manager holds the in-memory fake server under
/// the given id/name (no child process, no network).
async fn real_client_with_fake(id: &str, name: &str) -> RealMcpClient {
    let (client_side, server_side) = tokio::io::duplex(64 * 1024);
    let seen = Arc::new(StdMutex::new(Vec::new()));
    tokio::spawn(run_fake_server(
        server_side,
        FakeServerOptions::default(),
        seen,
    ));
    let (read, write) = tokio::io::split(client_side);
    let client = McpClient::over_stream(id, name, read, write);
    client.initialize().await.unwrap();

    let manager = McpClientManager::new();
    manager.insert_client(id, client).await;
    RealMcpClient::from_manager(manager).await
}

#[tokio::test]
async fn real_client_lists_fully_qualified_urns() {
    let real = real_client_with_fake("srv-id", "files").await;
    let urns = real.list_tools().await.unwrap();
    assert!(urns.contains(&"urn:mcp:files:echo".to_string()));
    assert!(urns.contains(&"urn:mcp:files:fail".to_string()));
}

#[tokio::test]
async fn real_client_routes_call_by_urn_and_hashes_result() {
    let real = real_client_with_fake("srv-id", "files").await;
    let result = real
        .call_tool(McpToolCall {
            tool_urn: "urn:mcp:files:echo".to_string(),
            parameters: json!({"x": 1}),
            traceparent: "00-a-b-01".to_string(),
        })
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.result["isError"], json!(false));
    assert_eq!(result.result["text"], json!(r#"{"x":1}"#));
    let sig = result.verification_signature.clone().unwrap();
    assert!(sig.starts_with("sha256:"));
    // verify_result accepts the untampered result...
    assert!(real.verify_result(&result).await.unwrap());
}

#[tokio::test]
async fn real_client_call_propagates_is_error() {
    let real = real_client_with_fake("srv-id", "files").await;
    let result = real
        .call_tool(McpToolCall {
            tool_urn: "urn:mcp:files:fail".to_string(),
            parameters: json!({}),
            traceparent: "00-a-b-01".to_string(),
        })
        .await
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.result["isError"], json!(true));
    assert!(real.verify_result(&result).await.unwrap());
}

#[tokio::test]
async fn real_client_unknown_server_name_errors() {
    let real = real_client_with_fake("srv-id", "files").await;
    let err = real
        .call_tool(McpToolCall {
            tool_urn: "urn:mcp:ghost:echo".to_string(),
            parameters: json!({}),
            traceparent: "00-a-b-01".to_string(),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, AdapterError::McpError(_)));
}

#[tokio::test]
async fn verify_result_rejects_tampered_digest() {
    let real = real_client_with_fake("srv-id", "files").await;
    let mut result = real
        .call_tool(McpToolCall {
            tool_urn: "urn:mcp:files:echo".to_string(),
            parameters: json!({"x": 1}),
            traceparent: "00-a-b-01".to_string(),
        })
        .await
        .unwrap();
    // Mutate the payload without updating the digest.
    result.result["text"] = json!("tampered");
    assert!(!real.verify_result(&result).await.unwrap());
}

#[tokio::test]
async fn verify_result_rejects_success_iserror_mismatch() {
    let real = real_client_with_fake("srv-id", "files").await;
    let mut result = real
        .call_tool(McpToolCall {
            tool_urn: "urn:mcp:files:fail".to_string(),
            parameters: json!({}),
            traceparent: "00-a-b-01".to_string(),
        })
        .await
        .unwrap();
    // Claim success while isError=true, re-hashing so the digest itself matches.
    result.success = true;
    result.verification_signature = Some(integrity_hash(&result.result));
    assert!(!real.verify_result(&result).await.unwrap());
}

// ---------------------------------------------------------------------------
// URN routing / RealMcpClient (McpProvider wiring) / integrity hash
// ---------------------------------------------------------------------------

#[test]
fn tool_urn_round_trip_and_errors() {
    let urn = make_tool_urn("files", "read_file");
    assert_eq!(urn, "urn:mcp:files:read_file");
    assert_eq!(parse_tool_urn(&urn).unwrap(), ("files", "read_file"));
    // Tool names may themselves contain ':' — split only once.
    assert_eq!(
        parse_tool_urn("urn:mcp:srv:ns:tool").unwrap(),
        ("srv", "ns:tool")
    );
    assert!(parse_tool_urn("urn:other:x:y").is_err());
    assert!(parse_tool_urn("urn:mcp:no-tool").is_err());
    assert!(parse_tool_urn("urn:mcp::tool").is_err());
}

#[test]
fn integrity_hash_is_deterministic_and_input_sensitive() {
    let a = integrity_hash(&json!({"b": 2, "a": 1}));
    let b = integrity_hash(&json!({"a": 1, "b": 2}));
    assert_eq!(a, b, "key order must not matter");
    assert!(a.starts_with("sha256:"));
    assert_ne!(a, integrity_hash(&json!({"a": 1, "b": 3})));
}

#[tokio::test]
async fn real_mcp_client_routes_urns_and_verifies_results() {
    use crate::traits::{McpProvider, McpToolCall};

    let (client, _) = start(FakeServerOptions::default());
    client.initialize().await.unwrap();
    let manager = McpClientManager::new();
    manager.insert_client("srv-1", client).await;
    // from_manager rebuilds the name->id index from connected clients
    // (the over_stream client was registered with name "test server" —
    // whitespace is fine here because the index uses the negotiated name).
    let real = RealMcpClient::from_manager(manager).await;

    let urns = McpProvider::list_tools(&real).await.unwrap();
    assert_eq!(
        urns,
        vec!["urn:mcp:test server:echo", "urn:mcp:test server:fail"]
    );

    let result = McpProvider::call_tool(
        &real,
        McpToolCall {
            tool_urn: "urn:mcp:test server:echo".into(),
            parameters: json!({"q": 7}),
            traceparent: "00-abc-def-01".into(),
        },
    )
    .await
    .unwrap();
    assert!(result.success);
    assert!(result
        .verification_signature
        .as_deref()
        .unwrap()
        .starts_with("sha256:"));
    assert!(real.verify_result(&result).await.unwrap());

    // Tampering with the payload breaks verification.
    let mut tampered = result.clone();
    tampered.result["text"] = json!("forged");
    assert!(!real.verify_result(&tampered).await.unwrap());

    // isError from the tool maps to success = false.
    let failed = McpProvider::call_tool(
        &real,
        McpToolCall {
            tool_urn: "urn:mcp:test server:fail".into(),
            parameters: json!({}),
            traceparent: "00-abc-def-01".into(),
        },
    )
    .await
    .unwrap();
    assert!(!failed.success);
    assert!(real.verify_result(&failed).await.unwrap());

    // Unknown server in the URN is a routing error.
    assert!(McpProvider::call_tool(
        &real,
        McpToolCall {
            tool_urn: "urn:mcp:nope:echo".into(),
            parameters: json!({}),
            traceparent: "00-abc-def-01".into(),
        },
    )
    .await
    .is_err());
}

#[tokio::test]
async fn manager_dead_client_without_config_cannot_reconnect() {
    // A test-injected client has no stored config, so when its connection
    // dies the manager must fail cleanly instead of reconnecting.
    let (client_side, server_side) = tokio::io::duplex(1024);
    drop(server_side);
    let (read, write) = tokio::io::split(client_side);
    let client = McpClient::over_stream("dead", "dead", read, write);
    let manager = McpClientManager::new();
    manager.insert_client("dead", client).await;
    let err = manager.list_tools("dead").await.unwrap_err();
    assert!(matches!(err, AdapterError::McpError(_)));
    // Still registered, but reported as not connected once the reader
    // observes EOF.
    for _ in 0..100 {
        if !manager.is_connected("dead").await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(!manager.is_connected("dead").await);
    assert_eq!(manager.connected_ids().await, vec!["dead"]);
}
