CREATE TABLE IF NOT EXISTS agent_workers (
    worker_id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    department TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Idle' CHECK(status IN ('Idle','Assigned','Planning','Executing','WaitingApproval','Reflecting','Completed','Failed','Cancelled')),
    current_work_order_id TEXT,
    current_session_id TEXT,
    loaded_skills_json TEXT NOT NULL DEFAULT '[]',
    memory_scope TEXT NOT NULL,
    tool_scope_json TEXT NOT NULL DEFAULT '[]',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_worker_agent ON agent_workers(agent_id);
CREATE INDEX idx_worker_status ON agent_workers(status);
