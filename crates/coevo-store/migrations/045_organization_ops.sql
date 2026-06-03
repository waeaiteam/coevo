CREATE TABLE IF NOT EXISTS meetings (
    meeting_id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    status TEXT NOT NULL,
    close_mode TEXT NOT NULL,
    agenda_json TEXT NOT NULL DEFAULT '[]',
    participants_json TEXT NOT NULL DEFAULT '[]',
    transcript_json TEXT NOT NULL DEFAULT '[]',
    resolution_md TEXT NOT NULL DEFAULT '',
    responsibility_anchor TEXT NOT NULL DEFAULT '',
    archive_relpath TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_meetings_created ON meetings(created_at_ms DESC);

CREATE TABLE IF NOT EXISTS kpi_records (
    kpi_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    work_order_id TEXT NOT NULL DEFAULT '',
    reviewer_agent_id TEXT NOT NULL,
    scores_json TEXT NOT NULL DEFAULT '{}',
    comment TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_kpi_records_agent ON kpi_records(agent_id, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_kpi_records_reviewer ON kpi_records(reviewer_agent_id, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS generated_reports (
    report_id TEXT PRIMARY KEY,
    period TEXT NOT NULL,
    report_md_path TEXT NOT NULL,
    kpi_summary_json TEXT NOT NULL DEFAULT '[]',
    token_usage_json TEXT NOT NULL DEFAULT '{}',
    alerts_json TEXT NOT NULL DEFAULT '[]',
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_generated_reports_period ON generated_reports(period, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS department_cost_quotas (
    department TEXT PRIMARY KEY,
    token_quota INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
