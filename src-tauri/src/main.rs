// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Fix for Linux WebKitGTK rendering issues (low FPS/lag)
    // Disables the DMABUF renderer which is often buggy on Nvidia/Linux
    // See: https://github.com/tauri-apps/tauri/issues/10543
    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    tauri_app_lib::run()
}
