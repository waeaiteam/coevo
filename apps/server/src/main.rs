//! coevo-server: axum HTTP API entrypoint for the coevo control plane.

use std::env;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, SystemTime};

use coevo_server::config::ServerConfig;
use coevo_server::router::build_router;
use coevo_server::state::AppState;
use coevo_store::migrate::{
    create_pool_and_run_migrations_with_recovery, MigrationRecoveryOutcome,
};
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_env();
    start_parent_watchdog();

    let backup_root = coevo_recovery_backup_root();
    let (pool, migration_outcome) =
        create_pool_and_run_migrations_with_recovery(&config.database_url, &backup_root).await?;
    tracing::info!("Database connected: {}", config.database_url);
    match migration_outcome {
        MigrationRecoveryOutcome::Applied => tracing::info!("Migrations applied"),
        MigrationRecoveryOutcome::Recovered {
            version,
            backup_dir,
        } => tracing::warn!(
            "Migration version mismatch at version {}; backed up old database to {} and recreated a clean local database",
            version,
            backup_dir.display()
        ),
    }

    // Handle --migrate flag
    if env::args().any(|a| a == "--migrate") {
        tracing::info!("Migrations complete");
        return Ok(());
    }

    // Build state
    let state = AppState::new(pool);

    // CORS (allow desktop app)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = build_router(state).layer(cors);

    // Bind and serve
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("coevo server listening on http://{}", config.bind_addr);
    tracing::info!("API docs: http://{}/docs", config.bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}

fn coevo_recovery_backup_root() -> PathBuf {
    let home = env::var("COEVO_HOME")
        .map(PathBuf::from)
        .or_else(|_| env::var("USERPROFILE").map(|h| PathBuf::from(h).join(".coevo")))
        .or_else(|_| env::var("HOME").map(|h| PathBuf::from(h).join(".coevo")))
        .unwrap_or_else(|_| PathBuf::from(".coevo"));
    home.join("backups").join("migration-recovery")
}

fn start_parent_watchdog() {
    let Ok(path) = env::var("COEVO_PARENT_HEARTBEAT") else {
        tracing::debug!("COEVO_PARENT_HEARTBEAT not set; parent watchdog disabled");
        return;
    };
    let heartbeat = PathBuf::from(path);
    tracing::info!("parent heartbeat watchdog enabled: {}", heartbeat.display());

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            match std::fs::metadata(&heartbeat)
                .and_then(|m| m.modified())
                .and_then(|modified| {
                    SystemTime::now()
                        .duration_since(modified)
                        .map_err(std::io::Error::other)
                }) {
                Ok(age) if age <= Duration::from_secs(5) => {}
                Ok(age) => {
                    tracing::info!(
                        "parent heartbeat stale at {} for {:?}; shutting down coevo sidecar",
                        heartbeat.display(),
                        age
                    );
                    process::exit(0);
                }
                Err(e) => {
                    tracing::warn!(
                        "parent heartbeat unreadable at {}: {}; shutting down coevo sidecar",
                        heartbeat.display(),
                        e
                    );
                    process::exit(0);
                }
            }
        }
    });
}
