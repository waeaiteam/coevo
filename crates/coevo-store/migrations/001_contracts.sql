CREATE TABLE IF NOT EXISTS contracts (
    id TEXT PRIMARY KEY NOT NULL,
    contract_hash TEXT NOT NULL UNIQUE,
    mcl_version TEXT NOT NULL,
    mcl_state TEXT NOT NULL CHECK(mcl_state IN ('DraftContract','ValidatedContract','ActiveContract','SuspendedContract','ClosedContract')),
    parent_contract_hash TEXT NOT NULL,
    goal_tree_json TEXT NOT NULL,
    institution_policy_hash TEXT NOT NULL,
    data_boundary_json TEXT NOT NULL,
    allowed_action_modes_json TEXT NOT NULL,
    human_approval_policy_json TEXT NOT NULL,
    evidence_requirement_json TEXT NOT NULL,
    risk_tolerance_profile_json TEXT NOT NULL,
    termination_policy_json TEXT NOT NULL,
    responsibility_anchor_policy_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX idx_contracts_hash ON contracts(contract_hash);
CREATE INDEX idx_contracts_state ON contracts(mcl_state);
