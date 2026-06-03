-- Agent Workbench: each AI employee carries a system prompt / working charter.
-- Empty by default for built-in employees; editable + versioned via the workbench.
ALTER TABLE agent_employees ADD COLUMN system_prompt TEXT NOT NULL DEFAULT '';
