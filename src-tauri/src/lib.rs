//! Tauri shell for Loom.
//!
//! Window/app plumbing only: the engine, storage, providers, and streaming all
//! live in `loom-core`, so a future CLI can reuse them unchanged.

mod commands;

use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use loom_core::db::Database;
use loom_core::engine::{Engine, EngineEvent, SharedConfig};

use commands::AppState;

/// Default quick-ask hotkey. Replaced at runtime by the configured value.
fn ask_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
}

/// Parses a hotkey string such as `Ctrl+Shift+Space`.
fn parse_shortcut(keys: &str) -> Result<Shortcut, String> {
    keys.trim()
        .parse::<Shortcut>()
        .map_err(|error| format!("unusable hotkey \"{keys}\": {error}"))
}

/// Applies the configured hotkey: unregisters whatever was registered and
/// registers the new one, or nothing at all when disabled.
fn apply_hotkey(app: &AppHandle, enabled: bool, keys: &str) -> Result<(), String> {
    let manager = app.global_shortcut();
    manager.unregister_all().map_err(|e| e.to_string())?;
    if !enabled {
        return Ok(());
    }
    let shortcut = parse_shortcut(keys)?;
    manager.register(shortcut).map_err(|e| e.to_string())
}

/// Used by the settings command; kept here because it owns the plugin handle.
pub(crate) fn set_hotkey_now(app: &AppHandle, enabled: bool, keys: &str) -> Result<(), String> {
    apply_hotkey(app, enabled, keys)
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Shows, hides, or lazily creates the quick-ask overlay window.
fn toggle_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("ask") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }

    let built = tauri::WebviewWindowBuilder::new(
        app,
        "ask",
        tauri::WebviewUrl::App("index.html?ask=1".into()),
    )
    .title("Ask Loom")
    .inner_size(640.0, 92.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .center()
    .build();

    match built {
        Ok(window) => {
            let _ = window.show();
            let _ = window.set_focus();
        }
        Err(error) => eprintln!("[loom] failed to create overlay window: {error}"),
    }
}

/// Reads `(hotkey_enabled, hotkey)` without needing managed state yet.
fn shared_interface(_app: &AppHandle) -> (bool, String) {
    match loom_core::config::load() {
        Ok(config) => (config.interface.hotkey_enabled, config.interface.hotkey),
        Err(_) => (true, "Ctrl+Shift+Space".to_string()),
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Loom", true, None::<&str>)?;
    let ask = MenuItem::with_id(app, "ask", "Quick ask", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &ask,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Loom")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "ask" => toggle_overlay(app),
            "settings" => {
                let _ = app.emit("loom://open-settings", ());
                show_main_window(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed && shortcut == &ask_shortcut() {
                        toggle_overlay(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            loom_core::paths::ensure_home()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;

            let mut app_config = loom_core::config::load()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;

            // Bring configs written by older builds up to date (for example,
            // providers that now need a session header).
            if loom_core::config::apply_preset_defaults(&mut app_config) {
                if let Err(error) = loom_core::config::save(&app_config) {
                    eprintln!("[loom] could not upgrade config: {error}");
                }
            }

            let shared: SharedConfig = Arc::new(std::sync::Mutex::new(app_config));

            let db = Database::open_default()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;

            let handle = app.handle().clone();
            let notify_config = Arc::clone(&shared);
            let emit: loom_core::engine::EmitFn = Arc::new(move |event: EngineEvent| {
                // Reported to stderr so `loom.exe > log` shows exactly which
                // events the UI is sent while diagnosing.
                eprintln!("[loom] -> {}", event.label());
                if let Err(error) = handle.emit("loom://event", &event) {
                    eprintln!("[loom] emit failed: {error}");
                }

                if let EngineEvent::Done { session_id, .. } = &event {
                    let wanted = notify_config
                        .lock()
                        .map(|config| config.interface.notify_on_completion)
                        .unwrap_or(true);
                    let focused = handle
                        .get_webview_window("main")
                        .and_then(|window| window.is_focused().ok())
                        .unwrap_or(false);
                    if wanted && !focused {
                        let _ = handle
                            .notification()
                            .builder()
                            .title("Loom")
                            .body("A reply finished while you were away.")
                            .show();
                    }
                    let _ = session_id;
                }
            });

            let engine = Engine::new(db, Arc::clone(&shared), emit);

            // A development build loads the Vite dev server. If that server is
            // not running the window would stay blank, which looks like the app
            // is broken; fall back to the frontend bundled in the binary.
            if cfg!(dev) {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    let reachable = tokio::net::TcpStream::connect("127.0.0.1:1420")
                        .await
                        .is_ok();
                    if reachable {
                        return;
                    }
                    if let Some(window) = handle.get_webview_window("main") {
                        eprintln!(
                            "[loom] dev server is not running; loading the bundled frontend instead"
                        );
                        match tauri::Url::parse("tauri://localhost/index.html") {
                            Ok(url) => {
                                if let Err(error) = window.navigate(url) {
                                    eprintln!("[loom] fallback navigation failed: {error}");
                                }
                            }
                            Err(error) => eprintln!("[loom] bad fallback url: {error}"),
                        }
                    }
                });
            }

            app.manage(AppState::new(engine, shared));
            build_tray(app)?;

            let interface = shared_interface(&app.handle().clone());
            if let Err(error) = apply_hotkey(&app.handle().clone(), interface.0, &interface.1) {
                eprintln!("[loom] hotkey not registered: {error}");
            }

            // Quietly check for a new release a few seconds after launch and
            // let the UI offer it.
            let update_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                let state = update_handle.state::<AppState>();
                let client = state.engine.http_client();
                match loom_core::updater::check(
                    &client,
                    env!("CARGO_PKG_VERSION"),
                    commands::UPDATE_MANIFEST_URL,
                )
                .await
                {
                    Ok(check) if check.available => {
                        let _ = update_handle.emit("loom://update-available", &check.manifest);
                    }
                    Ok(_) => {}
                    Err(error) => eprintln!("[loom] update check skipped: {error}"),
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_config,
            commands::save_config,
            commands::list_provider_presets,
            commands::upsert_provider,
            commands::delete_provider,
            commands::set_provider_key,
            commands::clear_provider_key,
            commands::provider_key_status,
            commands::refresh_provider_models,
            commands::set_provider_enabled,
            commands::list_models,
            commands::set_default_model,
            commands::set_chat_settings,
            commands::set_interface_settings,
            commands::set_hotkey,
            commands::upsert_persona,
            commands::delete_persona,
            commands::create_session,
            commands::list_sessions,
            commands::delete_session,
            commands::rename_session,
            commands::session_messages,
            commands::delete_message,
            commands::export_session,
            commands::set_model_favorite,
            commands::set_session_model,
            commands::set_session_variant,
            commands::set_session_workdir,
            commands::set_session_permission_mode,
            commands::respond_tool_permission,
            commands::list_tools,
            commands::workspace_info,
            commands::upsert_mcp_server,
            commands::delete_mcp_server,
            commands::mcp_tools,
            commands::list_skills,
            commands::set_image_model,
            commands::index_workspace,
            commands::workspace_index_status,
            commands::clear_workspace_index,
            commands::set_embedding_model,
            commands::add_model,
            commands::set_model_spec,
            commands::check_for_updates,
            commands::download_update,
            commands::apply_update,
            commands::set_session_persona,
            commands::send_message,
            commands::attach_files,
            commands::attach_bytes,
            commands::set_background_file,
            commands::cancel_stream,
            commands::busy_sessions,
            commands::hide_overlay,
            commands::show_main,
            commands::open_settings,
            commands::quit_app,
            commands::overlay_target,
        ])
        .on_window_event(|window, event| {
            // Close-to-tray: streams keep running in the background.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running loom");
}
