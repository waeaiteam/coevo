CREATE TABLE IF NOT EXISTS worker_runs (
    run_id TEXT PRIMARY KEY NOT NULL,
    work_order_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    worker_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Queued' CHECK(status IN ('Queued','Running','WaitingApproval','Completed','Failed','Cancelled','TimedOut','Blocked')),
    result_json TEXT NOT NULL DEFAULT '{}',
    memory_ids_json TEXT NOT NULL DEFAULT '[]',
    errors_json TEXT NOT NULL DEFAULT '[]',
    audit_ref TEXT,
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER
);
CREATE INDEX idx_wr_wo ON worker_runs(work_order_id);
CREATE INDEX idx_wr_agent ON worker_runs(agent_id);
CREATE INDEX idx_wr_worker ON worker_runs(worker_id);
