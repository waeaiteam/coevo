CREATE TABLE IF NOT EXISTS work_orders (
    work_order_id TEXT PRIMARY KEY NOT NULL,
    contract_hash TEXT NOT NULL DEFAULT '',
    plan_hash TEXT NOT NULL DEFAULT '',
    user_id TEXT NOT NULL DEFAULT '',
    opc_id TEXT NOT NULL DEFAULT '',
    mission_intent TEXT NOT NULL DEFAULT '',
    selected_agents_json TEXT NOT NULL DEFAULT '[]',
    selected_executors_json TEXT NOT NULL DEFAULT '[]',
    required_skills_json TEXT NOT NULL DEFAULT '[]',
    track TEXT NOT NULL DEFAULT 'green' CHECK(track IN ('green','yellow','red')),
    status TEXT NOT NULL DEFAULT 'Draft' CHECK(status IN ('Draft','Planned','Running','WaitingApproval','Completed','Failed','Cancelled')),
    allowed_actions_json TEXT NOT NULL DEFAULT '[]',
    restricted_actions_json TEXT NOT NULL DEFAULT '[]',
    risk_summary TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_wo_user ON work_orders(user_id);
CREATE INDEX idx_wo_status ON work_orders(status);
