//! coevo-server: axum HTTP API entrypoint for the coevo control plane.

use std::env;

use coevo_server::config::ServerConfig;
use coevo_server::router::build_router;
use coevo_server::state::AppState;
use coevo_store::migrate::run_migrations;
use coevo_store::pool::create_pool;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_env();

    // Handle --migrate flag
    if env::args().any(|a| a == "--migrate") {
        let pool = create_pool(&config.database_url).await?;
        run_migrations(&pool).await?;
        tracing::info!("Migrations complete");
        return Ok(());
    }

    // Create database pool
    let pool = create_pool(&config.database_url).await?;
    tracing::info!("Database connected: {}", config.database_url);

    // Run migrations
    run_migrations(&pool).await?;
    tracing::info!("Migrations applied");

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
