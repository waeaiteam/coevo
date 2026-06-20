-- Migration 054: scope MCP server registrations by company/opc_id.
ALTER TABLE mcp_servers ADD COLUMN opc_id TEXT NOT NULL DEFAULT 'default-opc';

CREATE TABLE mcp_servers_scoped (
    opc_id TEXT NOT NULL,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    transport TEXT NOT NULL CHECK(transport IN ('stdio','http')),
    command TEXT,
    args_json TEXT NOT NULL DEFAULT '[]',
    env_json TEXT NOT NULL DEFAULT '{}',
    url TEXT,
    headers_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'unknown' CHECK(status IN ('unknown','connected','error','disabled')),
    last_error TEXT,
    tools_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (opc_id, id),
    UNIQUE (opc_id, name)
);

INSERT INTO mcp_servers_scoped (
    opc_id, id, name, transport, command, args_json, env_json, url, headers_json,
    enabled, status, last_error, tools_json, created_at, updated_at
)
SELECT
    COALESCE(NULLIF(opc_id, ''), 'default-opc'),
    id,
    name,
    transport,
    command,
    args_json,
    env_json,
    url,
    headers_json,
    enabled,
    status,
    last_error,
    tools_json,
    created_at,
    updated_at
FROM mcp_servers;

DROP TABLE mcp_servers;
ALTER TABLE mcp_servers_scoped RENAME TO mcp_servers;

CREATE INDEX IF NOT EXISTS idx_mcp_servers_opc_enabled ON mcp_servers(opc_id, enabled);
CREATE INDEX IF NOT EXISTS idx_mcp_servers_enabled ON mcp_servers(enabled);