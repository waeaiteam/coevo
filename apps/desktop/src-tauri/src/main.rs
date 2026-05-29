#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::path::PathBuf;
use std::process::{Command, Child};
use std::sync::Mutex;
use std::fs;

static SERVER: Mutex<Option<Child>> = Mutex::new(None);
static SERVER_PORT: Mutex<u16> = Mutex::new(0);
static API_BASE: Mutex<String> = Mutex::new(String::new());

fn coevo_home() -> PathBuf {
    if let Ok(h) = std::env::var("COEVO_HOME") { return PathBuf::from(h); }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".coevo")
}

fn ensure_dirs(home: &PathBuf) {
    let dirs = [
        "config","data","logs","runtime","cache","memory","temp",
        "skills/installed","skills/generated","skills/pending","skills/archived",
        "executors/openclaw","executors/hermes","executors/mcp","executors/302ai","executors/local-process",
        "backups/daily","backups/manual",
    ];
    for d in &dirs { fs::create_dir_all(home.join(d)).ok(); }
}

fn find_free_port(start: u16) -> u16 {
    for p in start..start+100 {
        if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() { return p; }
    }
    start
}

fn resolve_server_binary() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidates = [
        exe_dir.join("coevo-server.exe"),
        exe_dir.join("binaries").join("coevo-server.exe"),
        // find with target triple suffix
        std::fs::read_dir(exe_dir.join("binaries")).ok().and_then(|mut d| {
            d.find_map(|e| e.ok().filter(|e| e.file_name().to_string_lossy().starts_with("coevo-server")).map(|e| e.path()))
        }).unwrap_or(PathBuf::new()),
        PathBuf::from("target/release/coevo-server.exe"),
    ];
    candidates.iter().find(|p| p.exists()).cloned()
}

#[tauri::command]
fn get_coevo_home() -> String { coevo_home().to_string_lossy().to_string() }

#[tauri::command]
fn get_server_port() -> u16 {
    let port = *SERVER_PORT.lock().unwrap();
    if port > 0 { return port; }
    // Try to read from runtime file
    let home = coevo_home();
    if let Ok(p) = fs::read_to_string(home.join("runtime").join("server.port")) {
        if let Ok(port) = p.trim().parse() { return port; }
    }
    8717
}

#[tauri::command]
fn get_api_base() -> String { API_BASE.lock().unwrap().clone() }

#[tauri::command]
fn launch_server() -> Result<String, String> {
    // If already running, return current apiBase
    {
        let api = API_BASE.lock().unwrap();
        if !api.is_empty() { return Ok(api.clone()); }
    }
    let home = coevo_home();
    ensure_dirs(&home);
    let port = find_free_port(8717);
    *SERVER_PORT.lock().unwrap() = port;
    let db = home.join("data").join("coevo.db");
    let log_dir = home.join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let server_path = resolve_server_binary().ok_or("coevo-server binary not found")?;
    let log_file = log_dir.join("coevo-server.log");
    let child = Command::new(&server_path)
        .env("COEVO_HOME", &home)
        .env("COEVO_PORT", port.to_string())
        .env("COEVO_DB_PATH", db.to_string_lossy().to_string())
        .env("COEVO_LOG_DIR", log_dir.to_string_lossy().to_string())
        .env("RUST_LOG", "coevo=info")
        .stdout(fs::File::create(&log_file).map_err(|e| e.to_string())?)
        .stderr(fs::File::create(&log_file).map_err(|e| e.to_string())?)
        .spawn().map_err(|e| format!("Failed to start server: {}", e))?;
    // Write runtime files
    let rt = home.join("runtime");
    fs::create_dir_all(&rt).ok();
    fs::write(rt.join("server.port"), port.to_string()).ok();
    fs::write(rt.join("server.pid"), child.id().to_string()).ok();
    let api_base = format!("http://127.0.0.1:{}", port);
    *API_BASE.lock().unwrap() = api_base.clone();
    *SERVER.lock().unwrap() = Some(child);
    Ok(api_base)
}

#[tauri::command]
fn stop_server() {
    if let Some(mut child) = SERVER.lock().unwrap().take() {
        child.kill().ok();
    }
    *API_BASE.lock().unwrap() = String::new();
    *SERVER_PORT.lock().unwrap() = 0;
}

#[tauri::command]
fn open_logs_dir() -> Result<String, String> {
    let log_dir = coevo_home().join("logs");
    fs::create_dir_all(&log_dir).ok();
    #[cfg(target_os = "windows")] { Command::new("explorer").arg(&log_dir).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "macos")] { Command::new("open").arg(&log_dir).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "linux")] { Command::new("xdg-open").arg(&log_dir).spawn().map_err(|e| e.to_string())?; }
    Ok(log_dir.to_string_lossy().to_string())
}

#[tauri::command]
fn open_coevo_dir() -> Result<String, String> {
    let home = coevo_home();
    #[cfg(target_os = "windows")] { Command::new("explorer").arg(&home).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "macos")] { Command::new("open").arg(&home).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "linux")] { Command::new("xdg-open").arg(&home).spawn().map_err(|e| e.to_string())?; }
    Ok(home.to_string_lossy().to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_coevo_home, get_server_port, get_api_base, launch_server, stop_server,
            open_logs_dir, open_coevo_dir
        ])
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(mut child) = SERVER.lock().unwrap().take() { child.kill().ok(); }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running coevo desktop");
}
