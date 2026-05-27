//! Server configuration from environment variables.

use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub database_url: String,
    pub log_level: String,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            bind_addr: env::var("COEVO_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8717".to_string()),
            database_url: env::var("COEVO_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:data/coevo.db?mode=rwc".to_string()),
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "coevo=debug,info".to_string()),
        }
    }
}
