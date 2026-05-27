//! SQLite connection pool management.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Create a SqlitePool from a database URL.
/// Supports both file-based (`sqlite:path`) and in-memory (`sqlite::memory:`).
pub async fn create_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(database_url)?
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
    create_pool("sqlite::memory:").await
}
