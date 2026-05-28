CREATE TABLE IF NOT EXISTS worker_skill_usage (
    usage_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    version TEXT NOT NULL,
    used_for TEXT NOT NULL,
    success INTEGER NOT NULL,
    score REAL NOT NULL,
    notes TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_wsu_run ON worker_skill_usage(run_id);
