CREATE TABLE IF NOT EXISTS agent_registry (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL UNIQUE,
    passport_json TEXT NOT NULL,
    capabilities_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('active','suspended','revoked')),
    registered_at_ms INTEGER NOT NULL
);

CREATE INDEX idx_agents_status ON agent_registry(status);
