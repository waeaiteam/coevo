#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Manager;
use tauri_plugin_shell::ShellExt;

static API_BASE: Mutex<String> = Mutex::new(String::new());
static SIDECAR: Mutex<Option<tauri_plugin_shell::process::CommandChild>> = Mutex::new(None);

fn coevo_home() -> PathBuf {
    if let Ok(h) = std::env::var("COEVO_HOME") {
        return PathBuf::from(h);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".coevo")
}

fn ensure_dirs(home: &PathBuf) {
    for d in &[
        "config",
        "data",
        "logs",
        "runtime",
        "cache",
        "memory",
        "temp",
        "skills/installed",
        "skills/generated",
        "skills/pending",
        "skills/archived",
        "executors/openclaw",
        "executors/hermes",
        "executors/mcp",
        "executors/302ai",
        "executors/local-process",
        "backups/daily",
        "backups/manual",
    ] {
        fs::create_dir_all(home.join(d)).ok();
    }
}

fn find_free_port(start: u16) -> Result<u16, String> {
    for p in start..start + 100 {
        if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() {
            return Ok(p);
        }
    }
    Err(format!("No free port in {}-{}", start, start + 99))
}

fn wait_for_health(port: u16, timeout: Duration) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(200)) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
            if stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .is_ok()
            {
                let mut response = [0_u8; 64];
                if let Ok(n) = stream.read(&mut response) {
                    let status = String::from_utf8_lossy(&response[..n]);
                    if status.starts_with("HTTP/1.1 200") || status.starts_with("HTTP/1.0 200") {
                        return true;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    false
}

fn unix_timestamp_secs() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn start_parent_heartbeat(path: PathBuf) {
    std::thread::spawn(move || loop {
        let _ = fs::write(&path, unix_timestamp_secs());
        std::thread::sleep(Duration::from_secs(1));
    });
}

#[tauri::command]
fn get_coevo_home() -> String {
    coevo_home().to_string_lossy().to_string()
}

#[tauri::command]
fn get_api_base() -> String {
    API_BASE.lock().unwrap().clone()
}

#[tauri::command]
async fn launch_server(app: tauri::AppHandle) -> Result<String, String> {
    {
        let api = API_BASE.lock().unwrap();
        if !api.is_empty() {
            return Ok(api.clone());
        }
    }
    let home = coevo_home();
    ensure_dirs(&home);
    let port = find_free_port(8717)?;
    let db_path = home.join("data").join("coevo.db");
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let log_dir = home.join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let log_path = log_dir.join("coevo-server.log");
    let rt = home.join("runtime");
    fs::create_dir_all(&rt).map_err(|e| e.to_string())?;
    let heartbeat_path = rt.join("desktop.heartbeat");
    fs::write(&heartbeat_path, unix_timestamp_secs()).map_err(|e| e.to_string())?;
    start_parent_heartbeat(heartbeat_path.clone());

    // Open log file synchronously, fail early if can't
    let mut log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Cannot open log file {}: {}", log_path.display(), e))?;

    // Write boot entry
    let _ = writeln!(
        log_file,
        "[boot] Starting coevo-server sidecar at unix_ts={}",
        unix_timestamp_secs()
    );

    let sidecar = app
        .shell()
        .sidecar("coevo-server")
        .map_err(|e| {
            format!(
                "Sidecar 'coevo-server' not found: {}. Run: npm run build:sidecar",
                e
            )
        })?
        .env("COEVO_HOME", home.to_string_lossy().to_string())
        .env("COEVO_PORT", port.to_string())
        .env("COEVO_DB_PATH", db_path.to_string_lossy().to_string())
        .env("COEVO_LOG_DIR", log_dir.to_string_lossy().to_string())
        .env(
            "COEVO_PARENT_HEARTBEAT",
            heartbeat_path.to_string_lossy().to_string(),
        )
        .env("RUST_LOG", "coevo=info");

    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| format!("Failed to spawn coevo-server: {}", e))?;
    let pid = child.pid();
    *SIDECAR.lock().unwrap() = Some(child);

    // Write runtime files
    fs::write(rt.join("server.port"), port.to_string()).ok();
    fs::write(rt.join("server.pid"), pid.to_string()).ok();

    // Async stdout/stderr → log file
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            let line = match event {
                tauri_plugin_shell::process::CommandEvent::Stdout(bytes) => {
                    String::from_utf8_lossy(&bytes).into_owned()
                }
                tauri_plugin_shell::process::CommandEvent::Stderr(bytes) => {
                    String::from_utf8_lossy(&bytes).into_owned()
                }
                tauri_plugin_shell::process::CommandEvent::Terminated(status) => {
                    let _ = writeln!(
                        log_file,
                        "[boot] coevo-server exited with {:?}",
                        status.code
                    );
                    break;
                }
                _ => continue,
            };
            let _ = writeln!(log_file, "{}", line);
        }
    });

    // Wait for /health to return OK, up to 15 seconds.
    let api_base = format!("http://127.0.0.1:{}", port);
    let health_ok = tauri::async_runtime::spawn_blocking(move || {
        wait_for_health(port, Duration::from_secs(15))
    })
    .await
    .map_err(|e| format!("Health check task failed: {}", e))?;
    if health_ok {
        *API_BASE.lock().unwrap() = api_base.clone();
        return Ok(api_base);
    }
    // Server didn't come up — kill sidecar and return error
    if let Some(child) = SIDECAR.lock().unwrap().take() {
        let _ = child.kill();
    }
    Err(format!(
        "coevo-server started but health check failed. Check logs: {}",
        log_path.display()
    ))
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
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&log_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&log_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&log_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(log_dir.to_string_lossy().to_string())
}

#[tauri::command]
fn open_coevo_dir() -> Result<String, String> {
    let home = coevo_home();
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&home)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&home)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&home)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(home.to_string_lossy().to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_coevo_home,
            get_api_base,
            launch_server,
            stop_server,
            open_logs_dir,
            open_coevo_dir
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(child) = SIDECAR.lock().unwrap().take() {
                    let _ = child.kill();
                }
                window.app_handle().exit(0);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running coevo desktop");
}
