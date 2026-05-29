#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::path::PathBuf;
use std::sync::Mutex;
use std::fs;
use std::io::Write;
use tauri_plugin_shell::ShellExt;

static API_BASE: Mutex<String> = Mutex::new(String::new());
static SIDECAR: Mutex<Option<tauri_plugin_shell::process::CommandChild>> = Mutex::new(None);

fn coevo_home() -> PathBuf {
    if let Ok(h) = std::env::var("COEVO_HOME") { return PathBuf::from(h); }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".coevo")
}

fn ensure_dirs(home: &PathBuf) {
    for d in &["config","data","logs","runtime","cache","memory","temp","skills/installed","skills/generated","skills/pending","skills/archived","executors/openclaw","executors/hermes","executors/mcp","executors/302ai","executors/local-process","backups/daily","backups/manual"] {
        fs::create_dir_all(home.join(d)).ok();
    }
}

fn find_free_port(start: u16) -> Result<u16, String> {
    for p in start..start+100 {
        if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() { return Ok(p); }
    }
    Err(format!("No free port in {}-{}", start, start+99))
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
    let log_path = log_dir.join("coevo-server.log");

    // Open log file synchronously, fail early if can't
    let mut log_file = fs::OpenOptions::new().create(true).append(true).open(&log_path)
        .map_err(|e| format!("Cannot open log file {}: {}", log_path.display(), e))?;

    // Write boot entry
    writeln!(log_file, "[boot] Starting coevo-server sidecar at {}", chrono::Utc::now()).ok();

    let sidecar = app.shell().sidecar("coevo-server")
        .map_err(|e| format!("Sidecar 'coevo-server' not found: {}. Run: npm run build:sidecar", e))?
        .env("COEVO_HOME", home.to_string_lossy().to_string())
        .env("COEVO_PORT", port.to_string())
        .env("COEVO_DB_PATH", db_path.to_string_lossy().to_string())
        .env("COEVO_LOG_DIR", log_dir.to_string_lossy().to_string())
        .env("RUST_LOG", "coevo=info");

    let (mut rx, child) = sidecar.spawn()
        .map_err(|e| format!("Failed to spawn coevo-server: {}", e))?;
    let pid = child.pid();
    *SIDECAR.lock().unwrap() = Some(child);

    // Write runtime files
    let rt = home.join("runtime");
    fs::create_dir_all(&rt).ok();
    fs::write(rt.join("server.port"), port.to_string()).ok();
    fs::write(rt.join("server.pid"), pid.to_string()).ok();

    // Async stdout/stderr → log file
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            let line = match event {
                tauri_plugin_shell::process::CommandEvent::Stdout(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                tauri_plugin_shell::process::CommandEvent::Stderr(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                tauri_plugin_shell::process::CommandEvent::Terminated(status) => {
                    let _ = writeln!(log_file, "[boot] coevo-server exited with {:?}", status.code());
                    break;
                }
                _ => continue,
            };
            let _ = writeln!(log_file, "{}", line);
        }
    });

    // Wait for TCP to become available (health check), up to 15 seconds
    let api_base = format!("http://127.0.0.1:{}", port);
    for _ in 0..30 {
        if reqwest::get(format!("{}/health", api_base)).await.is_ok() {
            *API_BASE.lock().unwrap() = api_base.clone();
            return Ok(api_base);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    // Server didn't come up — kill sidecar and return error
    if let Some(child) = SIDECAR.lock().unwrap().take() {
        let _ = child.kill();
    }
    Err(format!("coevo-server started but health check failed. Check logs: {}", log_path.display()))
}

#[tauri::command]
fn stop_server() {
    if let Some(child) = SIDECAR.lock().unwrap().take() {
        let _ = child.kill();
    }
    *API_BASE.lock().unwrap() = String::new();
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
                if let Some(child) = SIDECAR.lock().unwrap().take() {
                    let _ = child.kill();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running coevo desktop");
}
