CREATE TABLE IF NOT EXISTS worker_tasks (
    task_id TEXT PRIMARY KEY NOT NULL,
    work_order_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    intent TEXT NOT NULL,
    step_goal TEXT NOT NULL,
    required_skills_json TEXT NOT NULL DEFAULT '[]',
    allowed_tools_json TEXT NOT NULL DEFAULT '[]',
    restricted_tools_json TEXT NOT NULL DEFAULT '[]',
    track TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Draft',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_wt_wo ON worker_tasks(work_order_id);
