//! Embedded SQL migration runner using sqlx migrate.

use crate::pool::create_pool;
use sqlx::migrate::MigrateError;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationRecoveryOutcome {
    Applied,
    Recovered { version: i64, backup_dir: PathBuf },
}

/// Run all pending migrations against the given pool.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| sqlx::Error::Configuration(e.into()))
}

pub async fn create_pool_and_run_migrations_with_recovery(
    database_url: &str,
    backup_root: &Path,
) -> Result<(SqlitePool, MigrationRecoveryOutcome), sqlx::Error> {
    let pool = create_pool(database_url).await?;
    match run_migrations(&pool).await {
        Ok(()) => Ok((pool, MigrationRecoveryOutcome::Applied)),
        Err(error) => {
            let Some(version) = migration_version_mismatch(&error) else {
                return Err(error);
            };
            let Some(db_path) = recoverable_database_path(database_url) else {
                return Err(error);
            };

            pool.close().await;
            let backup_dir =
                with_windows_file_retry(|| backup_database_files(&db_path, backup_root))?;
            with_windows_file_retry(|| remove_database_files(&db_path))?;

            let recovered_pool = create_pool(database_url).await?;
            run_migrations(&recovered_pool).await?;
            Ok((
                recovered_pool,
                MigrationRecoveryOutcome::Recovered {
                    version,
                    backup_dir,
                },
            ))
        }
    }
}

fn migration_version_mismatch(error: &sqlx::Error) -> Option<i64> {
    match error {
        sqlx::Error::Configuration(source) => {
            let migrate_error = source.downcast_ref::<MigrateError>()?;
            match migrate_error {
                MigrateError::VersionMismatch(version) => Some(*version),
                _ => None,
            }
        }
        sqlx::Error::Migrate(source) => match source.as_ref() {
            MigrateError::VersionMismatch(version) => Some(*version),
            _ => None,
        },
        _ => None,
    }
}

fn recoverable_database_path(database_url: &str) -> Option<PathBuf> {
    if database_url == "sqlite::memory:" || database_url == ":memory:" {
        return None;
    }

    if database_url.starts_with("sqlite:") {
        let options = SqliteConnectOptions::from_str(database_url).ok()?;
        let path = options.get_filename();
        if path.as_os_str().is_empty() || path.as_os_str() == ":memory:" {
            return None;
        }
        return Some(path.to_path_buf());
    }

    Some(PathBuf::from(database_url))
}

fn backup_database_files(db_path: &Path, backup_root: &Path) -> Result<PathBuf, sqlx::Error> {
    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S%3f");
    let backup_dir = backup_root.join(format!("recovered-{stamp}"));
    std::fs::create_dir_all(&backup_dir).map_err(sqlx::Error::Io)?;

    for path in database_file_set(db_path) {
        if path.exists() {
            let Some(file_name) = path.file_name() else {
                continue;
            };
            std::fs::copy(&path, backup_dir.join(file_name)).map_err(sqlx::Error::Io)?;
        }
    }

    Ok(backup_dir)
}

fn remove_database_files(db_path: &Path) -> Result<(), sqlx::Error> {
    for path in database_file_set(db_path) {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(sqlx::Error::Io(e)),
        }
    }
    Ok(())
}

fn with_windows_file_retry<T>(
    mut operation: impl FnMut() -> Result<T, sqlx::Error>,
) -> Result<T, sqlx::Error> {
    let mut last_error = None;
    for _ in 0..10 {
        match operation() {
            Ok(value) => return Ok(value),
            Err(sqlx::Error::Io(e)) if is_transient_file_lock(&e) => {
                last_error = Some(e);
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    }

    Err(sqlx::Error::Io(last_error.unwrap_or_else(|| {
        std::io::Error::other("transient SQLite file lock did not clear")
    })))
}

fn is_transient_file_lock(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32) | Some(33))
        || matches!(
            error.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
        )
}

fn database_file_set(db_path: &Path) -> [PathBuf; 3] {
    [
        db_path.to_path_buf(),
        sidecar_sqlite_path(db_path, "wal"),
        sidecar_sqlite_path(db_path, "shm"),
    ]
}

fn sidecar_sqlite_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut os = db_path.as_os_str().to_os_string();
    os.push(format!("-{suffix}"));
    PathBuf::from(os)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::create_pool;
    use sqlx::Row;

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("coevo-{name}-{}", uuid::Uuid::new_v4()))
    }

    async fn write_legacy_mismatched_database(path: &std::path::Path) {
        let pool = create_pool(&path.to_string_lossy()).await.unwrap();

        sqlx::query(
            r#"
CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL,
    checksum BLOB NOT NULL,
    execution_time BIGINT NOT NULL
);
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (1, 'legacy contracts', true, x'00', 1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE legacy_marker (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO legacy_marker (value) VALUES ('old user data')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn migration_mismatch_recovery_backs_up_old_db_and_recreates_clean_db() {
        let root = unique_temp_dir("migration-recovery");
        let db_path = root.join("data").join("coevo.db");
        write_legacy_mismatched_database(&db_path).await;

        let backup_root = root.join("backups").join("migration-recovery");
        let (_pool, outcome) =
            create_pool_and_run_migrations_with_recovery(&db_path.to_string_lossy(), &backup_root)
                .await
                .unwrap();

        let MigrationRecoveryOutcome::Recovered { backup_dir, .. } = outcome else {
            panic!("expected recovery outcome");
        };
        assert!(backup_dir.starts_with(&backup_root));
        let backed_up_db = backup_dir.join("coevo.db");
        assert!(
            backed_up_db.exists(),
            "expected backup at {}",
            backed_up_db.display()
        );

        let backup_pool = create_pool(&backed_up_db.to_string_lossy()).await.unwrap();
        let row = sqlx::query("SELECT value FROM legacy_marker")
            .fetch_one(&backup_pool)
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("value"), "old user data");
        backup_pool.close().await;

        let clean_pool = create_pool(&db_path.to_string_lossy()).await.unwrap();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM contracts")
            .fetch_one(&clean_pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
        assert!(
            sqlx::query("SELECT value FROM legacy_marker")
                .fetch_optional(&clean_pool)
                .await
                .is_err(),
            "legacy table should only exist in backup"
        );
        clean_pool.close().await;

        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn fresh_file_database_migrates_without_recovery_backup() {
        let root = unique_temp_dir("fresh-migration");
        let db_path = root.join("data").join("coevo.db");
        let backup_root = root.join("backups").join("migration-recovery");

        let (_pool, outcome) =
            create_pool_and_run_migrations_with_recovery(&db_path.to_string_lossy(), &backup_root)
                .await
                .unwrap();

        assert_eq!(outcome, MigrationRecoveryOutcome::Applied);
        assert!(
            !backup_root.exists(),
            "fresh migration should not create recovery backups"
        );

        std::fs::remove_dir_all(root).ok();
    }
}
