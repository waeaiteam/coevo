-- Migration 047: align legacy runtime status CHECK constraints with current backend states.
PRAGMA foreign_keys=OFF;

ALTER TABLE worker_sessions RENAME TO worker_sessions_old;

CREATE TABLE worker_sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    worker_id TEXT NOT NULL,
    work_order_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'MissionChat' CHECK(channel IN ('MissionChat','API','System','Scheduled')),
    messages_json TEXT NOT NULL DEFAULT '[]',
    context_memory_ids_json TEXT NOT NULL DEFAULT '[]',
    loaded_skill_ids_json TEXT NOT NULL DEFAULT '[]',
    tool_call_ids_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'Running' CHECK(status IN ('Open','Running','WaitingApproval','Completed','Failed','Cancelled','TimedOut','Blocked')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

INSERT INTO worker_sessions (
    session_id,
    worker_id,
    work_order_id,
    agent_id,
    channel,
    messages_json,
    context_memory_ids_json,
    loaded_skill_ids_json,
    tool_call_ids_json,
    status,
    created_at_ms,
    updated_at_ms
)
SELECT
    session_id,
    worker_id,
    work_order_id,
    agent_id,
    COALESCE(channel, 'MissionChat'),
    COALESCE(messages_json, '[]'),
    COALESCE(context_memory_ids_json, '[]'),
    COALESCE(loaded_skill_ids_json, '[]'),
    COALESCE(tool_call_ids_json, '[]'),
    status,
    created_at_ms,
    updated_at_ms
FROM worker_sessions_old;

DROP TABLE worker_sessions_old;

CREATE INDEX idx_ws_worker ON worker_sessions(worker_id);
CREATE INDEX idx_ws_wo ON worker_sessions(work_order_id);

ALTER TABLE work_orders RENAME TO work_orders_old;

CREATE TABLE work_orders (
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
    status TEXT NOT NULL DEFAULT 'Draft' CHECK(status IN ('Draft','Planned','Running','WaitingApproval','Completed','Failed','Cancelled','Blocked')),
    allowed_actions_json TEXT NOT NULL DEFAULT '[]',
    restricted_actions_json TEXT NOT NULL DEFAULT '[]',
    risk_summary TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    conversation_id TEXT DEFAULT NULL,
    governance_proposal_json TEXT DEFAULT NULL,
    governance_verdict_json TEXT DEFAULT NULL
);

INSERT INTO work_orders (
    work_order_id,
    contract_hash,
    plan_hash,
    user_id,
    opc_id,
    mission_intent,
    selected_agents_json,
    selected_executors_json,
    required_skills_json,
    track,
    status,
    allowed_actions_json,
    restricted_actions_json,
    risk_summary,
    created_at_ms,
    updated_at_ms,
    conversation_id,
    governance_proposal_json,
    governance_verdict_json
)
SELECT
    work_order_id,
    contract_hash,
    plan_hash,
    user_id,
    opc_id,
    mission_intent,
    selected_agents_json,
    selected_executors_json,
    required_skills_json,
    track,
    status,
    allowed_actions_json,
    restricted_actions_json,
    risk_summary,
    created_at_ms,
    updated_at_ms,
    conversation_id,
    governance_proposal_json,
    governance_verdict_json
FROM work_orders_old;

DROP TABLE work_orders_old;

CREATE INDEX idx_wo_user ON work_orders(user_id);
CREATE INDEX idx_wo_status ON work_orders(status);
CREATE INDEX idx_wo_conversation ON work_orders(conversation_id);

PRAGMA foreign_keys=ON;
