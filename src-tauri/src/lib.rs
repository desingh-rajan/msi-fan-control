use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use sysinfo::System;
use tauri::{Manager, State};

// State to track the sidecar process
struct SidecarConnection {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
}

#[derive(Clone)]
struct SidecarState {
    connection: std::sync::Arc<Mutex<Option<SidecarConnection>>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FanStatus {
    pub cpu_temp: u8,
    pub gpu_temp: u8,
    pub fan1_rpm: u32,
    pub fan2_rpm: u32,
    pub cooler_boost: bool,
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

fn read_response(
    reader: &mut BufReader<std::process::ChildStdout>,
) -> Result<SidecarResponse, String> {
    let mut line = String::new();

    reader
        .read_line(&mut line)
        .map_err(|e| format!("Read error: {}", e))?;

    if line.is_empty() {
        return Err("Empty response from sidecar".to_string());
    }

    serde_json::from_str(&line).map_err(|e| format!("Parse error: {} (line: {})", e, line.trim()))
}

fn send_command(child: &mut Child, cmd: &str) -> Result<(), String> {
    let stdin = child.stdin.as_mut().ok_or("No stdin")?;
    writeln!(stdin, "{}", cmd).map_err(|e| format!("Write error: {}", e))?;
    stdin.flush().map_err(|e| format!("Flush error: {}", e))?;
    Ok(())
}

#[tauri::command]
async fn start_sidecar(state: State<'_, SidecarState>) -> Result<FanStatus, String> {
    let timeout_duration = Duration::from_secs(5);
    
    let state_clone = state.inner().clone();
    let result = tokio::time::timeout(
        timeout_duration,
        tokio::task::spawn_blocking(move || {
            let mut guard = state_clone.connection.lock().map_err(|e| e.to_string())?;

            // Kill existing if any
            if let Some(mut conn) = guard.take() {
                let _ = conn.child.kill();
                let _ = conn.child.wait();
            }

            let sidecar_path = get_sidecar_path();
            // eprintln!("Starting sidecar from: {}", sidecar_path);

            // Spawn with pkexec for privilege escalation
            let mut child = Command::new("pkexec")
                .arg(&sidecar_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Failed to start sidecar: {}", e))?;

            let stdout = child.stdout.take().ok_or("No stdout captured")?;
            let mut reader = BufReader::new(stdout);

            // Read initial status
            let response = read_response(&mut reader)?;

            *guard = Some(SidecarConnection { child, reader });

            match response {
                SidecarResponse::Status {
                    cpu_temp,
                    gpu_temp,
                    fan1_rpm,
                    fan2_rpm,
                    cooler_boost,
                } => Ok(FanStatus {
                    cpu_temp,
                    gpu_temp,
                    fan1_rpm,
                    fan2_rpm,
                    cooler_boost,
                }),
                SidecarResponse::Error { message } => Err(message),
                _ => Err("Unexpected response".to_string()),
            }
        })
    ).await;

    match result {
        Ok(Ok(status_result)) => status_result,
        Ok(Err(e)) => Err(format!("Task failed: {}", e)),
        Err(_) => {
            // Timeout occurred - kill any spawned process
            let mut guard = state.connection.lock().map_err(|e| e.to_string())?;
            if let Some(mut conn) = guard.take() {
                let _ = conn.child.kill();
            }
            Err("Sidecar startup timeout - connection failed".to_string())
        }
    }
}

#[tauri::command]
async fn stop_sidecar(state: State<'_, SidecarState>) -> Result<String, String> {
    let mut guard = state.connection.lock().map_err(|e| e.to_string())?;

    if let Some(mut conn) = guard.take() {
        // Send exit command
        let _ = send_command(&mut conn.child, r#"{"cmd":"exit"}"#);
        let _ = conn.child.kill();
        let _ = conn.child.wait();
    }

    Ok("Sidecar stopped".to_string())
}

#[tauri::command]
async fn get_status(state: State<'_, SidecarState>) -> Result<FanStatus, String> {
    // Spawn blocking task with timeout to prevent hangs
    let timeout_duration = Duration::from_secs(3);
    
    let state_clone = state.inner().clone();
    let result = tokio::time::timeout(
        timeout_duration,
        tokio::task::spawn_blocking(move || {
            let mut guard = state_clone.connection.lock().map_err(|e| e.to_string())?;

            let conn = guard
                .as_mut()
                .ok_or("Sidecar not running. Click Connect first.")?;

            send_command(&mut conn.child, r#"{"cmd":"get_status"}"#)?;
            let response = read_response(&mut conn.reader)?;

            match response {
                SidecarResponse::Status {
                    cpu_temp,
                    gpu_temp,
                    fan1_rpm,
                    fan2_rpm,
                    cooler_boost,
                } => Ok(FanStatus {
                    cpu_temp,
                    gpu_temp,
                    fan1_rpm,
                    fan2_rpm,
                    cooler_boost,
                }),
                SidecarResponse::Error { message } => Err(message),
                _ => Err("Unexpected response".to_string()),
            }
        })
    ).await;

    match result {
        Ok(Ok(status_result)) => status_result,
        Ok(Err(e)) => Err(format!("Task failed: {}", e)),
        Err(_) => {
            // Timeout occurred - kill the connection and force reconnect
            let mut guard = state.connection.lock().map_err(|e| e.to_string())?;
            if let Some(mut conn) = guard.take() {
                let _ = conn.child.kill();
            }
            Err("Sidecar connection timeout - please reconnect".to_string())
        }
    }
}

#[tauri::command]
async fn set_cooler_boost(state: State<'_, SidecarState>, enabled: bool) -> Result<String, String> {
    let timeout_duration = Duration::from_secs(3);
    
    let state_clone = state.inner().clone();
    let result = tokio::time::timeout(
        timeout_duration,
        tokio::task::spawn_blocking(move || {
            let mut guard = state_clone.connection.lock().map_err(|e| e.to_string())?;

            let conn = guard.as_mut().ok_or("Sidecar not running")?;

            let cmd = format!(
                r#"{{"cmd":"set_cooler_boost","data":{{"enabled":{}}}}}"#,
                enabled
            );
            send_command(&mut conn.child, &cmd)?;
            let response = read_response(&mut conn.reader)?;

            match response {
                SidecarResponse::Ok { message } => Ok(message),
                SidecarResponse::Error { message } => Err(message),
                _ => Err("Unexpected response".to_string()),
            }
        })
    ).await;

    match result {
        Ok(Ok(msg_result)) => msg_result,
        Ok(Err(e)) => Err(format!("Task failed: {}", e)),
        Err(_) => {
            // Timeout occurred - kill the connection
            let mut guard = state.connection.lock().map_err(|e| e.to_string())?;
            if let Some(mut conn) = guard.take() {
                let _ = conn.child.kill();
            }
            Err("Cooler boost command timeout - please reconnect".to_string())
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareInfo {
    pub cpu_model: String,
    pub gpu_model: String,
}

#[tauri::command]
fn get_hardware_info() -> HardwareInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_model = sys
        .cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or("Unknown CPU".to_string());

    // Enhanced GPU detection: Prioritize Discrete (NVIDIA/AMD) over Integrated (Intel)
    let gpu_output = std::process::Command::new("sh")
        .arg("-c")
        .arg("lspci | grep -i 'vga\\|3d controller'")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let mut gpu_model = "Discrete Graphics".to_string();

    for line in gpu_output.lines() {
        let lower = line.to_lowercase();
        // Extract basic model name after the hex ID and before brackets if possible
        // Example: 01:00.0 VGA compatible controller: NVIDIA Corporation TU116M [GeForce GTX 1660 Ti Mobile]

        if let Some(start_idx) = line.find(": ") {
            let distinct_part = &line[start_idx + 2..];
            // Try to get content inside [] first as it's usually the clean model name
            if let (Some(open), Some(close)) = (distinct_part.rfind('['), distinct_part.rfind(']'))
            {
                let model = distinct_part[open + 1..close].trim().to_string();
                if lower.contains("nvidia") || lower.contains("amd") || lower.contains("radeon") {
                    gpu_model = model;
                    break; // Found our target
                } else {
                    // Keep looking but save this (likely Intel) just in case
                    gpu_model = model;
                }
            }
        }
    }

    HardwareInfo {
        cpu_model: if cpu_model.is_empty() {
            "Generic Processer".into()
        } else {
            cpu_model
        },
        gpu_model: if gpu_model.is_empty() {
            "Discrete Graphics".into()
        } else {
            gpu_model
        },
    }
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
            connection: std::sync::Arc::new(Mutex::new(None)),
        })
        .invoke_handler(tauri::generate_handler![
            start_sidecar,
            stop_sidecar,
            get_status,
            set_cooler_boost,
            get_hardware_info
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
