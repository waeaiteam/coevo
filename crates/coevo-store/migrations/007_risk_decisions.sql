CREATE TABLE IF NOT EXISTS risk_decisions (
    id TEXT PRIMARY KEY NOT NULL,
    decision_id TEXT NOT NULL UNIQUE,
    contract_hash TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    action_urn TEXT NOT NULL,
    decision TEXT NOT NULL CHECK(decision IN ('ALLOW','DENY','REQUIRE_HUMAN_APPROVAL','DEFER_FOR_MORE_EVIDENCE','ALLOW_WITH_LEASE','ESCALATE_TO_RESOLUTION')),
    required_confidence REAL NOT NULL,
    available_confidence REAL NOT NULL,
    action_risk REAL NOT NULL,
    inaction_risk REAL NOT NULL,
    reason TEXT NOT NULL,
    decided_at_ms INTEGER NOT NULL,
    FOREIGN KEY (contract_hash) REFERENCES contracts(contract_hash)
);

CREATE INDEX idx_risk_decision ON risk_decisions(decision_id);
CREATE INDEX idx_risk_contract ON risk_decisions(contract_hash);
