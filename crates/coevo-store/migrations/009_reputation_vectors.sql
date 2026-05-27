CREATE TABLE IF NOT EXISTS reputation_vectors (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL UNIQUE,
    task_domain_competence REAL NOT NULL DEFAULT 0.5,
    uncertainty_honesty REAL NOT NULL DEFAULT 0.5,
    policy_compliance REAL NOT NULL DEFAULT 1.0,
    resource_efficiency REAL NOT NULL DEFAULT 0.5,
    task_count INTEGER NOT NULL DEFAULT 0,
    high_difficulty_avoidance_count INTEGER NOT NULL DEFAULT 0,
    last_updated_ms INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_rep_agent ON reputation_vectors(agent_id);
