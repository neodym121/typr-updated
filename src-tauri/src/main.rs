#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use typr_lib::audio;
use typr_lib::logger::{self, LogEntry};
use typr_lib::overlay::{self, Placement};
use typr_lib::recorder::{self, Recorder, RecordingState};
use typr_lib::settings::Settings;

struct AppState {
    recorder: Recorder,
    settings: Mutex<Settings>,
    app_dir: PathBuf,
    /// Whether the push-to-talk hotkey is physically held down right now
    ptt_held: AtomicBool,
    /// Global hotkey listening; switched off from the tray (e.g. while gaming)
    hotkey_enabled: AtomicBool,
}

/// Set while the main window is being re-created, so two quick tray clicks
/// don't try to build it twice
static OPENING_MAIN_WINDOW: AtomicBool = AtomicBool::new(false);

fn hotkey_menu_text(enabled: bool) -> &'static str {
    if enabled {
        "Hotkey: On"
    } else {
        "Hotkey: Off"
    }
}

fn get_app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.typr.app")
}

fn log_environment(app_dir: &PathBuf, settings: &Settings) {
    log::info!(
        "Typr v{} on {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    log::info!("Config folder: {}", app_dir.display());
    log::info!("Settings: {}", settings.summary());
}

/// Focuses the main window, re-creating it if it was closed. Closing destroys
/// the window together with its WebView, so Typr idles in the tray without a
/// browser process.
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }

    if OPENING_MAIN_WINDOW.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    // Building a window inside an event handler deadlocks on Windows, so use a thread
    std::thread::spawn(move || {
        let config = app
            .config()
            .app
            .windows
            .iter()
            .find(|window| window.label == "main")
            .cloned();
        match config {
            Some(config) => {
                log::info!("Opening the main window");
                let built = WebviewWindowBuilder::from_config(&app, &config)
                    .and_then(|builder| builder.build());
                match built {
                    Ok(window) => {
                        let _ = window.set_focus();
                    }
                    Err(e) => log::error!("Failed to open the main window: {}", e),
                }
            }
            None => log::error!("Main window is missing from tauri.conf.json"),
        }
        OPENING_MAIN_WINDOW.store(false, Ordering::SeqCst);
    });
}

/// Turns global hotkey listening on or off (tray menu).
fn set_hotkey_enabled(app: &tauri::AppHandle, enabled: bool) {
    let state = app.state::<AppState>();
    state.hotkey_enabled.store(enabled, Ordering::SeqCst);

    if enabled {
        let hotkey = state.settings.lock().unwrap().hotkey.clone();
        if let Err(e) = register_hotkey(app, &hotkey) {
            log::error!("Failed to turn the hotkey back on: {}", e);
        } else {
            log::info!("Hotkey turned on from the tray");
        }
    } else {
        if let Err(e) = app.global_shortcut().unregister_all() {
            log::warn!("Failed to unregister shortcuts: {}", e);
        }
        state.ptt_held.store(false, Ordering::SeqCst);
        // Never paste into whatever is focused now (e.g. a game)
        state.recorder.cancel_recording(app);
        log::info!("Hotkey turned off from the tray, shortcuts are ignored until it is back on");
    }

    sync_overlay(app);
    if let Err(e) = app.emit("hotkey-enabled", enabled) {
        log::debug!("Failed to emit hotkey-enabled: {}", e);
    }
}

/// The recording indicator may appear only while it is switched on in General
/// and the hotkey is on (nothing pops up over games while it's off in the tray).
fn sync_overlay(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let allowed = state.hotkey_enabled.load(Ordering::SeqCst)
        && state.settings.lock().unwrap().show_indicator;
    overlay::set_enabled(app, allowed);
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(
    app: tauri::AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Result<(), String> {
    let old = state.settings.lock().unwrap().clone();
    // While the hotkey is off (tray), the new one is registered when it's turned back on
    if settings.hotkey != old.hotkey && state.hotkey_enabled.load(Ordering::SeqCst) {
        if let Err(e) = register_hotkey(&app, &settings.hotkey) {
            log::error!("Failed to register new hotkey '{}': {}", settings.hotkey, e);
            let _ = register_hotkey(&app, &old.hotkey);
            return Err(e);
        }
    }
    if let Err(e) = settings.save(&state.app_dir) {
        log::error!("Failed to save settings: {}", e);
        return Err(e);
    }

    if settings.developer_mode != old.developer_mode {
        logger::set_enabled(settings.developer_mode);
        if settings.developer_mode {
            log::info!("Developer mode enabled, logging started");
            log_environment(&state.app_dir, &settings);
        }
    }

    let changes = settings.describe_changes(&old);
    if !changes.is_empty() {
        log::info!("Settings saved: {}", changes.join("; "));
    }

    let indicator_changed = settings.show_indicator != old.show_indicator;
    *state.settings.lock().unwrap() = settings;
    if indicator_changed {
        sync_overlay(&app);
    }
    Ok(())
}

#[tauri::command]
fn list_microphones() -> Vec<audio::MicDevice> {
    let mics = audio::list_microphones();
    log::debug!(
        "Input devices ({}): {}",
        mics.len(),
        mics.iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    mics
}

#[tauri::command]
fn get_recording_state(state: State<AppState>) -> RecordingState {
    state.recorder.get_state()
}

#[tauri::command]
async fn toggle_recording(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    do_toggle_recording(&app, &state).await
}

#[tauri::command]
fn get_hotkey_enabled(state: State<AppState>) -> bool {
    state.hotkey_enabled.load(Ordering::SeqCst)
}

#[tauri::command]
fn get_logs() -> Vec<LogEntry> {
    logger::entries()
}

#[tauri::command]
fn clear_logs() {
    logger::clear();
}

#[tauri::command]
fn frontend_log(level: String, message: String) {
    logger::record(logger::parse_level(&level), "ui", message);
}

fn begin_recording(app: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    let mic = state.settings.lock().unwrap().microphone.clone();
    state.recorder.start_recording(app, &mic).map_err(|e| {
        log::error!("Failed to start recording: {}", e);
        recorder::notify_error(app, &e);
        e
    })
}

async fn finish_recording(app: &tauri::AppHandle, state: &AppState) -> Result<String, String> {
    let settings = state.settings.lock().unwrap().clone();
    state
        .recorder
        .stop_and_transcribe(app, &settings, &state.app_dir)
        .await
}

/// Shared logic for toggle recording, used by both the Tauri command and hotkey handler.
async fn do_toggle_recording(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<String, String> {
    let current_state = state.recorder.get_state();
    match current_state {
        RecordingState::Ready => {
            begin_recording(app, state)?;
            Ok("recording".to_string())
        }
        RecordingState::Recording => finish_recording(app, state).await,
        RecordingState::Transcribing => {
            log::warn!("Hotkey ignored: still transcribing the previous recording");
            Err("Currently transcribing, please wait".to_string())
        }
    }
}

async fn push_to_talk_pressed(app: &tauri::AppHandle, state: &AppState) {
    let current = state.recorder.get_state();
    if current != RecordingState::Ready {
        log::debug!("Push-to-talk press ignored, recorder is {:?}", current);
        return;
    }
    if begin_recording(app, state).is_err() {
        return;
    }
    // The key may have been released while the microphone was opening;
    // the release handler then saw no recording, so stop it here.
    if !state.ptt_held.load(Ordering::SeqCst) {
        log::debug!("Push-to-talk key was released while the microphone was opening");
        if let Err(e) = finish_recording(app, state).await {
            log::debug!("Push-to-talk stop: {}", e);
        }
    }
}

async fn push_to_talk_released(app: &tauri::AppHandle, state: &AppState) {
    if state.recorder.get_state() != RecordingState::Recording {
        return;
    }
    if let Err(e) = finish_recording(app, state).await {
        log::debug!("Push-to-talk stop: {}", e);
    }
}

fn normalize_hotkey(hotkey: &str) -> String {
    let parts: Vec<&str> = hotkey.split('+').collect();
    let mut normalized_parts = Vec::new();

    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Single letter A..Z or a..z -> KeyA..KeyZ
        if trimmed.len() == 1 {
            let ch = trimmed.chars().next().unwrap();
            if ch.is_ascii_alphabetic() {
                normalized_parts.push(format!("Key{}", ch.to_ascii_uppercase()));
                continue;
            } else if ch.is_ascii_digit() {
                normalized_parts.push(format!("Digit{}", ch));
                continue;
            }
        }

        if trimmed.eq_ignore_ascii_case("ctrl") || trimmed.eq_ignore_ascii_case("control") {
            normalized_parts.push("Ctrl".to_string());
        } else if trimmed.eq_ignore_ascii_case("shift") {
            normalized_parts.push("Shift".to_string());
        } else if trimmed.eq_ignore_ascii_case("alt") {
            normalized_parts.push("Alt".to_string());
        } else if trimmed.eq_ignore_ascii_case("super")
            || trimmed.eq_ignore_ascii_case("meta")
            || trimmed.eq_ignore_ascii_case("cmd")
        {
            normalized_parts.push("Super".to_string());
        } else if trimmed.eq_ignore_ascii_case("cmdorctrl") {
            normalized_parts.push("CmdOrCtrl".to_string());
        } else if trimmed.eq_ignore_ascii_case("space") {
            normalized_parts.push("Space".to_string());
        } else {
            normalized_parts.push(trimmed.to_string());
        }
    }

    normalized_parts.join("+")
}

fn register_hotkey(app: &tauri::AppHandle, hotkey_str: &str) -> Result<(), String> {
    let normalized = normalize_hotkey(hotkey_str);
    let handle = app.clone();

    if let Err(e) = app.global_shortcut().unregister_all() {
        log::warn!("Failed to unregister previous shortcuts: {}", e);
    }

    log::info!("Registering global shortcut: {}", normalized);

    app.global_shortcut().on_shortcut(normalized.as_str(), move |_app, shortcut, event| {
        let handle = handle.clone();
        let state = handle.state::<AppState>();
        let mode = state.settings.lock().unwrap().recording_mode.clone();
        let push_to_talk = mode == "push-to-talk";

        match event.state {
            ShortcutState::Pressed => {
                log::debug!("Hotkey pressed: {:?} (mode: {})", shortcut, mode);
                if push_to_talk {
                    state.ptt_held.store(true, Ordering::SeqCst);
                }
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    if push_to_talk {
                        push_to_talk_pressed(&handle, state.inner()).await;
                    } else {
                        match do_toggle_recording(&handle, state.inner()).await {
                            Ok(result) => log::debug!("Toggle result: {}", result),
                            Err(e) => log::debug!("Toggle: {}", e),
                        }
                    }
                });
            }
            ShortcutState::Released => {
                log::debug!("Hotkey released: {:?} (mode: {})", shortcut, mode);
                if push_to_talk {
                    state.ptt_held.store(false, Ordering::SeqCst);
                    tauri::async_runtime::spawn(async move {
                        let state = handle.state::<AppState>();
                        push_to_talk_released(&handle, state.inner()).await;
                    });
                }
            }
        }
    }).map_err(|e| format!("Failed to register shortcut '{}': {}", normalized, e))?;

    log::info!("Global shortcut registered: {}", normalized);
    Ok(())
}

fn main() {
    logger::init();

    let app_dir = get_app_dir();
    let settings = Settings::load(&app_dir);
    logger::set_enabled(settings.developer_mode);
    let initial_hotkey = settings.hotkey.clone();
    let startup_settings = settings.clone();
    let startup_dir = app_dir.clone();

    let app = tauri::Builder::default()
        // Must be the first plugin, so a second launch is intercepted early
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("Second instance launched, focusing the existing window");
            show_main_window(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            recorder: Recorder::new(),
            settings: Mutex::new(settings),
            app_dir,
            ptt_held: AtomicBool::new(false),
            hotkey_enabled: AtomicBool::new(true),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_microphones,
            get_recording_state,
            toggle_recording,
            get_hotkey_enabled,
            get_logs,
            clear_logs,
            frontend_log,
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::Destroyed = event {
                    log::info!("Main window closed and its WebView released, Typr keeps running in the tray");
                }
            }
        })
        .setup(move |app| {
            logger::attach_app(app.handle().clone());
            log_environment(&startup_dir, &startup_settings);

            // System tray: hotkey on/off switch and Exit
            let hotkey_item = Arc::new(CheckMenuItem::with_id(
                app,
                "toggle-hotkey",
                hotkey_menu_text(true),
                true,
                true,
                None::<&str>,
            )?);
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&*hotkey_item, &separator, &quit_i])?;
            let menu_hotkey_item = hotkey_item.clone();

            let tray_icon = match app.default_window_icon() {
                Some(i) => i.clone(),
                None => tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("tray icon"),
            };

            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .tooltip("Typr")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button, .. } = event {
                        if button == tauri::tray::MouseButton::Left {
                            log::debug!("Tray icon clicked, showing the main window");
                            show_main_window(tray.app_handle());
                        }
                    }
                })
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "exit" => {
                        log::info!("Tray Exit clicked, terminating application");
                        app.exit(0);
                    }
                    "toggle-hotkey" => {
                        let enabled = !app.state::<AppState>().hotkey_enabled.load(Ordering::SeqCst);
                        set_hotkey_enabled(app, enabled);
                        let _ = menu_hotkey_item.set_checked(enabled);
                        let _ = menu_hotkey_item.set_text(hotkey_menu_text(enabled));
                    }
                    _ => {}
                })
                .build(app)?;

            // Recording indicator (small mic, top-right, always on top). It waits
            // behind the top edge of the screen and slides in only while dictating
            let placement = match app.primary_monitor().ok().flatten() {
                Some(monitor) => Placement::for_monitor(
                    monitor.position().x,
                    monitor.position().y,
                    monitor.size().width,
                    monitor.scale_factor(),
                ),
                None => Placement::fallback(),
            };
            overlay::create(app.handle(), placement);
            sync_overlay(app.handle());

            if let Err(e) = register_hotkey(app.handle(), &initial_hotkey) {
                log::error!("Failed to register initial global shortcut: {}", e);
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, event| {
        // Closing the main window leaves no windows open; Typr keeps running
        // in the tray. Only an explicit exit (tray → Exit) carries a code.
        if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
