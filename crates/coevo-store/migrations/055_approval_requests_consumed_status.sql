-- Migration 055: make approval receipts one-time use by allowing a consumed state.
PRAGMA foreign_keys=OFF;

ALTER TABLE approval_requests RENAME TO approval_requests_old;

CREATE TABLE approval_requests (
    id TEXT PRIMARY KEY NOT NULL,
    opc_id TEXT NOT NULL DEFAULT '',
    contract_hash TEXT NOT NULL,
    action_urn TEXT NOT NULL,
    approval_mode TEXT NOT NULL CHECK(approval_mode IN ('NEGATIVE_CONSENT','EXPLICIT_APPROVAL')),
    status TEXT NOT NULL CHECK(status IN ('pending','approved','denied','expired','consumed')),
    requested_by TEXT NOT NULL,
    approved_by TEXT,
    requested_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    decided_at_ms INTEGER,
    FOREIGN KEY (contract_hash) REFERENCES contracts(contract_hash)
);

INSERT INTO approval_requests (
    id,
    opc_id,
    contract_hash,
    action_urn,
    approval_mode,
    status,
    requested_by,
    approved_by,
    requested_at_ms,
    expires_at_ms,
    decided_at_ms
)
SELECT
    id,
    COALESCE(opc_id, ''),
    contract_hash,
    action_urn,
    approval_mode,
    status,
    requested_by,
    approved_by,
    requested_at_ms,
    expires_at_ms,
    decided_at_ms
FROM approval_requests_old;

DROP TABLE approval_requests_old;

CREATE INDEX idx_approval_status ON approval_requests(status);
CREATE INDEX idx_approval_contract ON approval_requests(contract_hash);
CREATE INDEX idx_approval_opc_id ON approval_requests(opc_id);

PRAGMA foreign_keys=ON;