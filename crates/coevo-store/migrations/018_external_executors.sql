CREATE TABLE IF NOT EXISTS external_executors (
    executor_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',
    source_type TEXT NOT NULL DEFAULT 'Custom' CHECK(source_type IN ('Hermes','OpenClaw','MCP','Local302AI','LocalProcess','Browser','Docker','Custom')),
    runtime_endpoint TEXT NOT NULL DEFAULT '',
    capabilities_json TEXT NOT NULL DEFAULT '[]',
    required_credentials_json TEXT NOT NULL DEFAULT '[]',
    permission_boundary_json TEXT NOT NULL DEFAULT '{}',
    file_scope_json TEXT NOT NULL DEFAULT '[]',
    network_scope_json TEXT NOT NULL DEFAULT '[]',
    memory_scope TEXT NOT NULL DEFAULT 'Executor' CHECK(memory_scope IN ('User','Company','Agent','Task','Skill','Executor','Audit')),
    risk_ceiling REAL NOT NULL DEFAULT 0.5,
    supported_actions_json TEXT NOT NULL DEFAULT '[]',
    sandbox_level TEXT NOT NULL DEFAULT 'None' CHECK(sandbox_level IN ('None','Process','Container','VM','Remote')),
    health_check_url TEXT NOT NULL DEFAULT '',
    audit_callback_url TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'Draft' CHECK(status IN ('Draft','Registered','Disabled')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_executor_status ON external_executors(status);
