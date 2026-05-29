CREATE TABLE IF NOT EXISTS worker_sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    work_order_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    worker_id TEXT NOT NULL,
    selected_skills_json TEXT NOT NULL DEFAULT '[]',
    selected_tools_json TEXT NOT NULL DEFAULT '[]',
    memory_context_ids_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'Running' CHECK(status IN ('Running','WaitingApproval','Completed','Failed','Cancelled')),
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER
);
CREATE INDEX idx_ws_wo ON worker_sessions(work_order_id);
