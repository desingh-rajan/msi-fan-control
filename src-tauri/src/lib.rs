use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{CpuRefreshKind, System};
use tauri::{Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
// State to track the sidecar process
struct SystemMonitor {
    sys: Arc<std::sync::Mutex<System>>,
}
struct SidecarConnection {
    child: Child,
    reader: BufReader<tokio::process::ChildStdout>,
}

#[derive(Clone)]
struct SidecarState {
    connection: Arc<Mutex<Option<SidecarConnection>>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FanStatus {
    pub cpu_temp: u8,
    pub gpu_temp: u8,
    pub fan1_rpm: u32,
    pub fan2_rpm: u32,
    pub cooler_boost: bool,
    pub fan_mode: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum SidecarResponse {
    #[serde(rename = "status")]
    Status {
        cpu_temp: u8,
        gpu_temp: u8,
        fan1_rpm: u32,
        fan2_rpm: u32,
        cooler_boost: bool,
        fan_mode: String,
    },
    #[serde(rename = "ok")]
    Ok { message: String },
    #[serde(rename = "error")]
    Error { message: String },
}

fn get_sidecar_path() -> String {
    // In development, use the compiled binary directly
    // In production, Tauri bundles it with target triple suffix
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();

    // Try to find the sidecar binary - check multiple locations
    let possible_paths = [
        // Production: bundled next to executable
        exe_dir.join("msi-sidecar-x86_64-unknown-linux-gnu"),
        exe_dir.join("msi-sidecar"),
        // Development: in target/debug or target/release - allow standard cargo structures
        exe_dir.join("../../binaries/msi-sidecar/target/release/msi-sidecar"),
        exe_dir.join("../binaries/msi-sidecar/target/release/msi-sidecar"),
        exe_dir.join("../../binaries/msi-sidecar/target/debug/msi-sidecar"),
        exe_dir.join("../binaries/msi-sidecar/target/debug/msi-sidecar"),
    ];

    for path in &possible_paths {
        if path.exists() {
            return path
                .canonicalize()
                .unwrap_or_else(|_| path.clone())
                .to_string_lossy()
                .to_string();
        }
    }

    // Fallback - let pkexec find it
    "msi-sidecar".to_string()
}

async fn read_response(
    reader: &mut BufReader<tokio::process::ChildStdout>,
) -> Result<SidecarResponse, String> {
    let mut line = String::new();

    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Read error: {}", e))?;

    if line.is_empty() {
        return Err("Empty response from sidecar - EOF".to_string());
    }

    serde_json::from_str(&line).map_err(|e| format!("Parse error: {} (line: {})", e, line.trim()))
}

async fn send_command(child: &mut Child, cmd: &str) -> Result<(), String> {
    let stdin = child.stdin.as_mut().ok_or("No stdin")?;
    stdin
        .write_all(format!("{}\n", cmd).as_bytes())
        .await
        .map_err(|e| format!("Write error: {}", e))?;
    stdin
        .flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;
    Ok(())
}

#[tauri::command]
async fn start_sidecar(state: State<'_, SidecarState>) -> Result<FanStatus, String> {
    // Acquire lock asynchronously
    let mut guard = state.connection.lock().await;

    // Clean up existing connection if any
    if let Some(mut conn) = guard.take() {
        // We don't care about the result, just try to kill and wait
        let _ = conn.child.kill().await;
        let _ = conn.child.wait().await;
    }

    let sidecar_path = get_sidecar_path();

    // Spawn with pkexec for privilege escalation
    // Note: tokio::process::Command is used here
    let mut child = Command::new("pkexec")
        .arg(&sidecar_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Important: kill on drop allows cleanup if the handle is dropped
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to start sidecar: {}", e))?;

    let stdout = child.stdout.take().ok_or("No stdout captured")?;
    let mut reader = BufReader::new(stdout);

    // Initial handshake with timeout
    // We only need to timeout the read operation, not the whole setup
    // And we can pass &mut reader to read_response directly
    let response_result =
        tokio::time::timeout(Duration::from_secs(5), read_response(&mut reader)).await;

    match response_result {
        Ok(Ok(response)) => {
            // Success - store connection
            *guard = Some(SidecarConnection { child, reader });

            match response {
                SidecarResponse::Status {
                    cpu_temp,
                    gpu_temp,
                    fan1_rpm,
                    fan2_rpm,
                    cooler_boost,
                    fan_mode,
                } => Ok(FanStatus {
                    cpu_temp,
                    gpu_temp,
                    fan1_rpm,
                    fan2_rpm,
                    cooler_boost,
                    fan_mode,
                }),
                SidecarResponse::Error { message } => Err(message),
                _ => Err("Unexpected initial response".to_string()),
            }
        }
        Ok(Err(e)) => {
            // Read error
            let _ = child.kill().await;
            Err(e)
        }
        Err(_) => {
            // Timeout
            let _ = child.kill().await;
            Err("Sidecar startup timeout".to_string())
        }
    }
}

#[tauri::command]
async fn stop_sidecar(state: State<'_, SidecarState>) -> Result<String, String> {
    let mut guard = state.connection.lock().await;

    if let Some(mut conn) = guard.take() {
        // Try graceful exit first
        let _ = send_command(&mut conn.child, r#"{"cmd":"exit"}"#).await;

        // Force kill to be sure
        let _ = conn.child.kill().await;
        let _ = conn.child.wait().await;
    }

    Ok("Sidecar stopped".to_string())
}

#[tauri::command]
async fn get_status(state: State<'_, SidecarState>) -> Result<FanStatus, String> {
    // Acquire lock with timeout to prevent hanging if the lock is held indefinitely
    let guard_result = tokio::time::timeout(Duration::from_secs(1), state.connection.lock()).await;

    // Check if we got the lock
    let mut guard = match guard_result {
        Ok(g) => g,
        Err(_) => return Err("Failed to acquire lock (busy)".to_string()),
    };

    // Check if connected
    let conn = guard
        .as_mut()
        .ok_or("Sidecar not running. Click Connect first.")?;

    let request_future = async {
        send_command(&mut conn.child, r#"{"cmd":"get_status"}"#).await?;
        read_response(&mut conn.reader).await
    };

    // Overall operation timeout
    match tokio::time::timeout(Duration::from_secs(3), request_future).await {
        Ok(Ok(response)) => match response {
            SidecarResponse::Status {
                cpu_temp,
                gpu_temp,
                fan1_rpm,
                fan2_rpm,
                cooler_boost,
                fan_mode,
            } => Ok(FanStatus {
                cpu_temp,
                gpu_temp,
                fan1_rpm,
                fan2_rpm,
                cooler_boost,
                fan_mode,
            }),
            SidecarResponse::Error { message } => Err(message),
            _ => Err("Unexpected response".to_string()),
        },
        Ok(Err(e)) => {
            // IO Error - connection likely dead
            // We should kill it so the next retry forces a clean reconnect
            let _ = conn.child.kill().await;
            *guard = None;
            Err(format!("Communication error: {}", e))
        }
        Err(_) => {
            // Timeout - connection hanging
            let _ = conn.child.kill().await;
            *guard = None;
            Err("Sidecar request timeout".to_string())
        }
    }
}

#[tauri::command]
async fn set_cooler_boost(state: State<'_, SidecarState>, enabled: bool) -> Result<String, String> {
    let mut guard = state.connection.lock().await;
    let conn = guard.as_mut().ok_or("Sidecar not running")?;

    let cmd = format!(
        r#"{{"cmd":"set_cooler_boost","data":{{"enabled":{}}}}}"#,
        enabled
    );

    let request_future = async {
        send_command(&mut conn.child, &cmd).await?;
        read_response(&mut conn.reader).await
    };

    match tokio::time::timeout(Duration::from_secs(3), request_future).await {
        Ok(Ok(response)) => match response {
            SidecarResponse::Ok { message } => Ok(message),
            SidecarResponse::Error { message } => Err(message),
            _ => Err("Unexpected response".to_string()),
        },
        Ok(Err(e)) => {
            let _ = conn.child.kill().await;
            *guard = None;
            Err(format!("Communication error: {}", e))
        }
        Err(_) => {
            let _ = conn.child.kill().await;
            *guard = None;
            Err("Command timeout".to_string())
        }
    }
}

#[tauri::command]
async fn set_fan_speed(state: State<'_, SidecarState>, percent: u8) -> Result<String, String> {
    let mut guard = state.connection.lock().await;
    let conn = guard.as_mut().ok_or("Sidecar not running")?;

    let cmd = format!(
        r#"{{"cmd":"set_fan_speed","data":{{"percent":{}}}}}"#,
        percent
    );

    let request_future = async {
        send_command(&mut conn.child, &cmd).await?;
        read_response(&mut conn.reader).await
    };

    match tokio::time::timeout(Duration::from_secs(3), request_future).await {
        Ok(Ok(response)) => match response {
            SidecarResponse::Ok { message } => Ok(message),
            SidecarResponse::Error { message } => Err(message),
            _ => Err("Unexpected response".to_string()),
        },
        Ok(Err(e)) => {
            let _ = conn.child.kill().await;
            *guard = None;
            Err(format!("Communication error: {}", e))
        }
        Err(_) => {
            let _ = conn.child.kill().await;
            *guard = None;
            Err("Command timeout".to_string())
        }
    }
}

#[tauri::command]
async fn set_fan_mode(state: State<'_, SidecarState>, mode: String) -> Result<String, String> {
    let mut guard = state.connection.lock().await;
    let conn = guard.as_mut().ok_or("Sidecar not running")?;

    let cmd = format!(r#"{{"cmd":"set_fan_mode","data":{{"mode":"{}"}}}}"#, mode);

    let request_future = async {
        send_command(&mut conn.child, &cmd).await?;
        read_response(&mut conn.reader).await
    };

    match tokio::time::timeout(Duration::from_secs(3), request_future).await {
        Ok(Ok(response)) => match response {
            SidecarResponse::Ok { message } => Ok(message),
            SidecarResponse::Error { message } => Err(message),
            _ => Err("Unexpected response".to_string()),
        },
        Ok(Err(e)) => {
            let _ = conn.child.kill().await;
            *guard = None;
            Err(format!("Communication error: {}", e))
        }
        Err(_) => {
            let _ = conn.child.kill().await;
            *guard = None;
            Err("Command timeout".to_string())
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareInfo {
    pub cpu_model: String,
    pub gpu_model: String,
    pub memory_total: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStats {
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub cpu_global_frequency: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CpuCoreDetail {
    pub name: String,
    pub frequency: u64,
    pub usage: f32,
}

#[tauri::command]
async fn get_hardware_info(state: State<'_, SystemMonitor>) -> Result<HardwareInfo, String> {
    let sys_arc = state.sys.clone();

    // Spawn blocking task so we don't freeze the async runtime
    let info = tokio::task::spawn_blocking(move || {
        let mut sys = sys_arc.lock().map_err(|e| e.to_string())?;
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let cpu_model = sys
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());

        let memory_total = sys.total_memory();

        Ok::<HardwareInfo, String>(HardwareInfo {
            cpu_model,
            gpu_model: "GeForce GTX 1660 Ti Mobile".to_string(),
            memory_total,
        })
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??; // Unpack JoinError then Result

    Ok(info)
}

#[tauri::command]
async fn get_system_stats(state: State<'_, SystemMonitor>) -> Result<SystemStats, String> {
    let sys_arc = state.sys.clone();

    let stats = tokio::task::spawn_blocking(move || {
        let mut sys = sys_arc.lock().map_err(|e| e.to_string())?;
        sys.refresh_memory();
        sys.refresh_cpu_specifics(CpuRefreshKind::nothing().with_frequency().with_cpu_usage());

        let memory_used = sys.used_memory();
        let memory_total = sys.total_memory();
        let swap_used = sys.used_swap();
        let swap_total = sys.total_swap();

        // Calculate global frequency (MAX of all cores to represent "Turbo" speed)
        let cpus = sys.cpus();
        let global_freq = if !cpus.is_empty() {
            cpus.iter().map(|c| c.frequency()).max().unwrap_or(0)
        } else {
            0
        };

        Ok::<SystemStats, String>(SystemStats {
            memory_used,
            memory_total,
            swap_used,
            swap_total,
            cpu_global_frequency: global_freq,
        })
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??;

    Ok(stats)
}

#[tauri::command]
async fn get_cpu_details(state: State<'_, SystemMonitor>) -> Result<Vec<CpuCoreDetail>, String> {
    let sys_arc = state.sys.clone();

    let details = tokio::task::spawn_blocking(move || {
        let mut sys = sys_arc.lock().map_err(|e| e.to_string())?;
        sys.refresh_cpu_specifics(CpuRefreshKind::nothing().with_frequency().with_cpu_usage());

        let cores = sys
            .cpus()
            .iter()
            .map(|cpu| CpuCoreDetail {
                name: cpu.name().to_string(),
                frequency: cpu.frequency(),
                usage: cpu.cpu_usage(),
            })
            .collect();

        Ok::<Vec<CpuCoreDetail>, String>(cores)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))??;

    Ok(details)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }))
        .manage(SidecarState {
            connection: Arc::new(Mutex::new(None)),
        })
        .manage(SystemMonitor {
            sys: Arc::new(std::sync::Mutex::new(System::new_all())),
        })
        .invoke_handler(tauri::generate_handler![
            start_sidecar,
            stop_sidecar,
            get_status,
            set_cooler_boost,
            set_fan_speed,
            set_fan_mode,
            get_hardware_info,
            get_system_stats,
            get_cpu_details
        ])
        .setup(|app| {
            use tauri::image::Image;
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            use tauri::Manager;

            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let window_icon = Image::from_bytes(include_bytes!("../icons/128x128.png"))
                .expect("Failed to load window icon");
            let tray_icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))
                .expect("Failed to load tray icon");

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(window_icon);
            }

            let _tray = TrayIconBuilder::with_id("msi-main-tray")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .icon(tray_icon)
                .tooltip("MSI Fan Control")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    use tauri::tray::TrayIconEvent;
                    if let TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            use tauri::WindowEvent;
            if let WindowEvent::CloseRequested { api, .. } = event {
                // hide the window instead of closing it
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
