ALTER TABLE worker_sessions ADD COLUMN opc_id TEXT NOT NULL DEFAULT '';
UPDATE worker_sessions
SET opc_id = COALESCE(
    (
        SELECT work_orders.opc_id
        FROM work_orders
        WHERE work_orders.work_order_id = worker_sessions.work_order_id
        LIMIT 1
    ),
    ''
)
WHERE opc_id = '';
CREATE INDEX IF NOT EXISTS idx_ws_opc_id ON worker_sessions(opc_id);
CREATE INDEX IF NOT EXISTS idx_ws_opc_work_order ON worker_sessions(opc_id, work_order_id);

ALTER TABLE worker_runs ADD COLUMN opc_id TEXT NOT NULL DEFAULT '';
UPDATE worker_runs
SET opc_id = COALESCE(
    (
        SELECT work_orders.opc_id
        FROM work_orders
        WHERE work_orders.work_order_id = worker_runs.work_order_id
        LIMIT 1
    ),
    ''
)
WHERE opc_id = '';
CREATE INDEX IF NOT EXISTS idx_wr_opc_id ON worker_runs(opc_id);
CREATE INDEX IF NOT EXISTS idx_wr_opc_work_order ON worker_runs(opc_id, work_order_id);
