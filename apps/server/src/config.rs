//! Server configuration from environment variables.
//! Supports desktop launch via COEVO_HOME/COEVO_PORT/COEVO_DB_PATH.

use std::env;
use std::path::PathBuf;

pub fn coevo_home() -> PathBuf {
    if let Ok(h) = env::var("COEVO_HOME") {
        return PathBuf::from(h);
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        return PathBuf::from(home).join(".coevo");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".coevo");
    }
    PathBuf::from(".coevo")
}

fn ensure_default_workspace(home: &std::path::Path) {
    let workspace = home.join("workspace");
    std::fs::create_dir_all(&workspace).ok();
    let welcome = workspace.join("welcome.md");
    if !welcome.exists() {
        let content = "# coevo workspace\n\nThis file exists so the first Green WorkOrder can perform a real governed read-only file execution and produce audit evidence.\n";
        std::fs::write(welcome, content).ok();
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
    /// Raw file path or sqlite: URL. pool.rs handles both.
    pub database_url: String,
    pub log_level: String,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let home = coevo_home();
        ensure_default_workspace(&home);

        let bind_addr = if let Ok(addr) = env::var("COEVO_BIND_ADDR") {
            addr
        } else if let Ok(port) = env::var("COEVO_PORT") {
            format!("127.0.0.1:{}", port)
        } else {
            "127.0.0.1:8717".to_string()
        };

        // database: COEVO_DATABASE_URL > COEVO_DB_PATH (raw) > COEVO_HOME/data/coevo.db (raw) > sqlite:data/coevo.db?mode=rwc
        let database_url = if let Ok(url) = env::var("COEVO_DATABASE_URL") {
            url
        } else if let Ok(db_path) = env::var("COEVO_DB_PATH") {
            // Raw file path — pool.rs handles non-sqlite: paths
            let p = PathBuf::from(&db_path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            db_path
        } else {
            let db = home.join("data").join("coevo.db");
            if let Some(parent) = db.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            db.to_string_lossy().to_string()
        };

        let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "coevo=debug,info".to_string());
        Self {
            bind_addr,
            database_url,
            log_level,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());
    #[test]
    fn test_default_db_is_raw_path() {
        let _lock = LOCK.lock().unwrap();
        let config = ServerConfig::from_env();
        assert!(!config.database_url.is_empty());
        assert!(
            !config.database_url.starts_with("sqlite:"),
            "default should be raw path, got: {}",
            config.database_url
        );
    }
    #[test]
    fn test_coevo_db_path_env_raw() {
        let _lock = LOCK.lock().unwrap();
        std::env::set_var("COEVO_DB_PATH", "C:\\Users\\test\\.coevo\\data\\coevo.db");
        let config = ServerConfig::from_env();
        assert!(
            !config.database_url.starts_with("sqlite:"),
            "COEVO_DB_PATH should be raw, got: {}",
            config.database_url
        );
        assert!(config.database_url.contains("coevo.db"));
        std::env::remove_var("COEVO_DB_PATH");
    }
    #[test]
    fn test_coevo_home_seeds_readonly_workspace_file() {
        let _lock = LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("coevo-home-{}", uuid::Uuid::new_v4()));
        std::env::set_var("COEVO_HOME", &home);

        let _config = ServerConfig::from_env();

        let welcome = home.join("workspace").join("welcome.md");
        assert!(welcome.exists(), "expected {} to exist", welcome.display());
        assert!(std::fs::read_to_string(&welcome).unwrap().contains("coevo"));
        std::env::remove_var("COEVO_HOME");
        std::fs::remove_dir_all(home).ok();
    }
}
