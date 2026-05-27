CREATE TABLE IF NOT EXISTS cognitive_edges (
    id TEXT PRIMARY KEY NOT NULL,
    source_entry_id TEXT NOT NULL,
    target_entry_id TEXT NOT NULL,
    edge_type TEXT NOT NULL CHECK(edge_type IN ('hypothesis_depends_on_fact','suggestion_depends_on_hypothesis','decision_depends_on_suggestion','decision_depends_on_fact')),
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY (source_entry_id) REFERENCES blackboard_entries(id),
    FOREIGN KEY (target_entry_id) REFERENCES blackboard_entries(id)
);

CREATE INDEX idx_edges_source ON cognitive_edges(source_entry_id);
CREATE INDEX idx_edges_target ON cognitive_edges(target_entry_id);
