//! Tauri shell for Loom.
//!
//! Window/app plumbing only: the engine, storage, providers, and streaming all
//! live in `loom-core`, so a future CLI can reuse them unchanged.

mod commands;
mod voice;

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

/// Applies the configured hotkey: re-registers only the quick-ask binding, so
/// the computer panic key survives a hotkey change.
fn apply_hotkey(app: &AppHandle, enabled: bool, keys: &str) -> Result<(), String> {
    let manager = app.global_shortcut();
    let _ = manager.unregister(ask_shortcut());
    let _ = manager.unregister(stop_shortcut());
    if enabled {
        let shortcut = parse_shortcut(keys)?;
        manager.register(shortcut).map_err(|e| e.to_string())?;
    }
    manager
        .register(stop_shortcut())
        .map_err(|e| e.to_string())
}

/// Ctrl+Alt+Esc: stops the computer turn immediately, from anywhere.
fn stop_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Escape)
}

/// The panic key, and the pill's Stop: ends the chat that is driving the
/// machine, and nothing else.
///
/// It used to call `cancel_all`, which ended every other chat's turn and every
/// detached background run too — a key labelled "stop computer use" that killed
/// unrelated work is not a panic button, it is a hazard. Returns the chat that
/// was stopped, if any.
fn stop_computer_turn(app: &AppHandle) -> Option<String> {
    let stopped = app
        .try_state::<AppState>()
        .and_then(|state| state.engine.stop_computer());
    hide_computer_pill(app);
    // Every window follows: the pill is one, but the chip in the main window
    // and the overlay may be showing a Resume button for the same turn.
    let _ = app.emit("loom://computer-stopped", stopped.clone());
    stopped
}

/// Shows the "Loom is controlling your computer" pill, creating it lazily.
pub(crate) fn show_computer_pill(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("computer") {
        position_computer_pill(&window);
        let _ = window.show();
        return;
    }

    let built = tauri::WebviewWindowBuilder::new(
        app,
        "computer",
        tauri::WebviewUrl::App("index.html?computer=1".into()),
    )
    .title("Loom is controlling your computer")
    .inner_size(430.0, 56.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .focused(false)
    .build();

    match built {
        Ok(window) => {
            // Clicking Stop (or Resume) on the pill is Loom's own UI, not the
            // user taking the machine back; keep it out of the takeover
            // detector, and out of screenshots.
            if let Ok(hwnd) = window.hwnd() {
                loom_core::computer::set_ignored_window(hwnd.0 as isize);
                // Pressing Resume must not pull focus away from the window
                // Loom is driving: the keystroke it sends next goes wherever
                // focus is, and the model had no way to know it moved.
                loom_core::computer::make_window_non_activating(hwnd.0 as isize);
                let _ = loom_core::screen::exclude_from_capture(hwnd.0 as isize);
            }
            position_computer_pill(&window);
            let _ = window.show();
        }
        Err(error) => eprintln!("[loom] failed to create the computer pill: {error}"),
    }
}

/// Registers every window Loom owns with the takeover detector, so the user
/// reading the transcript, typing in the composer or answering the quick-ask
/// overlay is not mistaken for taking the machine over.
///
/// All of them, not just the pill: the old build registered one hwnd, so a
/// click anywhere in the main window paused the turn that the user was only
/// trying to watch.
fn register_own_windows(app: &AppHandle) {
    for label in ["main", "ask", "computer"] {
        if let Some(window) = app.get_webview_window(label) {
            if let Ok(hwnd) = window.hwnd() {
                loom_core::computer::set_ignored_window(hwnd.0 as isize);
            }
        }
    }
}

/// Hides the pill. The window is kept alive so showing it again is instant.
pub(crate) fn hide_computer_pill(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("computer") {
        let _ = window.hide();
    }
}

/// Top-centre of the primary monitor: a fixed spot for the Stop button.
fn position_computer_pill(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let size = window
        .outer_size()
        .unwrap_or(tauri::PhysicalSize::new(430, 56));
    let x = monitor.position().x
        + ((monitor.size().width as i32 - size.width as i32) / 2).max(0);
    let y = monitor.position().y + 24;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
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

/// Keeps the overlay out of every screen capture (Windows 10 2004 and later),
/// and reports whether that worked. The quick-ask screenshot relies on this to
/// see the desktop under the window instead of hiding it for every send.
#[cfg(windows)]
fn exclude_overlay(window: &tauri::WebviewWindow) -> bool {
    window
        .hwnd()
        .ok()
        .map(|hwnd| loom_core::screen::exclude_from_capture(hwnd.0 as isize))
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn exclude_overlay(_window: &tauri::WebviewWindow) -> bool {
    false
}

/// Shows, hides, or lazily creates the quick-ask overlay window.
fn toggle_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("ask") {
        let excluded = exclude_overlay(&window);
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            // The first message of this summon sends the screen as it was
            // when the hotkey was pressed.
            if excluded {
                commands::stash_screen(app);
            }
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
            // The overlay is Loom's own window: typing a question into it is
            // not taking the machine over.
            if let Ok(hwnd) = window.hwnd() {
                loom_core::computer::set_ignored_window(hwnd.0 as isize);
            }
            if exclude_overlay(&window) {
                commands::stash_screen(app);
            }
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
    let tasks = MenuItem::with_id(app, "tasks", "Runs…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &ask,
            &tasks,
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
            "tasks" => {
                let _ = app.emit("loom://open-tasks", ());
                show_main_window(app);
            }
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
    let mut context = tauri::generate_context!();

    // A dev build claims its own identifier, so one dev instance and one
    // installed instance can run together: the single-instance guard below is
    // keyed on the identifier, and so is the WebView2 profile.
    if cfg!(dev) {
        context.config_mut().identifier = "com.ellio.loom.dev".to_string();
    }

    tauri::Builder::default()
        // First plugin on purpose: a second instance exits inside its setup,
        // before the rest of the app touches the database, hotkeys, or tray.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            // The same name the installer writes to the Run key, so the
            // Settings toggle and a fresh install agree on one entry.
            tauri_plugin_autostart::Builder::new()
                .app_name("Loom")
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if shortcut == &ask_shortcut() {
                            toggle_overlay(app);
                        } else if shortcut == &stop_shortcut() {
                            stop_computer_turn(app);
                        }
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
            // providers that now need a session header), then tag each model's
            // metadata with where it came from so refreshes can correct it.
            let mut upgraded = loom_core::config::apply_preset_defaults(&mut app_config);
            upgraded |= loom_core::config::migrate_metadata(&mut app_config);
            if upgraded {
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
                // events the UI is sent while diagnosing — but opt-in, and never
                // for the per-token events. `Delta` and `Reasoning` fire hundreds
                // of times a turn, so logging them is both a log that grows by
                // hundreds of megabytes and, when stderr is a pipe nobody drains
                // (a parent process spawning loom.exe), a permanent stall: the
                // pipe's buffer fills and `eprintln!` blocks *inside this
                // callback*, on the engine task, for ever.
                if std::env::var_os("LOOM_EVENT_LOG").is_some() {
                    if !matches!(
                        &event,
                        EngineEvent::Delta { .. } | EngineEvent::Reasoning { .. }
                    ) {
                        eprintln!("[loom] -> {}", event.label());
                    }
                }
                if let Err(error) = handle.emit("loom://event", &event) {
                    eprintln!("[loom] emit failed: {error}");
                }

                // The control pill follows computer tools: up while one runs,
                // and paused/resumed as the user takes over and hands back.
                match &event {
                    EngineEvent::ToolCallStarted { name, .. }
                        if loom_core::computer::is_computer_tool(name) =>
                    {
                        show_computer_pill(&handle);
                    }
                    // A stop ends the turn just as much as a Done does, so the
                    // pill comes down either way — but only for the chat that
                    // was actually driving. Another chat finishing must not take
                    // the pill away while Loom is still moving the mouse.
                    EngineEvent::Done { session_id, .. }
                    | EngineEvent::Notice { session_id, .. } => {
                        let holder = handle
                            .try_state::<AppState>()
                            .and_then(|state| state.engine.computer_holder());
                        let ended_the_driver =
                            holder.as_deref().is_none_or(|owner| owner == session_id);
                        if ended_the_driver {
                            hide_computer_pill(&handle);
                        }
                    }
                    EngineEvent::ComputerPaused { .. } => {
                        if let Some(window) = handle.get_webview_window("computer") {
                            let _ = window.emit("loom://computer-state", "paused");
                        }
                    }
                    EngineEvent::ComputerResumed { .. } => {
                        if let Some(window) = handle.get_webview_window("computer") {
                            let _ = window.emit("loom://computer-state", "active");
                        }
                    }
                    // Detached runs talk to the user through notifications:
                    // failures always, successes only when the run asked for it,
                    // and anything that is waiting on an answer.
                    EngineEvent::TaskChanged { task } => {
                        let body = match task.status.as_str() {
                            "done" if task.notify => {
                                Some(format!("“{}” finished.", task.title))
                            }
                            "failed" => Some(format!(
                                "“{}” failed: {}",
                                task.title,
                                task.detail.clone().unwrap_or_default()
                            )),
                            "running"
                                if task
                                    .detail
                                    .as_deref()
                                    .is_some_and(|detail| detail.contains("approval")) =>
                            {
                                Some(format!("“{}” is waiting for your approval.", task.title))
                            }
                            _ => None,
                        };
                        if let Some(body) = body {
                            let _ = handle
                                .notification()
                                .builder()
                                .title("Loom")
                                .body(body)
                                .show();
                        }
                    }
                    _ => {}
                }

                if let EngineEvent::Done { session_id, .. } = &event {
                    let wanted = notify_config
                        .lock()
                        .map(|config| config.interface.notify_on_completion)
                        .unwrap_or(true);
                    let focused = ["main", "ask"].iter().any(|label| {
                        handle
                            .get_webview_window(label)
                            .and_then(|window| window.is_focused().ok())
                            .unwrap_or(false)
                    });
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

            // The main window is Loom's own: input over it is the user reading
            // or typing in Loom, not taking the machine over.
            register_own_windows(app.handle());

            // A development build loads the Vite dev server. If that server is
            // not running the window would stay blank, which looks like the app
            // is broken; fall back to the frontend bundled in the binary.
            //
            // The probe follows the configured dev URL rather than a fixed
            // address: Vite can bind `::1` only on Windows, so probing
            // `127.0.0.1` would report a server that is right there as down,
            // and "localhost" lets the resolver try every address family.
            if cfg!(dev) {
                let handle = app.handle().clone();
                let probe = app
                    .config()
                    .build
                    .dev_url
                    .as_ref()
                    .and_then(|url| {
                        let host = url.host_str()?;
                        Some(format!("{host}:{}", url.port_or_known_default().unwrap_or(1420)))
                    })
                    .unwrap_or_else(|| "localhost:1420".to_string());
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    let reachable = tokio::net::TcpStream::connect(&probe).await.is_ok();
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

            // Anything left queued or running by a previous session is not
            // running now; then start the scheduler (also runs the launch
            // catch-up pass for jobs missed while the app was closed).
            let scheduler = app.state::<AppState>().engine.clone();
            let interrupted = scheduler.mark_interrupted_tasks();
            if interrupted > 0 {
                eprintln!("[loom] marked {interrupted} interrupted run(s)");
            }
            // Background commands are deliberately NOT killed when Loom quits,
            // so a row left as running is marked orphaned rather than
            // pretending the process ended with the app.
            let orphaned = scheduler.mark_interrupted_commands();
            if orphaned > 0 {
                eprintln!("[loom] marked {orphaned} orphaned command(s)");
            }
            tauri::async_runtime::spawn(async move {
                loop {
                    scheduler.tick_jobs().await;
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            });

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
            commands::duplicate_provider,
            commands::set_models_selected,
            commands::set_provider_auto_select,
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
            commands::autostart_enabled,
            commands::set_autostart,
            commands::storage_usage,
            commands::clear_cache,
            commands::clear_generated,
            commands::upsert_prompt,
            commands::delete_prompt,
            commands::upsert_persona,
            commands::delete_persona,
            commands::set_user_profile,
            commands::upsert_persona_group,
            commands::delete_persona_group,
            commands::persona_memory,
            commands::set_persona_memory,
            commands::delete_persona_memory,
            commands::clear_persona_memory,
            commands::set_session_cast,
            commands::session_cast,
            commands::create_session,
            commands::list_sessions,
            commands::delete_session,
            commands::prune_empty_sessions,
            commands::rename_session,
            commands::reorder_sessions,
            commands::session_messages,
            commands::delete_message,
            commands::export_session,
            commands::set_model_favorite,
            commands::set_session_model,
            commands::set_session_variant,
            commands::set_session_workdir,
            commands::set_session_permission_mode,
            commands::set_session_agent_mode,
            commands::set_session_computer_access,
            commands::set_session_goal,
            commands::session_goal,
            commands::session_summary,
            commands::session_todos,
            commands::set_todos,
            commands::stop_computer,
            commands::resume_computer,
            commands::computer_status,
            // Debug-only; returns an error rather than not existing, so the
            // generated handler list is the same in every build.
            commands::debug_trip_computer_takeover,
            commands::respond_tool_permission,
            commands::respond_question,
            commands::list_tools,
            commands::workspace_info,
            commands::list_workspace_files,
            commands::add_workspace,
            commands::rename_workspace,
            commands::remove_workspace,
            commands::set_search_provider,
            commands::set_search_key,
            commands::search_key_status,
            commands::upsert_mcp_server,
            commands::delete_mcp_server,
            commands::mcp_tools,
            commands::list_skills,
            commands::save_skill,
            commands::delete_skill,
            commands::read_skill,
            commands::set_image_model,
            commands::index_workspace,
            commands::workspace_index_status,
            commands::clear_workspace_index,
            commands::set_embedding_model,
            commands::add_model,
            commands::set_model_spec,
            commands::reset_model_spec,
            commands::usage_capable_providers,
            commands::provider_usage,
            commands::usage_summary,
            commands::check_for_updates,
            commands::download_update,
            commands::apply_update,
            commands::set_session_persona,
            commands::send_message,
            commands::attach_files,
            commands::attach_bytes,
            commands::capture_screen,
            commands::claim_screen,
            commands::set_background_file,
            commands::cancel_stream,
            commands::busy_sessions,
            commands::hide_overlay,
            commands::show_main,
            commands::open_settings,
            commands::quit_app,
            voice::voice_status,
            voice::voice_list_voices,
            voice::voice_speak,
            voice::voice_preview,
            voice::voice_cancel,
            voice::voice_install,
            voice::save_voice_settings,
            voice::set_persona_voice,
    voice::voice_listen_start,
    voice::voice_listen_audio,
    voice::voice_listen_stop,
    voice::voice_listen_status,
            commands::overlay_target,
            commands::list_tasks,
            commands::cancel_task,
            commands::retry_task,
            commands::delete_task,
            commands::start_task,
            commands::list_commands,
            commands::command_output,
            commands::stop_command,
            commands::delete_command,
            commands::list_jobs,
            commands::upsert_job,
            commands::delete_job,
            commands::run_job_now,
            commands::preview_schedule,
            commands::list_memories,
            commands::upsert_memory,
            commands::delete_memory,
            commands::clear_memories,
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
        .run(context)
        .expect("error while running loom");
}
