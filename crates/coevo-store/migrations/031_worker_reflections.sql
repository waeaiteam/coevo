CREATE TABLE IF NOT EXISTS worker_reflections (
    reflection_id TEXT PRIMARY KEY NOT NULL,
    work_order_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    worker_id TEXT NOT NULL,
    what_worked_json TEXT NOT NULL DEFAULT '[]',
    what_failed_json TEXT NOT NULL DEFAULT '[]',
    memory_to_add_json TEXT NOT NULL DEFAULT '[]',
    skill_to_update_json TEXT NOT NULL DEFAULT '[]',
    user_preference_observed_json TEXT NOT NULL DEFAULT '[]',
    needs_human_review INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_wr_ref_run ON worker_reflections(run_id);
