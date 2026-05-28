CREATE TABLE IF NOT EXISTS skill_version_records (
    skill_id TEXT NOT NULL,
    version TEXT NOT NULL,
    parent_version TEXT NOT NULL DEFAULT '',
    diff_summary TEXT NOT NULL DEFAULT '',
    change_reason TEXT NOT NULL DEFAULT '',
    verifier_result_json TEXT,
    approved_by TEXT,
    rollback_available INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (skill_id, version)
);
