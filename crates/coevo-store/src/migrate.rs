//! Embedded SQL migration runner using sqlx migrate.

use sqlx::SqlitePool;

/// Run all pending migrations against the given pool.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations").run(pool).await
}
