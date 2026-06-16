ALTER TABLE approval_requests ADD COLUMN opc_id TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_approval_opc_id ON approval_requests(opc_id);
