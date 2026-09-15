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

/// Ctrl+Shift+Space summons the quick-ask overlay.
fn ask_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
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

            let app_config = loom_core::config::load()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let shared: SharedConfig = Arc::new(std::sync::Mutex::new(app_config));

            let db = Database::open_default()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;

            let handle = app.handle().clone();
            let emit: loom_core::engine::EmitFn = Arc::new(move |event: EngineEvent| {
                let _ = handle.emit("loom://event", &event);

                if let EngineEvent::Done { session_id, .. } = &event {
                    let focused = handle
                        .get_webview_window("main")
                        .and_then(|window| window.is_focused().ok())
                        .unwrap_or(false);
                    if !focused {
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

            app.manage(AppState::new(engine, shared));
            build_tray(app)?;

            app.global_shortcut().register(ask_shortcut())?;

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
