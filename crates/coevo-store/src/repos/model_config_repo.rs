use sqlx::{SqlitePool, Row};
use coevo_models::types::*;

pub struct ModelConfigRepo;
impl ModelConfigRepo {
    // Alpha: api_key stored as plaintext in api_key_ciphertext field.
    // TODO: Replace with OS keychain/credential vault before Private Beta.
    pub fn mask_key(key: &str) -> String {
        if key.is_empty() { return "****".into(); }
        if key.len() <= 8 { return format!("{}****", &key[..key.len().min(4)]); }
        format!("{}****{}", &key[..4], &key[key.len()-4..])
    }

    fn parse_config(row: &sqlx::sqlite::SqliteRow) -> Result<ModelProviderConfig, sqlx::Error> {
        let kind_str: String = row.get("kind");
        let kind: ModelProviderKind = serde_json::from_str(&format!("\"{}\"", kind_str))
            .map_err(|_| sqlx::Error::Protocol(format!("MODEL_CONFIG_INVALID_KIND: {}", kind_str)))?;
        Ok(ModelProviderConfig {
            provider_id: row.get("provider_id"),
            kind,
            base_url: row.get("base_url"),
            api_key: row.get("api_key_ciphertext"),
            default_model: row.get("default_model"),
            fast_model: row.get("fast_model"),
            reasoning_model: row.get("reasoning_model"),
            structured_output_model: row.get("structured_output_model"),
            max_tokens: row.get::<i64,_>("max_tokens") as u32,
            temperature: row.get("temperature"),
            timeout_ms: row.get::<i64,_>("timeout_ms") as u64,
            max_cost_per_task_usd: row.get("max_cost_per_task_usd"),
        })
    }

    pub async fn seed_mock_if_empty(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let count: i64 = sqlx::query("SELECT COUNT(*) as c FROM model_provider_configs").fetch_one(pool).await?.get("c");
        if count == 0 {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query("INSERT INTO model_provider_configs VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind("mock-default").bind("Mock").bind("").bind("").bind("").bind("mock-model")
                .bind("mock-model").bind("mock-model").bind("mock-model").bind(4096).bind(0.7)
                .bind(30000).bind(0.0).bind(1).bind(now).bind(now).execute(pool).await?;
        }
        Ok(())
    }

    pub async fn get_active_config(pool: &SqlitePool) -> Result<Option<ModelProviderConfig>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM model_provider_configs WHERE is_active=1 LIMIT 1")
            .fetch_optional(pool).await?;
        match row {
            Some(ref r) => Ok(Some(Self::parse_config(r)?)),
            None => Ok(None),
        }
    }

    pub async fn get_active_config_or_seed(pool: &SqlitePool) -> Result<ModelProviderConfig, sqlx::Error> {
        Self::seed_mock_if_empty(pool).await?;
        Self::get_active_config(pool).await?.ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn set_active(pool: &SqlitePool, provider_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE model_provider_configs SET is_active=0,updated_at_ms=?").bind(now).execute(pool).await?;
        sqlx::query("UPDATE model_provider_configs SET is_active=1,updated_at_ms=? WHERE provider_id=?").bind(now).bind(provider_id).execute(pool).await?;
        Ok(())
    }

    pub async fn upsert_config(pool: &SqlitePool, pid: &str, kind_str: &str, base_url: &str, api_key: &str, masked: &str, default_model: &str, fast_model: &str, reasoning_model: &str, structured_model: &str, max_tokens: i64, temperature: f64, timeout_ms: i64, max_cost: f64) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,1,?,?) ON CONFLICT(provider_id) DO UPDATE SET kind=excluded.kind,base_url=excluded.base_url,api_key_ciphertext=excluded.api_key_ciphertext,api_key_masked=excluded.api_key_masked,default_model=excluded.default_model,fast_model=excluded.fast_model,reasoning_model=excluded.reasoning_model,structured_output_model=excluded.structured_output_model,max_tokens=excluded.max_tokens,temperature=excluded.temperature,timeout_ms=excluded.timeout_ms,max_cost_per_task_usd=excluded.max_cost_per_task_usd,is_active=1,updated_at_ms=excluded.updated_at_ms")
            .bind(pid).bind(kind_str).bind(base_url).bind(api_key).bind(masked).bind(default_model).bind(fast_model).bind(reasoning_model).bind(structured_model)
            .bind(max_tokens).bind(temperature).bind(timeout_ms).bind(max_cost).bind(now).bind(now).execute(pool).await?;
        Ok(())
    }
    pub async fn deactivate_others(pool: &SqlitePool, provider_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE model_provider_configs SET is_active=0,updated_at_ms=? WHERE provider_id!=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(provider_id).execute(pool).await?; Ok(())
    }
    pub async fn clear_api_key(pool: &SqlitePool, provider_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE model_provider_configs SET api_key_ciphertext='',api_key_masked='',updated_at_ms=? WHERE provider_id=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(provider_id).execute(pool).await?; Ok(())
    }
}
