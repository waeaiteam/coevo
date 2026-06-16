ALTER TABLE agent_workers ADD COLUMN opc_id TEXT NOT NULL DEFAULT 'default-opc';
CREATE INDEX IF NOT EXISTS idx_worker_opc ON agent_workers(opc_id);
