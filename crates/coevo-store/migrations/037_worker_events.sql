CREATE TABLE IF NOT EXISTS worker_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_we_session ON worker_events(session_id);
