use sqlx::{SqlitePool, Row};

pub struct ModelConfigRepo;
impl ModelConfigRepo {
    // Alpha: api_key stored as plaintext in api_key_ciphertext field.
    // TODO: Replace with OS keychain/credential vault before Private Beta.
    pub fn mask_key(key: &str) -> String {
        if key.is_empty() { return "****".into(); }
        if key.len() <= 8 { return format!("{}****", &key[..key.len().min(4)]); }
        format!("{}****{}", &key[..4], &key[key.len()-4..])
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

    pub async fn get_active(pool: &SqlitePool) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM model_provider_configs WHERE is_active=1 LIMIT 1").fetch_optional(pool).await
    }

    pub async fn upsert(pool: &SqlitePool, r: &sqlx::sqlite::SqliteRow) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let pid: String = r.get("provider_id");
        let kind: String = r.get("kind");
        let bu: String = r.get("base_url");
        let key: String = r.get("api_key_ciphertext");
        let msk: String = r.get("api_key_masked");
        let dm: String = r.get("default_model");
        let fm: String = r.get("fast_model");
        let rm: String = r.get("reasoning_model");
        let sm: String = r.get("structured_output_model");
        let mt: i64 = r.get("max_tokens");
        let temp: f64 = r.get("temperature");
        let to: i64 = r.get("timeout_ms");
        let cost: f64 = r.get("max_cost_per_task_usd");
        sqlx::query("INSERT OR REPLACE INTO model_provider_configs VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&pid).bind(&kind).bind(&bu).bind(&key).bind(&msk).bind(&dm).bind(&fm).bind(&rm).bind(&sm)
            .bind(mt).bind(temp).bind(to).bind(cost).bind(1).bind(now).bind(now).execute(pool).await?;
        // Deactivate others
        sqlx::query("UPDATE model_provider_configs SET is_active=0,updated_at_ms=? WHERE provider_id!=?").bind(now).bind(&pid).execute(pool).await?;
        Ok(())
    }

    pub async fn set_active(pool: &SqlitePool, provider_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE model_provider_configs SET is_active=0,updated_at_ms=?").bind(now).execute(pool).await?;
        sqlx::query("UPDATE model_provider_configs SET is_active=1,updated_at_ms=? WHERE provider_id=?").bind(now).bind(provider_id).execute(pool).await?;
        Ok(())
    }

    pub async fn clear_api_key(pool: &SqlitePool, provider_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE model_provider_configs SET api_key_ciphertext='',api_key_masked='',updated_at_ms=? WHERE provider_id=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(provider_id).execute(pool).await?; Ok(())
    }
}
