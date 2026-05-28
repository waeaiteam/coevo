CREATE TABLE IF NOT EXISTS worker_tool_calls (
    tool_call_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    tool_id TEXT NOT NULL,
    tool_type TEXT NOT NULL CHECK(tool_type IN ('GitHubReadonly','FileReadonly','BrowserMock','MCPMock','LocalProcessSandbox','ExternalExecutor')),
    input_summary TEXT NOT NULL,
    output_summary TEXT NOT NULL,
    success INTEGER NOT NULL,
    risk_ceiling REAL NOT NULL,
    memory_id TEXT,
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER
);
CREATE INDEX idx_wtc_run ON worker_tool_calls(run_id);
