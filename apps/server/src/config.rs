//! Server configuration from environment variables.
//! Supports desktop launch via COEVO_HOME/COEVO_PORT/COEVO_DB_PATH.

use std::env;
use std::path::PathBuf;

fn coevo_home() -> PathBuf {
    if let Ok(h) = env::var("COEVO_HOME") { return PathBuf::from(h); }
    if let Ok(home) = std::env::var("USERPROFILE") { return PathBuf::from(home).join(".coevo"); }
    if let Ok(home) = std::env::var("HOME") { return PathBuf::from(home).join(".coevo"); }
    PathBuf::from(".coevo")
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub database_url: String,
    pub log_level: String,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let home = coevo_home();

        // bind_addr: COEVO_BIND_ADDR > COEVO_PORT > 127.0.0.1:8717
        let bind_addr = if let Ok(addr) = env::var("COEVO_BIND_ADDR") {
            addr
        } else if let Ok(port) = env::var("COEVO_PORT") {
            format!("127.0.0.1:{}", port)
        } else {
            "127.0.0.1:8717".to_string()
        };

        // database: COEVO_DATABASE_URL > COEVO_DB_PATH > COEVO_HOME/data/coevo.db
        let database_url = if let Ok(url) = env::var("COEVO_DATABASE_URL") {
            url
        } else if let Ok(db_path) = env::var("COEVO_DB_PATH") {
            format!("sqlite:{}?mode=rwc", db_path)
        } else {
            let db = home.join("data").join("coevo.db");
            std::fs::create_dir_all(db.parent().unwrap()).ok();
            format!("sqlite:{}?mode=rwc", db.display())
        };

        let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "coevo=debug,info".to_string());

        Self { bind_addr, database_url, log_level }
    }
}
