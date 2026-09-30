#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use typr_lib::audio;
use typr_lib::logger::{self, LogEntry};
use typr_lib::recorder::{self, Recorder, RecordingState};
use typr_lib::settings::Settings;

struct AppState {
    recorder: Recorder,
    settings: Mutex<Settings>,
    app_dir: PathBuf,
    /// Whether the push-to-talk hotkey is physically held down right now
    ptt_held: AtomicBool,
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

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    } else {
        log::warn!("Main window not found");
    }
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
    if settings.hotkey != old.hotkey {
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

    *state.settings.lock().unwrap() = settings;
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

    tauri::Builder::default()
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
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_microphones,
            get_recording_state,
            toggle_recording,
            get_logs,
            clear_logs,
            frontend_log,
        ])
        .on_window_event(|window, event| {
            // Closing the main window hides it to the tray; otherwise it would be
            // destroyed and the tray icon / second launch could not bring it back.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                    log::info!("Main window hidden to the tray");
                }
            }
        })
        .setup(move |app| {
            logger::attach_app(app.handle().clone());
            log_environment(&startup_dir, &startup_settings);

            // Setup System Tray with Logo and Exit menu item
            let quit_i = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&quit_i])?;

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
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "exit" {
                        log::info!("Tray Exit clicked, terminating application");
                        app.exit(0);
                    }
                })
                .build(app)?;

            // Create the overlay window (small mic icon, top-right, always on top)
            let monitor = app.primary_monitor().ok().flatten();
            let (x, y) = if let Some(m) = monitor {
                let size = m.size();
                let scale = m.scale_factor();
                let logical_w = size.width as f64 / scale;
                ((logical_w - 60.0) as i32, 10_i32)
            } else {
                (1380, 10)
            };

            let overlay = WebviewWindowBuilder::new(
                app,
                "overlay",
                WebviewUrl::App("src/overlay.html".into()),
            )
            .title("")
            .inner_size(50.0, 50.0)
            .position(x as f64, y as f64)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .shadow(false)
            .build();

            match overlay {
                Ok(_) => log::info!("Overlay window created at ({}, {})", x, y),
                Err(e) => log::error!("Failed to create overlay: {}", e),
            }

            if let Err(e) = register_hotkey(app.handle(), &initial_hotkey) {
                log::error!("Failed to register initial global shortcut: {}", e);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
