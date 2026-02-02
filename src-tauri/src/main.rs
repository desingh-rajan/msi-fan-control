// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::fs;

fn main() {
    // Check if we are on Linux
    #[cfg(target_os = "linux")]
    {
        // Optimization: Attempt to uncap FPS by disabling VSync on supported drivers
        // 1. Standard GTK debug flag
        env::set_var("GTK_DEBUG", "no-vsync");

        // 2. MESA drivers (Intel/AMD)
        env::set_var("vblank_mode", "0");

        // 3. NVIDIA drivers
        env::set_var("__GL_SYNC_TO_VBLANK", "0");
        env::set_var("__GL_YIELD", "USLEEP");

        // Read the OS release info
        if let Ok(os_release) = fs::read_to_string("/etc/os-release") {
            // If we detect Ubuntu 22.04, apply the performance fix
            if os_release.contains("VERSION_ID=\"22.04\"") {
                // This disables the buggy hardware compositing in older WebKit versions
                // which is the primary cause of low FPS/sluggishness on NVIDIA + 22.04.
                env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            }
        }
    }

    tauri_app_lib::run()
}
