CREATE TABLE IF NOT EXISTS user_profiles (
    user_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',
    preferred_language TEXT NOT NULL DEFAULT 'en',
    timezone TEXT NOT NULL DEFAULT 'UTC',
    risk_preference TEXT NOT NULL DEFAULT 'Balanced' CHECK(risk_preference IN ('Conservative','Balanced','Aggressive')),
    default_mission_mode TEXT NOT NULL DEFAULT 'Auto' CHECK(default_mission_mode IN ('Auto','ReadOnly','Collaborative','HighRiskRequest')),
    long_term_goals_json TEXT NOT NULL DEFAULT '[]',
    business_domains_json TEXT NOT NULL DEFAULT '[]',
    communication_style TEXT NOT NULL DEFAULT '',
    approval_preferences_json TEXT NOT NULL DEFAULT '{}',
    data_boundaries_json TEXT NOT NULL DEFAULT '[]',
    budget_limits_json TEXT NOT NULL DEFAULT '{}',
    favorite_tools_json TEXT NOT NULL DEFAULT '[]',
    active_projects_json TEXT NOT NULL DEFAULT '[]',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
