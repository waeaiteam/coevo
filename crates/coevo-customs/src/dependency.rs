//! Cognitive Dependency Graph — local invalidation propagation.
//! Per coevo whitepaper Section 8.3.

use coevo_store::repos::cognitive_edge_repo::CognitiveEdgeRepo;
use sqlx::SqlitePool;
use std::collections::{HashSet, VecDeque};

/// Edge types for the cognitive dependency graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    HypothesisDependsOnFact,
    SuggestionDependsOnHypothesis,
    DecisionDependsOnSuggestion,
    DecisionDependsOnFact,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::HypothesisDependsOnFact => "hypothesis_depends_on_fact",
            EdgeType::SuggestionDependsOnHypothesis => "suggestion_depends_on_hypothesis",
            EdgeType::DecisionDependsOnSuggestion => "decision_depends_on_suggestion",
            EdgeType::DecisionDependsOnFact => "decision_depends_on_fact",
        }
    }
}

/// The cognitive dependency graph.
pub struct CognitiveDependencyGraph;

impl CognitiveDependencyGraph {
    /// Add a dependency edge.
    pub async fn add_edge(
        pool: &SqlitePool,
        source_entry_id: &str,
        target_entry_id: &str,
        edge_type: EdgeType,
    ) -> Result<(), DependencyGraphError> {
        CognitiveEdgeRepo::insert(pool, source_entry_id, target_entry_id, edge_type.as_str())
            .await?;
        Ok(())
    }

    /// When a Fact is invalidated (becomes StaleFact or RevokedFact),
    /// perform local invalidation propagation along the dependency graph.
    /// Returns all entry IDs that were invalidated.
    pub async fn propagate_invalidation(
        pool: &SqlitePool,
        invalidated_entry_id: &str,
    ) -> Result<Vec<String>, DependencyGraphError> {
        let mut invalidated: Vec<String> = vec![invalidated_entry_id.to_string()];
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(invalidated_entry_id.to_string());
        visited.insert(invalidated_entry_id.to_string());

        // BFS traversal following dependents. `visited` dedupes the frontier so
        // a diamond/multi-hop dependency graph never invalidates (or re-queries)
        // the same entry twice; `find_dependents` additionally filters
        // already-invalid rows at the DB layer (SELECT DISTINCT ... is_valid=1),
        // so this loop is idempotent even across overlapping paths.
        while let Some(current) = queue.pop_front() {
            let dependents = CognitiveEdgeRepo::find_dependents(pool, &current).await?;
            for edge in dependents {
                // Skip self-edges and anything already seen on the frontier.
                if edge.source_entry_id == current || visited.contains(&edge.source_entry_id) {
                    continue;
                }
                visited.insert(edge.source_entry_id.clone());
                queue.push_back(edge.source_entry_id.clone());
                invalidated.push(edge.source_entry_id.clone());
                // Mark the dependent entry as invalid
                coevo_store::repos::blackboard_repo::BlackboardRepo::invalidate(
                    pool,
                    &edge.source_entry_id,
                )
                .await?;
            }
        }

        Ok(invalidated)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DependencyGraphError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
