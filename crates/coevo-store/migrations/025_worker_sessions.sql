CREATE TABLE IF NOT EXISTS worker_sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    worker_id TEXT NOT NULL,
    work_order_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'MissionChat' CHECK(channel IN ('MissionChat','API','System','Scheduled')),
    messages_json TEXT NOT NULL DEFAULT '[]',
    context_memory_ids_json TEXT NOT NULL DEFAULT '[]',
    loaded_skill_ids_json TEXT NOT NULL DEFAULT '[]',
    tool_call_ids_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'Open' CHECK(status IN ('Open','Running','WaitingApproval','Completed','Failed','Cancelled')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_ws_worker ON worker_sessions(worker_id);
CREATE INDEX idx_ws_wo ON worker_sessions(work_order_id);
