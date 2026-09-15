//! Tauri shell for Loom.
//!
//! Kept deliberately thin: window/app plumbing lives here, everything else
//! (config, storage, providers, streaming, tools, updater) lives in
//! `loom-core` so a future CLI or headless mode can reuse it.

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    loom_home: String,
}

#[tauri::command]
fn app_info() -> Result<AppInfo, String> {
    let loom_home = loom_core::paths::loom_home().map_err(|e| e.to_string())?;
    Ok(AppInfo {
        name: "Loom",
        version: env!("CARGO_PKG_VERSION"),
        loom_home: loom_home.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
fn get_config() -> Result<loom_core::config::AppConfig, String> {
    loom_core::config::load().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_config(config: loom_core::config::AppConfig) -> Result<(), String> {
    loom_core::config::save(&config).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|_app| {
            loom_core::paths::ensure_home()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info, get_config, save_config])
        .run(tauri::generate_context!())
        .expect("error while running loom");
}
