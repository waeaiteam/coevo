CREATE TABLE IF NOT EXISTS skill_eval_results (
    eval_id TEXT PRIMARY KEY NOT NULL,
    skill_id TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '',
    run_id TEXT NOT NULL DEFAULT '',
    passed INTEGER NOT NULL DEFAULT 0,
    score REAL NOT NULL DEFAULT 0.0,
    failures_json TEXT NOT NULL DEFAULT '[]',
    regression_detected INTEGER NOT NULL DEFAULT 0,
    verifier_notes TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_eval_skill ON skill_eval_results(skill_id);
