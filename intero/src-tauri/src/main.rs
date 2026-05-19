#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::process::Command;
use std::sync::Mutex;
use sysinfo::{ProcessExt, System, SystemExt};
use tauri::State;

struct AppState {
    sys: Mutex<System>,
}

#[tauri::command]
fn check_daemon_status(state: State<'_, AppState>) -> Result<bool, String> {
    let mut sys = state.sys.lock().unwrap();
    sys.refresh_all();

    // Check if veyn-core or veyn-daemon is active.
    for (_, process) in sys.processes() {
        let name = process.name().to_lowercase();
        if name.contains("veyn-core") || name.contains("veyn-daemon") {
            return Ok(true);
        }
    }
    Ok(false)
}

#[tauri::command]
fn launch_veyn_daemon() -> Result<(), String> {
    // Attempt to start the veyn-core binary in the workspace target folder or system paths.
    let paths = vec![
        "../target/debug/veyn-core.exe",
        "../target/debug/veyn-core",
        "./veyn-core.exe",
        "./veyn-core",
    ];

    for path in paths {
        if std::path::Path::new(path).exists() {
            Command::new(path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn daemon: {}", e))?;
            return Ok(());
        }
    }

    Err("VEYN daemon executable not found in standard build directories.".to_string())
}

#[tauri::command]
fn read_secure_token() -> Result<String, String> {
    // Read the authorization token from the default VEYN token storage.
    let path = dirs::home_dir()
        .ok_or_else(|| "Could not locate home directory".to_string())?
        .join(".veyn")
        .join("token");

    if path.exists() {
        std::fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .map_err(|e| format!("Failed to read token file: {}", e))
    } else {
        Err("Secure token file not found. Ensure the daemon has been started once.".to_string())
    }
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            sys: Mutex::new(System::new_all()),
        })
        .invoke_handler(tauri::generate_handler![
            check_daemon_status,
            launch_veyn_daemon,
            read_secure_token
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
