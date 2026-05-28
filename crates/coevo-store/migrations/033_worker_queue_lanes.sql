CREATE TABLE IF NOT EXISTS worker_queue_lanes (
    lane_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL UNIQUE,
    active_run_id TEXT,
    status TEXT NOT NULL DEFAULT 'Idle',
    locked_until_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_wql_session ON worker_queue_lanes(session_id);
