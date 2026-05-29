#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::path::PathBuf;
use std::process::{Command, Child};
use std::sync::Mutex;
use std::fs;

static mut SERVER: Option<Child> = None;
static SERVER_PORT: Mutex<u16> = Mutex::new(8717);

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

fn start_server(home: &PathBuf, port: u16, db: &PathBuf, log_dir: &PathBuf) -> Option<Child> {
    let log = log_dir.join("coevo-server.log");
    fs::create_dir_all(log_dir).ok();
    // Find coevo-server sidecar
    let server_path = {
        let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
        let candidates = [
            exe_dir.join("coevo-server.exe"),
            exe_dir.join("binaries/coevo-server-x86_64-pc-windows-msvc.exe"),
            exe_dir.join("binaries/coevo-server.exe"),
            // Dev fallback: cargo build output
            PathBuf::from("target/release/coevo-server.exe"),
            PathBuf::from("../target/release/coevo-server.exe"),
        ];
        candidates.iter().find(|p| p.exists())?.clone()
    };
    let child = Command::new(&server_path)
        .env("COEVO_HOME", home)
        .env("COEVO_DB_PATH", db)
        .env("COEVO_LOG_DIR", log_dir)
        .env("COEVO_PORT", port.to_string())
        .stdout(fs::File::create(&log).ok()?)
        .stderr(fs::File::create(&log).ok()?)
        .spawn().ok()?;
    // Write runtime files
    let rt = home.join("runtime");
    fs::create_dir_all(&rt).ok();
    fs::write(rt.join("server.port"), port.to_string()).ok();
    fs::write(rt.join("server.pid"), child.id().to_string()).ok();
    fs::write(rt.join("health.json"), r#"{"status":"starting"}"#).ok();
    Some(child)
}

#[tauri::command]
fn get_coevo_home() -> String { coevo_home().to_string_lossy().to_string() }

#[tauri::command]
fn get_server_port() -> u16 { *SERVER_PORT.lock().unwrap() }

#[tauri::command]
fn launch_server() -> Result<String, String> {
    let home = coevo_home();
    ensure_dirs(&home);
    let db = home.join("data").join("coevo.db");
    let log_dir = home.join("logs");
    let port = find_free_port(8717);
    *SERVER_PORT.lock().unwrap() = port;
    match start_server(&home, port, &db, &log_dir) {
        Some(child) => { unsafe { SERVER = Some(child); } Ok(format!("http://127.0.0.1:{}", port)) }
        None => Err("Server binary not found at coevo-server.exe".into())
    }
}

#[tauri::command]
fn stop_server() {
    unsafe { if let Some(ref mut s) = SERVER { s.kill().ok(); SERVER = None; } }
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
            get_coevo_home, get_server_port, launch_server, stop_server,
            open_logs_dir, open_coevo_dir
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                unsafe { if let Some(ref mut s) = SERVER { s.kill().ok(); SERVER = None; } }
                let _ = window;
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running coevo desktop");
}
