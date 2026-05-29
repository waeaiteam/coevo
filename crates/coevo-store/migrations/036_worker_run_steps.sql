CREATE TABLE IF NOT EXISTS worker_run_steps (
    step_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    step_type TEXT NOT NULL,
    input_json TEXT NOT NULL,
    output_json TEXT,
    status TEXT NOT NULL DEFAULT 'Pending',
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_wrs_session ON worker_run_steps(session_id);
