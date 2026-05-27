CREATE TABLE IF NOT EXISTS approval_requests (
    id TEXT PRIMARY KEY NOT NULL,
    contract_hash TEXT NOT NULL,
    action_urn TEXT NOT NULL,
    approval_mode TEXT NOT NULL CHECK(approval_mode IN ('NEGATIVE_CONSENT','EXPLICIT_APPROVAL')),
    status TEXT NOT NULL CHECK(status IN ('pending','approved','denied','expired')),
    requested_by TEXT NOT NULL,
    approved_by TEXT,
    requested_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    decided_at_ms INTEGER,
    FOREIGN KEY (contract_hash) REFERENCES contracts(contract_hash)
);

CREATE INDEX idx_approval_status ON approval_requests(status);
CREATE INDEX idx_approval_contract ON approval_requests(contract_hash);
