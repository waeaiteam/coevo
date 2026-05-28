CREATE TABLE IF NOT EXISTS agent_employees (
    agent_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',
    department TEXT NOT NULL DEFAULT 'Custom' CHECK(department IN ('FounderOffice','Product','Engineering','Research','Growth','Finance','Legal','SRE','Design','Content','Custom')),
    role TEXT NOT NULL DEFAULT '',
    passport_json TEXT NOT NULL DEFAULT '{}',
    model_profile_json TEXT NOT NULL DEFAULT '{}',
    tool_scopes_json TEXT NOT NULL DEFAULT '[]',
    memory_scope TEXT NOT NULL DEFAULT 'Agent' CHECK(memory_scope IN ('User','Company','Agent','Task','Skill','Executor','Audit')),
    permission_boundary_json TEXT NOT NULL DEFAULT '{}',
    allowed_cognitive_layers_json TEXT NOT NULL DEFAULT '[]',
    allowed_action_modes_json TEXT NOT NULL DEFAULT '[]',
    risk_ceiling REAL NOT NULL DEFAULT 0.3,
    reputation_vector_json TEXT NOT NULL DEFAULT '{}',
    supervisor_agent_id TEXT,
    lifecycle_status TEXT NOT NULL DEFAULT 'Draft' CHECK(lifecycle_status IN ('Draft','Active','Suspended','Retired')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_employee_status ON agent_employees(lifecycle_status);
CREATE INDEX idx_employee_dept ON agent_employees(department);
