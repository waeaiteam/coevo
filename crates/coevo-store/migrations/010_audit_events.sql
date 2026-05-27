CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    event_type TEXT NOT NULL,
    contract_hash TEXT,
    agent_id TEXT,
    traceparent TEXT,
    tenant_id TEXT NOT NULL,
    event_data_json TEXT NOT NULL,
    recorded_at_ms INTEGER NOT NULL
);

CREATE INDEX idx_audit_type ON audit_events(event_type);
CREATE INDEX idx_audit_tenant ON audit_events(tenant_id);
CREATE INDEX idx_audit_time ON audit_events(recorded_at_ms);
