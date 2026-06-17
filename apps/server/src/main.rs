//! coevo-server: axum HTTP API entrypoint for the coevo control plane.

use axum::http::{HeaderValue, Method};
use std::env;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, SystemTime};

use coevo_server::config::coevo_home;
use coevo_server::config::ServerConfig;
use coevo_server::handlers::mcp::sync_enabled_mcp_servers;
use coevo_server::router::build_router;
use coevo_server::state::AppState;
use coevo_store::migrate::{
    create_pool_and_run_migrations_with_recovery, MigrationRecoveryOutcome,
};
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};
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
    let state = AppState::new(pool, coevo_home());
    if let Err(err) = sync_enabled_mcp_servers(&state).await {
        tracing::warn!(error = %err, "failed to sync enabled MCP servers at startup");
    }

    let cors = build_cors_layer();

    // Build router
    let app = build_router(state).layer(cors);

    // Bind and serve
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("coevo server listening on http://{}", config.bind_addr);
    tracing::info!("API docs: http://{}/docs", config.bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}

fn build_cors_layer() -> CorsLayer {
    let extra_origins = env::var("COEVO_ALLOWED_ORIGINS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let allow_origin = AllowOrigin::predicate(move |origin: &HeaderValue, _| {
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        is_default_allowed_origin(origin)
            || extra_origins
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(origin))
    });

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(AllowHeaders::mirror_request())
}

fn is_default_allowed_origin(origin: &str) -> bool {
    matches!(
        origin,
        "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"
    ) || (cfg!(debug_assertions)
        && matches!(origin, "http://localhost:5173" | "http://127.0.0.1:5173"))
}

#[cfg(test)]
mod tests {
    use super::is_default_allowed_origin;

    #[test]
    fn default_cors_allows_tauri_and_debug_dev_shell_origins() {
        assert!(is_default_allowed_origin("tauri://localhost"));
        assert!(is_default_allowed_origin("http://tauri.localhost"));
        assert!(is_default_allowed_origin("http://127.0.0.1:5173"));
        assert!(is_default_allowed_origin("http://localhost:5173"));
        assert!(!is_default_allowed_origin("http://localhost:3000"));
    }
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
