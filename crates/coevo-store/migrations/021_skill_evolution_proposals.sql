CREATE TABLE IF NOT EXISTS skill_evolution_proposals (
    proposal_id TEXT PRIMARY KEY NOT NULL,
    source_type TEXT NOT NULL DEFAULT 'Failure' CHECK(source_type IN ('Failure','RepeatedSuccess','UserFeedback','AgentReflection','CriticObjection','ADRFeedback')),
    source_refs_json TEXT NOT NULL DEFAULT '[]',
    target_skill_id TEXT NOT NULL DEFAULT '',
    proposal_type TEXT NOT NULL DEFAULT 'PatchSkill' CHECK(proposal_type IN ('CreateNewSkill','PatchSkill','DeprecateSkill','SplitSkill','MergeSkills')),
    diagnosis TEXT NOT NULL DEFAULT '',
    proposed_changes TEXT NOT NULL DEFAULT '',
    expected_benefit TEXT NOT NULL DEFAULT '',
    risk_assessment TEXT NOT NULL DEFAULT '',
    generated_tests_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'Draft' CHECK(status IN ('Draft','UnderVerification','NeedsHumanReview','Approved','Rejected','Applied')),
    created_by_agent TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_proposal_skill ON skill_evolution_proposals(target_skill_id);
CREATE INDEX idx_proposal_status ON skill_evolution_proposals(status);
