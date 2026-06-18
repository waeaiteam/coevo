-- Migration 053: widen worker_events event_type CHECK so governed sub-agent
-- delegation events can be persisted. A department head spawning a bounded,
-- single-skill helper records SubagentSpawned, then SubagentCompleted or
-- SubagentFailed with the helper's outcome. Existing rows stay valid.
PRAGMA foreign_keys=OFF;

ALTER TABLE worker_events RENAME TO worker_events_old;

CREATE TABLE worker_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    event_seq INTEGER NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN (
        'LifecycleStart',
        'LifecycleEnd',
        'LifecycleError',
        'AssistantDelta',
        'ReasoningDelta',
        'ContentDelta',
        'ToolCallDelta',
        'Usage',
        'Done',
        'ToolStart',
        'ToolUpdate',
        'ToolEnd',
        'MemoryWrite',
        'SkillLoaded',
        'ApprovalRequired',
        'WorkerBlocked',
        'SubagentSpawned',
        'SubagentCompleted',
        'SubagentFailed'
    )),
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at_ms INTEGER NOT NULL,
    session_id TEXT NOT NULL DEFAULT '',
    UNIQUE(run_id, event_seq)
);

INSERT INTO worker_events (
    event_id,
    run_id,
    event_seq,
    event_type,
    payload_json,
    created_at_ms,
    session_id
)
SELECT
    event_id,
    run_id,
    event_seq,
    event_type,
    payload_json,
    created_at_ms,
    COALESCE(session_id, '')
FROM worker_events_old;

DROP TABLE worker_events_old;

CREATE INDEX idx_we_run ON worker_events(run_id);

PRAGMA foreign_keys=ON;
