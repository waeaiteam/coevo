use coevo_models::types::*;
use sqlx::{Row, SqlitePool};

const CREDENTIAL_SERVICE: &str = "coevo:model-provider";
const KEYRING_PREFIX: &str = "keyring:";

pub struct ModelConfigRepo;
impl ModelConfigRepo {
    pub fn mask_key(key: &str) -> String {
        if key.is_empty() { return "****".into(); }
        if key.len() <= 8 { return format!("{}****", &key[..key.len().min(4)]); }
        format!("{}****{}", &key[..4], &key[key.len()-4..])
    }

    fn init_credential_store() -> Result<(), sqlx::Error> {
        if keyring_core::get_default_store().is_none() {
            #[cfg(target_os = "windows")]
            {
                let store = windows_native_keyring_store::Store::new()
                    .map_err(|e| sqlx::Error::Protocol(format!("CREDENTIAL_VAULT_UNAVAILABLE: {}", e)))?;
                keyring_core::set_default_store(store);
            }
            #[cfg(not(target_os = "windows"))]
            {
                return Err(sqlx::Error::Protocol(
                    "CREDENTIAL_VAULT_UNAVAILABLE: native keyring store is not configured for this platform".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn credential_ref(provider_id: &str) -> String {
        format!("{}{}:{}", KEYRING_PREFIX, CREDENTIAL_SERVICE, provider_id)
    }

    fn credential_user(provider_id: &str) -> String {
        provider_id.to_string()
    }

    fn store_api_key(provider_id: &str, api_key: &str) -> Result<String, sqlx::Error> {
        if api_key.is_empty() {
            return Ok(String::new());
        }
        Self::init_credential_store()?;
        let entry = keyring_core::Entry::new(CREDENTIAL_SERVICE, &Self::credential_user(provider_id))
            .map_err(|e| sqlx::Error::Protocol(format!("CREDENTIAL_VAULT_ENTRY_FAILED: {}", e)))?;
        entry
            .set_password(api_key)
            .map_err(|e| sqlx::Error::Protocol(format!("CREDENTIAL_VAULT_WRITE_FAILED: {}", e)))?;
        Ok(Self::credential_ref(provider_id))
    }

    fn resolve_api_key(stored: String, provider_id: &str) -> Result<String, sqlx::Error> {
        if !stored.starts_with(KEYRING_PREFIX) {
            // Backward compatibility for existing Alpha databases. New writes never use this path.
            return Ok(stored);
        }
        Self::init_credential_store()?;
        let entry = keyring_core::Entry::new(CREDENTIAL_SERVICE, &Self::credential_user(provider_id))
            .map_err(|e| sqlx::Error::Protocol(format!("CREDENTIAL_VAULT_ENTRY_FAILED: {}", e)))?;
        entry
            .get_password()
            .map_err(|e| sqlx::Error::Protocol(format!("CREDENTIAL_VAULT_READ_FAILED: {}", e)))
    }

    fn delete_api_key(provider_id: &str) -> Result<(), sqlx::Error> {
        Self::init_credential_store()?;
        let entry = keyring_core::Entry::new(CREDENTIAL_SERVICE, &Self::credential_user(provider_id))
            .map_err(|e| sqlx::Error::Protocol(format!("CREDENTIAL_VAULT_ENTRY_FAILED: {}", e)))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(sqlx::Error::Protocol(format!("CREDENTIAL_VAULT_DELETE_FAILED: {}", e))),
        }
    }

    fn parse_config(row: &sqlx::sqlite::SqliteRow) -> Result<ModelProviderConfig, sqlx::Error> {
        let kind_str: String = row.get("kind");
        let kind = Self::parse_provider_kind(&kind_str)?;
        let provider_id: String = row.get("provider_id");
        let stored_key: String = row.get("api_key_ciphertext");
        let api_key = Self::resolve_api_key(stored_key, &provider_id)?;
        Ok(ModelProviderConfig {
            provider_id,
            kind,
            base_url: row.get("base_url"),
            api_key,
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

    fn parse_provider_kind(kind: &str) -> Result<ModelProviderKind, sqlx::Error> {
        match kind {
            "Mock" => Ok(ModelProviderKind::Mock),
            "OpenAICompatible" => Ok(ModelProviderKind::OpenAICompatible),
            "OpenAI" => Ok(ModelProviderKind::OpenAI),
            "Anthropic" => Ok(ModelProviderKind::Anthropic),
            "Gemini" => Ok(ModelProviderKind::Gemini),
            "DeepSeek" => Ok(ModelProviderKind::DeepSeek),
            "Ollama" => Ok(ModelProviderKind::Ollama),
            "Local" => Ok(ModelProviderKind::Local),
            _ => Err(sqlx::Error::Protocol(format!("MODEL_CONFIG_INVALID_KIND: {}", kind))),
        }
    }

    pub async fn seed_mock_if_empty(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let count: i64 = sqlx::query("SELECT COUNT(*) as c FROM model_provider_configs").fetch_one(pool).await?.get("c");
        if count == 0 {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query("INSERT INTO model_provider_configs VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind("mock-default").bind("Mock").bind("").bind("").bind("").bind("mock-model")
                .bind("mock-model").bind("mock-model").bind("mock-model").bind(4096).bind(0.7)
                .bind(30000).bind(0.0).bind(0).bind(now).bind(now).execute(pool).await?;
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
        let key_ref = Self::store_api_key(pid, api_key)?;
        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,1,?,?) ON CONFLICT(provider_id) DO UPDATE SET kind=excluded.kind,base_url=excluded.base_url,api_key_ciphertext=excluded.api_key_ciphertext,api_key_masked=excluded.api_key_masked,default_model=excluded.default_model,fast_model=excluded.fast_model,reasoning_model=excluded.reasoning_model,structured_output_model=excluded.structured_output_model,max_tokens=excluded.max_tokens,temperature=excluded.temperature,timeout_ms=excluded.timeout_ms,max_cost_per_task_usd=excluded.max_cost_per_task_usd,is_active=1,updated_at_ms=excluded.updated_at_ms")
            .bind(pid).bind(kind_str).bind(base_url).bind(key_ref).bind(masked).bind(default_model).bind(fast_model).bind(reasoning_model).bind(structured_model)
            .bind(max_tokens).bind(temperature).bind(timeout_ms).bind(max_cost).bind(now).bind(now).execute(pool).await?;
        Ok(())
    }
    pub async fn deactivate_others(pool: &SqlitePool, provider_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE model_provider_configs SET is_active=0,updated_at_ms=? WHERE provider_id!=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(provider_id).execute(pool).await?; Ok(())
    }
    pub async fn clear_api_key(pool: &SqlitePool, provider_id: &str) -> Result<(), sqlx::Error> {
        Self::delete_api_key(provider_id)?;
        sqlx::query("UPDATE model_provider_configs SET api_key_ciphertext='',api_key_masked='',updated_at_ms=? WHERE provider_id=?")
            .bind(chrono::Utc::now().timestamp_millis()).bind(provider_id).execute(pool).await?; Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{migrate::run_migrations, pool::create_test_pool};
    use std::sync::Mutex;

    static KEYRING_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn upsert_config_stores_api_key_in_keyring_not_sqlite_plaintext() {
        let _lock = KEYRING_LOCK.lock().unwrap();
        keyring_core::set_default_store(keyring_core::sample::Store::new().unwrap());
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        ModelConfigRepo::upsert_config(
            &pool,
            "desktop",
            "OpenAICompatible",
            "https://api.openai.com/v1",
            "sk-secret-value",
            &ModelConfigRepo::mask_key("sk-secret-value"),
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4o",
            "gpt-4o",
            4096,
            0.7,
            30000,
            5.0,
        )
        .await
        .unwrap();

        let stored_ref: String = sqlx::query_scalar(
            "SELECT api_key_ciphertext FROM model_provider_configs WHERE provider_id='desktop'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_ne!(stored_ref, "sk-secret-value");
        assert!(stored_ref.starts_with("keyring:"));

        let config = ModelConfigRepo::get_active_config(&pool)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(config.api_key, "sk-secret-value");
        keyring_core::unset_default_store();
    }

    #[tokio::test]
    async fn clear_api_key_removes_keyring_secret_and_db_reference() {
        let _lock = KEYRING_LOCK.lock().unwrap();
        keyring_core::set_default_store(keyring_core::sample::Store::new().unwrap());
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        ModelConfigRepo::upsert_config(
            &pool,
            "desktop-clear",
            "OpenAICompatible",
            "https://api.openai.com/v1",
            "sk-clear-me",
            &ModelConfigRepo::mask_key("sk-clear-me"),
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4o",
            "gpt-4o",
            4096,
            0.7,
            30000,
            5.0,
        )
        .await
        .unwrap();

        ModelConfigRepo::clear_api_key(&pool, "desktop-clear")
            .await
            .unwrap();

        let stored_ref: String = sqlx::query_scalar(
            "SELECT api_key_ciphertext FROM model_provider_configs WHERE provider_id='desktop-clear'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(stored_ref.is_empty());
        let entry = keyring_core::Entry::new(CREDENTIAL_SERVICE, "desktop-clear").unwrap();
        assert!(matches!(entry.get_password(), Err(keyring_core::Error::NoEntry)));
        keyring_core::unset_default_store();
    }

    #[tokio::test]
    async fn legacy_plaintext_api_key_rows_remain_readable() {
        let _lock = KEYRING_LOCK.lock().unwrap();
        keyring_core::unset_default_store();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query("INSERT INTO model_provider_configs (provider_id,kind,base_url,api_key_ciphertext,api_key_masked,default_model,fast_model,reasoning_model,structured_output_model,max_tokens,temperature,timeout_ms,max_cost_per_task_usd,is_active,created_at_ms,updated_at_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind("legacy-desktop")
            .bind("OpenAICompatible")
            .bind("https://api.openai.com/v1")
            .bind("sk-legacy-plaintext")
            .bind("sk-l****text")
            .bind("gpt-4o")
            .bind("gpt-4o-mini")
            .bind("gpt-4o")
            .bind("gpt-4o")
            .bind(4096)
            .bind(0.7)
            .bind(30000)
            .bind(5.0)
            .bind(1)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        let config = ModelConfigRepo::get_active_config(&pool)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(config.provider_id, "legacy-desktop");
        assert_eq!(config.api_key, "sk-legacy-plaintext");
    }

    #[tokio::test]
    async fn empty_api_key_writes_empty_db_reference_without_keyring_store() {
        let _lock = KEYRING_LOCK.lock().unwrap();
        keyring_core::unset_default_store();
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        ModelConfigRepo::upsert_config(
            &pool,
            "dev-mock",
            "Mock",
            "",
            "",
            "",
            "mock-model",
            "mock-model",
            "mock-model",
            "mock-model",
            4096,
            0.7,
            30000,
            0.0,
        )
        .await
        .unwrap();

        let stored_ref: String = sqlx::query_scalar(
            "SELECT api_key_ciphertext FROM model_provider_configs WHERE provider_id='dev-mock'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored_ref, "");
        assert!(keyring_core::get_default_store().is_none());
    }

    #[tokio::test]
    async fn reupserting_provider_replaces_keyring_secret() {
        let _lock = KEYRING_LOCK.lock().unwrap();
        keyring_core::set_default_store(keyring_core::sample::Store::new().unwrap());
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        for key in ["sk-first-secret", "sk-second-secret"] {
            ModelConfigRepo::upsert_config(
                &pool,
                "desktop-replace",
                "OpenAICompatible",
                "https://api.openai.com/v1",
                key,
                &ModelConfigRepo::mask_key(key),
                "gpt-4o",
                "gpt-4o-mini",
                "gpt-4o",
                "gpt-4o",
                4096,
                0.7,
                30000,
                5.0,
            )
            .await
            .unwrap();
        }

        let entry = keyring_core::Entry::new(CREDENTIAL_SERVICE, "desktop-replace").unwrap();
        assert_eq!(entry.get_password().unwrap(), "sk-second-secret");
        let config = ModelConfigRepo::get_active_config(&pool)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(config.api_key, "sk-second-secret");
        keyring_core::unset_default_store();
    }

    #[tokio::test]
    async fn seed_mock_if_empty_keeps_mock_provider_inactive() {
        let pool = create_test_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();

        ModelConfigRepo::seed_mock_if_empty(&pool).await.unwrap();

        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM model_provider_configs WHERE kind='Mock' AND is_active=1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(active_count, 0);
        assert!(ModelConfigRepo::get_active_config(&pool).await.unwrap().is_none());
    }
}
