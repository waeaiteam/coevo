CREATE TABLE IF NOT EXISTS provenance_envelopes (
    id TEXT PRIMARY KEY NOT NULL,
    entry_id TEXT NOT NULL,
    source_agent_id TEXT NOT NULL,
    verification_tool_urn TEXT NOT NULL,
    environmental_scope_json TEXT NOT NULL,
    ttl_seconds INTEGER NOT NULL,
    cryptographic_signature TEXT NOT NULL,
    verification_report_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (entry_id) REFERENCES blackboard_entries(id)
);

CREATE INDEX idx_provenance_entry ON provenance_envelopes(entry_id);
