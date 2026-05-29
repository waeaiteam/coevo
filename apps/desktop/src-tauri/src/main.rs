#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::path::PathBuf;
use std::process::{Command, Child};
use std::sync::Mutex;

static mut SERVER: Option<Child> = None;
static SERVER_PORT: Mutex<u16> = Mutex::new(8717);

fn coevo_home() -> PathBuf {
    if let Ok(h) = std::env::var("COEVO_HOME") { return PathBuf::from(h); }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".coevo")
}

fn ensure_dirs(home: &PathBuf) {
    let dirs = ["config","data","logs","runtime","cache","temp"];
    for d in &dirs { std::fs::create_dir_all(home.join(d)).ok(); }
}

fn find_free_port(start: u16) -> u16 {
    for p in start..start+100 {
        if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() { return p; }
    }
    start
}

fn start_server(home: &PathBuf, port: u16) -> Option<Child> {
    let db = home.join("data").join("coevo.db");
    let log = home.join("logs").join("coevo-server.log");
    // Try to find coevo-server binary next to the current exe
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let server_path = exe_dir.join("coevo-server.exe");
    let server_path = if server_path.exists() { server_path } else {
        // Fallback: try cargo run in dev
        return None;
    };
    let child = Command::new(&server_path)
        .env("COEVO_HOME", home)
        .env("COEVO_PORT", port.to_string())
        .stdout(std::fs::File::create(&log).ok()?)
        .stderr(std::fs::File::create(&log).ok()?)
        .spawn().ok()?;
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
    let port = find_free_port(8717);
    *SERVER_PORT.lock().unwrap() = port;
    match start_server(&home, port) {
        Some(child) => { unsafe { SERVER = Some(child); } Ok(format!("http://127.0.0.1:{}", port)) }
        None => Err("Server binary not found. Run in dev mode with cargo.".into())
    }
}

#[tauri::command]
fn stop_server() {
    unsafe { if let Some(ref mut s) = SERVER { s.kill().ok(); SERVER = None; } }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_coevo_home, get_server_port, launch_server, stop_server])
        .run(tauri::generate_context!())
        .expect("error while running coevo desktop");
}
