CREATE TABLE IF NOT EXISTS worker_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    event_seq INTEGER NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN ('LifecycleStart','LifecycleEnd','LifecycleError','AssistantDelta','ToolStart','ToolUpdate','ToolEnd','MemoryWrite','SkillLoaded','ApprovalRequired','WorkerBlocked')),
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at_ms INTEGER NOT NULL,
    UNIQUE(run_id, event_seq)
);
CREATE INDEX idx_we_run ON worker_events(run_id);
