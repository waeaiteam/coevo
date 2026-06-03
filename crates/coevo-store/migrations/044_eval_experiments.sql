CREATE TABLE IF NOT EXISTS eval_datasets (
    dataset_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS eval_cases (
    case_id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    input_text TEXT NOT NULL,
    expected_text TEXT,
    tags_json TEXT NOT NULL DEFAULT '[]',
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(dataset_id) REFERENCES eval_datasets(dataset_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS eval_experiments (
    experiment_id TEXT PRIMARY KEY,
    target_kind TEXT NOT NULL,
    agent_id TEXT,
    system_prompt TEXT,
    dataset_id TEXT NOT NULL,
    judge_model TEXT NOT NULL,
    exec_model TEXT NOT NULL,
    status TEXT NOT NULL,
    aggregate_json TEXT NOT NULL DEFAULT '{}',
    overall_score REAL NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    FOREIGN KEY(dataset_id) REFERENCES eval_datasets(dataset_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS eval_case_results (
    result_id TEXT PRIMARY KEY,
    experiment_id TEXT NOT NULL,
    case_id TEXT NOT NULL,
    input_text TEXT NOT NULL,
    output_text TEXT NOT NULL,
    scores_json TEXT NOT NULL DEFAULT '{}',
    judge_reasoning TEXT NOT NULL DEFAULT '',
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(experiment_id) REFERENCES eval_experiments(experiment_id) ON DELETE CASCADE,
    FOREIGN KEY(case_id) REFERENCES eval_cases(case_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_eval_cases_dataset ON eval_cases(dataset_id);
CREATE INDEX IF NOT EXISTS idx_eval_experiments_dataset ON eval_experiments(dataset_id);
CREATE INDEX IF NOT EXISTS idx_eval_case_results_experiment ON eval_case_results(experiment_id);
