#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::path::PathBuf;
use std::sync::Mutex;
use std::fs;
use tauri::Manager;
use tauri_plugin_shell::ShellExt;

static API_BASE: Mutex<String> = Mutex::new(String::new());
static SIDECAR_PID: Mutex<Option<u32>> = Mutex::new(None);

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

fn find_free_port(start: u16) -> Result<u16, String> {
    for p in start..start+100 {
        if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() { return Ok(p); }
    }
    Err(format!("No free port found in range {}-{}", start, start+99))
}

#[tauri::command]
fn get_coevo_home() -> String { coevo_home().to_string_lossy().to_string() }

#[tauri::command]
fn get_api_base() -> String { API_BASE.lock().unwrap().clone() }

#[tauri::command]
async fn launch_server(app: tauri::AppHandle) -> Result<String, String> {
    {
        let api = API_BASE.lock().unwrap();
        if !api.is_empty() { return Ok(api.clone()); }
    }
    let home = coevo_home();
    ensure_dirs(&home);
    let port = find_free_port(8717)?;
    let db_path = home.join("data").join("coevo.db");
    if let Some(parent) = db_path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let log_dir = home.join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;

    // Use Tauri sidecar API
    let sidecar = app.shell().sidecar("coevo-server")
        .map_err(|e| format!("Sidecar not found: {}. Is coevo-server built and in binaries/? {}", e, "Run npm run build:sidecar"))?
        .env("COEVO_HOME", home.to_string_lossy().to_string())
        .env("COEVO_PORT", port.to_string())
        .env("COEVO_DB_PATH", db_path.to_string_lossy().to_string())
        .env("COEVO_LOG_DIR", log_dir.to_string_lossy().to_string())
        .env("RUST_LOG", "coevo=info");

    let (mut rx, child) = sidecar.spawn().map_err(|e| format!("Failed to spawn sidecar: {}", e))?;
    let pid = child.pid();
    *SIDECAR_PID.lock().unwrap() = Some(pid);

    // Write runtime files
    let rt = home.join("runtime");
    fs::create_dir_all(&rt).ok();
    fs::write(rt.join("server.port"), port.to_string()).ok();
    fs::write(rt.join("server.pid"), pid.to_string()).ok();

    // Spawn async reader for sidecar stdout/stderr → log
    let log_file = log_dir.join("coevo-server.log");
    tauri::async_runtime::spawn(async move {
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&log_file).unwrap();
        use std::io::Write;
        while let Some(event) = rx.recv().await {
            match event {
                tauri_plugin_shell::process::CommandEvent::Stdout(line) => {
                    writeln!(file, "[stdout] {}", String::from_utf8_lossy(&line)).ok();
                }
                tauri_plugin_shell::process::CommandEvent::Stderr(line) => {
                    writeln!(file, "[stderr] {}", String::from_utf8_lossy(&line)).ok();
                }
                tauri_plugin_shell::process::CommandEvent::Terminated(_) => break,
                _ => {}
            }
        }
    });

    let api_base = format!("http://127.0.0.1:{}", port);
    *API_BASE.lock().unwrap() = api_base.clone();
    Ok(api_base)
}

#[tauri::command]
fn stop_server(app: tauri::AppHandle) {
    if let Some(pid) = SIDECAR_PID.lock().unwrap().take() {
        // Kill by PID on Windows
        #[cfg(target_os = "windows")]
        { let _ = std::process::Command::new("taskkill").args(["/F","/PID",&pid.to_string()]).output(); }
        #[cfg(not(target_os = "windows"))]
        { unsafe { libc::kill(pid as i32, libc::SIGTERM); } }
    }
    *API_BASE.lock().unwrap() = String::new();
    let _ = app;
}

#[tauri::command]
fn open_logs_dir() -> Result<String, String> {
    let log_dir = coevo_home().join("logs");
    fs::create_dir_all(&log_dir).ok();
    #[cfg(target_os = "windows")] { std::process::Command::new("explorer").arg(&log_dir).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "macos")] { std::process::Command::new("open").arg(&log_dir).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "linux")] { std::process::Command::new("xdg-open").arg(&log_dir).spawn().map_err(|e| e.to_string())?; }
    Ok(log_dir.to_string_lossy().to_string())
}

#[tauri::command]
fn open_coevo_dir() -> Result<String, String> {
    let home = coevo_home();
    #[cfg(target_os = "windows")] { std::process::Command::new("explorer").arg(&home).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "macos")] { std::process::Command::new("open").arg(&home).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "linux")] { std::process::Command::new("xdg-open").arg(&home).spawn().map_err(|e| e.to_string())?; }
    Ok(home.to_string_lossy().to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_coevo_home, get_api_base, launch_server, stop_server,
            open_logs_dir, open_coevo_dir
        ])
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(pid) = SIDECAR_PID.lock().unwrap().take() {
                    #[cfg(target_os = "windows")]
                    { let _ = std::process::Command::new("taskkill").args(["/F","/PID",&pid.to_string()]).output(); }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running coevo desktop");
}
