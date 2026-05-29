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
    let opts = opts.create_if_missing(true).foreign_keys(true).journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
}

/// Create an in-memory pool for testing.
pub async fn create_test_pool() -> Result<SqlitePool, sqlx::Error> {
    create_pool("sqlite::memory:").await
}
