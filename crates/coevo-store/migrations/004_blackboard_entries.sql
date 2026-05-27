CREATE TABLE IF NOT EXISTS blackboard_entries (
    id TEXT PRIMARY KEY NOT NULL,
    entry_key TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 0,
    value_json TEXT NOT NULL,
    cognitive_layer TEXT NOT NULL CHECK(cognitive_layer IN ('Hypothesis','Fact','Suggestion','Decision','StaleFact','RevokedFact')),
    source_agent_id TEXT NOT NULL,
    contract_hash TEXT NOT NULL,
    is_valid INTEGER NOT NULL DEFAULT 1,
    created_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER,
    FOREIGN KEY (contract_hash) REFERENCES contracts(contract_hash)
);

CREATE INDEX idx_blackboard_key ON blackboard_entries(entry_key, version);
CREATE INDEX idx_blackboard_layer ON blackboard_entries(cognitive_layer);
CREATE INDEX idx_blackboard_valid ON blackboard_entries(is_valid);
