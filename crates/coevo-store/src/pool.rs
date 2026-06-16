//! SQLite connection pool management.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Create a SqlitePool from a database URL or raw file path.
/// Supports: `sqlite:path`, `sqlite::memory:`, or plain `/absolute/path/to/file.db`.
pub async fn create_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = if database_url.starts_with("sqlite:") || database_url.starts_with("sqlite::") {
        SqliteConnectOptions::from_str(database_url)?
    } else {
        // Raw file path — create parent dirs
        let path = std::path::Path::new(database_url);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        SqliteConnectOptions::new().filename(path)
    };
    let opts = opts
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
}

/// Create an in-memory pool for testing.
pub async fn create_test_pool() -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_raw_path_with_spaces() {
        let tmp = std::env::temp_dir().join("coevo test raw.db");
        let _ = std::fs::remove_file(&tmp);
        let pool = create_pool(&tmp.to_string_lossy())
            .await
            .expect("raw path with space");
        sqlx::query("CREATE TABLE IF NOT EXISTS test_raw (id INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        let _ = std::fs::remove_file(&tmp);
    }
    #[tokio::test]
    async fn test_raw_path_with_chinese() {
        let tmp = std::env::temp_dir().join("coevo测试中文.db");
        let _ = std::fs::remove_file(&tmp);
        let pool = create_pool(&tmp.to_string_lossy())
            .await
            .expect("raw path with Chinese chars");
        sqlx::query("CREATE TABLE IF NOT EXISTS test_cn (id INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        let _ = std::fs::remove_file(&tmp);
    }
    #[tokio::test]
    async fn test_raw_path_windows_backslash() {
        let tmp = std::env::temp_dir().join("coevo_backslash.db");
        let _ = std::fs::remove_file(&tmp);
        let path = tmp.to_string_lossy().replace('/', "\\");
        let pool = create_pool(&path).await.expect("raw path with backslash");
        sqlx::query("CREATE TABLE IF NOT EXISTS test_bs (id INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        let _ = std::fs::remove_file(&tmp);
    }
    #[tokio::test]
    async fn test_sqlite_memory_works() {
        let pool = create_test_pool().await.unwrap();
        sqlx::query("SELECT 1").execute(&pool).await.unwrap();
    }
}
