use sqlx::{Column, Row, SqlitePool};

pub struct EvalRepo;

impl EvalRepo {
    pub async fn create_dataset(
        pool: &SqlitePool,
        dataset_id: &str,
        name: &str,
        description: &str,
        created_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO eval_datasets (dataset_id, name, description, created_at_ms, updated_at_ms) VALUES (?,?,?,?,?)",
        )
        .bind(dataset_id)
        .bind(name)
        .bind(description)
        .bind(created_at_ms)
        .bind(created_at_ms)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_datasets(
        pool: &SqlitePool,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM eval_datasets ORDER BY created_at_ms DESC")
            .fetch_all(pool)
            .await
    }

    pub async fn create_case(
        pool: &SqlitePool,
        case_id: &str,
        dataset_id: &str,
        input_text: &str,
        expected_text: Option<&str>,
        tags_json: &str,
        created_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO eval_cases (case_id, dataset_id, input_text, expected_text, tags_json, created_at_ms) VALUES (?,?,?,?,?,?)",
        )
        .bind(case_id)
        .bind(dataset_id)
        .bind(input_text)
        .bind(expected_text)
        .bind(tags_json)
        .bind(created_at_ms)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_cases(
        pool: &SqlitePool,
        dataset_id: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM eval_cases WHERE dataset_id=? ORDER BY created_at_ms")
            .bind(dataset_id)
            .fetch_all(pool)
            .await
    }

    pub async fn delete_case(pool: &SqlitePool, case_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM eval_cases WHERE case_id=?")
            .bind(case_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn case_belongs_to_dataset(
        pool: &SqlitePool,
        case_id: &str,
        dataset_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT 1 FROM eval_cases WHERE case_id=? AND dataset_id=?")
            .bind(case_id)
            .bind(dataset_id)
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_experiment(
        pool: &SqlitePool,
        experiment_id: &str,
        target_kind: &str,
        agent_id: Option<&str>,
        system_prompt: Option<&str>,
        dataset_id: &str,
        judge_model: &str,
        exec_model: &str,
        status: &str,
        created_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO eval_experiments (experiment_id, target_kind, agent_id, system_prompt, dataset_id, judge_model, exec_model, status, aggregate_json, overall_score, created_at_ms, completed_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,NULL)",
        )
        .bind(experiment_id)
        .bind(target_kind)
        .bind(agent_id)
        .bind(system_prompt)
        .bind(dataset_id)
        .bind(judge_model)
        .bind(exec_model)
        .bind(status)
        .bind("{}")
        .bind(0.0f64)
        .bind(created_at_ms)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn complete_experiment(
        pool: &SqlitePool,
        experiment_id: &str,
        aggregate_json: &str,
        overall_score: f64,
        completed_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE eval_experiments SET status='completed', aggregate_json=?, overall_score=?, completed_at_ms=? WHERE experiment_id=?",
        )
        .bind(aggregate_json)
        .bind(overall_score)
        .bind(completed_at_ms)
        .bind(experiment_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn append_case_result(
        pool: &SqlitePool,
        result_id: &str,
        experiment_id: &str,
        case_id: &str,
        input_text: &str,
        output_text: &str,
        scores_json: &str,
        judge_reasoning: &str,
        created_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO eval_case_results (result_id, experiment_id, case_id, input_text, output_text, scores_json, judge_reasoning, created_at_ms) VALUES (?,?,?,?,?,?,?,?)",
        )
        .bind(result_id)
        .bind(experiment_id)
        .bind(case_id)
        .bind(input_text)
        .bind(output_text)
        .bind(scores_json)
        .bind(judge_reasoning)
        .bind(created_at_ms)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_experiments(
        pool: &SqlitePool,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM eval_experiments ORDER BY created_at_ms DESC")
            .fetch_all(pool)
            .await
    }

    pub async fn get_experiment(
        pool: &SqlitePool,
        experiment_id: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM eval_experiments WHERE experiment_id=?")
            .bind(experiment_id)
            .fetch_optional(pool)
            .await
    }

    pub async fn list_case_results(
        pool: &SqlitePool,
        experiment_id: &str,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM eval_case_results WHERE experiment_id=? ORDER BY created_at_ms")
            .bind(experiment_id)
            .fetch_all(pool)
            .await
    }

    pub async fn dataset_exists(pool: &SqlitePool, dataset_id: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT 1 FROM eval_datasets WHERE dataset_id=?")
            .bind(dataset_id)
            .fetch_optional(pool)
            .await?;
        Ok(row.is_some())
    }
}

pub fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for column in row.columns() {
        let name = column.name();
        if let Ok(value) = row.try_get::<String, _>(name) {
            map.insert(name.to_string(), serde_json::Value::String(value));
        } else if let Ok(value) = row.try_get::<i64, _>(name) {
            map.insert(name.to_string(), serde_json::Value::Number(value.into()));
        } else if let Ok(value) = row.try_get::<f64, _>(name) {
            map.insert(
                name.to_string(),
                serde_json::Number::from_f64(value)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
    }
    serde_json::Value::Object(map)
}
