-- Migration 052: widen worker_tool_calls tool_type CHECK so real MCP and
-- Browser tool calls can be persisted. Legacy values stay valid for existing rows.
PRAGMA foreign_keys=OFF;

ALTER TABLE worker_tool_calls RENAME TO worker_tool_calls_old;

CREATE TABLE worker_tool_calls (
    tool_call_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    tool_id TEXT NOT NULL,
    tool_type TEXT NOT NULL CHECK(tool_type IN (
        'GitHubReadonly',
        'FileReadonly',
        'BrowserMock',
        'MCPMock',
        'LocalProcessSandbox',
        'ExternalExecutor',
        'MCP',
        'Browser'
    )),
    input_summary TEXT NOT NULL,
    output_summary TEXT NOT NULL,
    success INTEGER NOT NULL,
    risk_ceiling REAL NOT NULL,
    memory_id TEXT,
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER
);

INSERT INTO worker_tool_calls (
    tool_call_id,
    run_id,
    tool_id,
    tool_type,
    input_summary,
    output_summary,
    success,
    risk_ceiling,
    memory_id,
    started_at_ms,
    ended_at_ms
)
SELECT
    tool_call_id,
    run_id,
    tool_id,
    tool_type,
    input_summary,
    output_summary,
    success,
    risk_ceiling,
    memory_id,
    started_at_ms,
    ended_at_ms
FROM worker_tool_calls_old;

DROP TABLE worker_tool_calls_old;

CREATE INDEX idx_wtc_run ON worker_tool_calls(run_id);

PRAGMA foreign_keys=ON;
