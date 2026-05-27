CREATE TABLE IF NOT EXISTS adr_records (
    id TEXT PRIMARY KEY NOT NULL,
    decision_id TEXT NOT NULL UNIQUE,
    mcl_reference TEXT NOT NULL,
    proposer_agent TEXT NOT NULL,
    critic_objections_json TEXT NOT NULL,
    blocker_conflict_status TEXT NOT NULL CHECK(blocker_conflict_status IN ('CONSENSUS','TRADE_OFF','DIVERGENCE')),
    selected_option TEXT NOT NULL,
    rejected_alternatives_json TEXT NOT NULL,
    risk_accepted_json TEXT NOT NULL,
    human_override_reason TEXT,
    responsibility_anchor_json TEXT NOT NULL,
    follow_up_monitoring_plan TEXT,
    post_execution_feedback_json,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (mcl_reference) REFERENCES contracts(contract_hash)
);

CREATE INDEX idx_adr_mcl ON adr_records(mcl_reference);
CREATE INDEX idx_adr_decision ON adr_records(decision_id);
