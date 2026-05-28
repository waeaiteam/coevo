CREATE TABLE IF NOT EXISTS worker_steps (
    step_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    step_index INTEGER NOT NULL,
    step_type TEXT NOT NULL CHECK(step_type IN ('BuildContext','LoadMemory','LoadSkillIndex','LoadSkillFull','Think','ModelCall','SelectTool','CallTool','CallExecutor','WriteMemory','Reflect','ProposeSkillUpdate','AskHuman')),
    input_json TEXT NOT NULL DEFAULT '{}',
    output_json TEXT,
    status TEXT NOT NULL DEFAULT 'Pending' CHECK(status IN ('Pending','Running','Completed','Failed','Skipped')),
    started_at_ms INTEGER NOT NULL,
    ended_at_ms INTEGER,
    error TEXT,
    UNIQUE(run_id, step_index)
);
CREATE INDEX idx_ws_run ON worker_steps(run_id);
CREATE INDEX idx_ws_type ON worker_steps(step_type);
