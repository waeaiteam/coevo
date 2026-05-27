CREATE TABLE IF NOT EXISTS execution_plans (
    id TEXT PRIMARY KEY NOT NULL,
    plan_hash TEXT NOT NULL UNIQUE,
    contract_hash TEXT NOT NULL,
    execution_plan_version TEXT NOT NULL,
    parent_plan_hash TEXT NOT NULL,
    primary_path_dag_json TEXT NOT NULL,
    agent_configs_json TEXT NOT NULL,
    failback_rules_json TEXT NOT NULL,
    hard_resource_ceilings_json TEXT NOT NULL,
    exploration_budget_quota REAL NOT NULL DEFAULT 0.0,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (contract_hash) REFERENCES contracts(contract_hash)
);

CREATE INDEX idx_plans_contract ON execution_plans(contract_hash);
CREATE INDEX idx_plans_hash ON execution_plans(plan_hash);
