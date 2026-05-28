CREATE TABLE IF NOT EXISTS memory_records (
    memory_id TEXT PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('User','Company','Agent','Task','Skill','Executor','Audit')),
    owner_id TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL DEFAULT '',
    provenance TEXT NOT NULL DEFAULT '',
    confidence REAL NOT NULL DEFAULT 0.5,
    ttl_seconds INTEGER NOT NULL DEFAULT 86400,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    access_policy TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'Active' CHECK(status IN ('Active','Stale','Revoked')),
    cognitive_layer TEXT NOT NULL DEFAULT 'Hypothesis' CHECK(cognitive_layer IN ('Hypothesis','Fact','Suggestion','Decision','StaleFact','RevokedFact','AuditNote')),
    linked_contract_hash TEXT,
    linked_plan_hash TEXT,
    linked_adr_id TEXT
);
CREATE INDEX idx_memory_owner ON memory_records(owner_id, scope);
CREATE INDEX idx_memory_status ON memory_records(status);
