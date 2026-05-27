CREATE TABLE IF NOT EXISTS leases (
    id TEXT PRIMARY KEY NOT NULL,
    lease_id TEXT NOT NULL UNIQUE,
    contract_hash TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    lease_scope_json TEXT NOT NULL,
    lease_budget INTEGER NOT NULL,
    operations_used INTEGER NOT NULL DEFAULT 0,
    granted_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    ttl_ms INTEGER NOT NULL,
    monitoring_signature TEXT NOT NULL,
    diagnostic_signature TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    was_revoked INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (contract_hash) REFERENCES contracts(contract_hash)
);

CREATE INDEX idx_leases_active ON leases(is_active);
CREATE INDEX idx_leases_agent ON leases(agent_id);
