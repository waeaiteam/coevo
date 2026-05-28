CREATE TABLE IF NOT EXISTS opc_profiles (
    opc_id TEXT PRIMARY KEY NOT NULL,
    founder_user_id TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    mission TEXT NOT NULL DEFAULT '',
    current_strategy TEXT NOT NULL DEFAULT '',
    operating_principles_json TEXT NOT NULL DEFAULT '[]',
    active_projects_json TEXT NOT NULL DEFAULT '[]',
    asset_indexes_json TEXT NOT NULL DEFAULT '[]',
    policy_profile TEXT NOT NULL DEFAULT '',
    memory_policy_json TEXT NOT NULL DEFAULT '{}',
    default_departments_json TEXT NOT NULL DEFAULT '[]',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
