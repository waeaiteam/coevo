-- Stage 2: AI employee self-optimization loop — observability columns,
-- reputation history snapshots, and agent↔prompt binding.

-- 1. Queryable execution summary columns on worker_runs (previously buried in step JSON).
ALTER TABLE worker_runs ADD COLUMN prompt_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE worker_runs ADD COLUMN completion_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE worker_runs ADD COLUMN total_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE worker_runs ADD COLUMN total_cost_usd REAL NOT NULL DEFAULT 0;
ALTER TABLE worker_runs ADD COLUMN latency_ms INTEGER NOT NULL DEFAULT 0;

-- 2. Reputation history snapshots — one row per evaluated run, to draw growth curves.
CREATE TABLE IF NOT EXISTS reputation_history (
    snapshot_id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    run_id TEXT,
    domain_competence REAL NOT NULL DEFAULT 0.5,
    uncertainty_honesty REAL NOT NULL DEFAULT 0.5,
    policy_compliance REAL NOT NULL DEFAULT 0.5,
    resource_efficiency REAL NOT NULL DEFAULT 0.5,
    task_count INTEGER NOT NULL DEFAULT 0,
    overall_score REAL NOT NULL DEFAULT 0.5,
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_rep_hist_agent ON reputation_history(agent_id);
CREATE INDEX IF NOT EXISTS idx_rep_hist_created ON reputation_history(created_at_ms);

-- 3. Agent↔prompt binding so an employee can carry an active published prompt version.
ALTER TABLE agent_employees ADD COLUMN active_prompt_id TEXT;
ALTER TABLE agent_employees ADD COLUMN active_prompt_version_id TEXT;
