CREATE TABLE IF NOT EXISTS agent_memories (
    agent_id TEXT PRIMARY KEY NOT NULL,
    memory_records_json TEXT NOT NULL DEFAULT '[]',
    working_preferences TEXT NOT NULL DEFAULT '',
    learned_constraints_json TEXT NOT NULL DEFAULT '[]',
    recurring_failures_json TEXT NOT NULL DEFAULT '[]',
    successful_patterns_json TEXT NOT NULL DEFAULT '[]',
    recent_tasks_json TEXT NOT NULL DEFAULT '[]',
    performance_notes TEXT NOT NULL DEFAULT '',
    skill_usage_stats TEXT NOT NULL DEFAULT ''
);
