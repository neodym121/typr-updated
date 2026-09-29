#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use typr_lib::audio;
use typr_lib::recorder::{Recorder, RecordingState};
use typr_lib::settings::Settings;

struct AppState {
    recorder: Recorder,
    settings: Mutex<Settings>,
    app_dir: PathBuf,
}

fn get_app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.typr.app")
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
    let old_hotkey = state.settings.lock().unwrap().hotkey.clone();
    if settings.hotkey != old_hotkey {
        if let Err(e) = register_hotkey(&app, &settings.hotkey) {
            eprintln!("[Typr] Failed to register new hotkey: {}", e);
            let _ = register_hotkey(&app, &old_hotkey);
            return Err(e);
        }
    }
    settings.save(&state.app_dir)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn list_microphones() -> Vec<audio::MicDevice> {
    audio::list_microphones()
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

/// Shared logic for toggle recording, used by both the Tauri command and hotkey handler.
async fn do_toggle_recording(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<String, String> {
    let current_state = state.recorder.get_state();
    match current_state {
        RecordingState::Ready => {
            let mic = state.settings.lock().unwrap().microphone.clone();
            state.recorder.start_recording(app, &mic)?;
            Ok("recording".to_string())
        }
        RecordingState::Recording => {
            let settings = state.settings.lock().unwrap().clone();
            let result = state
                .recorder
                .stop_and_transcribe(app, &settings, &state.app_dir)
                .await?;
            Ok(result)
        }
        RecordingState::Transcribing => {
            Err("Currently transcribing, please wait".to_string())
        }
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

    let _ = app.global_shortcut().unregister_all();

    println!("[Typr] Registering global shortcut: {}", normalized);

    app.global_shortcut().on_shortcut(normalized.as_str(), move |_app, shortcut, event| {
        println!("[Typr] Hotkey event: {:?} state={:?}", shortcut, event.state);
        let handle = handle.clone();
        let state = handle.state::<AppState>();
        let mode = state.settings.lock().unwrap().recording_mode.clone();

        match event.state {
            ShortcutState::Pressed => {
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    match mode.as_str() {
                        "toggle" => {
                            println!("[Typr] Toggle mode: calling do_toggle_recording");
                            match do_toggle_recording(&handle, state.inner()).await {
                                Ok(result) => println!("[Typr] Toggle result: {}", result),
                                Err(e) => eprintln!("[Typr] Toggle error: {}", e),
                            }
                        }
                        "push-to-talk" => {
                            let current = state.recorder.get_state();
                            println!("[Typr] PTT mode, current state: {:?}", current);
                            if current == RecordingState::Ready {
                                let mic = state
                                    .settings
                                    .lock()
                                    .unwrap()
                                    .microphone
                                    .clone();
                                match state.recorder.start_recording(&handle, &mic) {
                                    Ok(_) => println!("[Typr] Recording started"),
                                    Err(e) => eprintln!("[Typr] Start recording error: {}", e),
                                }
                            }
                        }
                        _ => {}
                    }
                });
            }
            ShortcutState::Released => {
                if mode == "push-to-talk" {
                    tauri::async_runtime::spawn(async move {
                        let state = handle.state::<AppState>();
                        let current = state.recorder.get_state();
                        if current == RecordingState::Recording {
                            let settings =
                                state.settings.lock().unwrap().clone();
                            match state.recorder.stop_and_transcribe(
                                &handle,
                                &settings,
                                &state.app_dir,
                            ).await {
                                Ok(result) => println!("[Typr] Transcription: {}", result),
                                Err(e) => eprintln!("[Typr] Transcription error: {}", e),
                            }
                        }
                    });
                }
            }
        }
    }).map_err(|e| format!("Failed to register shortcut '{}': {}", normalized, e))?;

    Ok(())
}

fn main() {
    let app_dir = get_app_dir();
    let settings = Settings::load(&app_dir);
    let initial_hotkey = settings.hotkey.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Another instance was launched — show and focus the existing main window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(AppState {
            recorder: Recorder::new(),
            settings: Mutex::new(settings),
            app_dir,
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_microphones,
            get_recording_state,
            toggle_recording,
        ])
        .setup(move |app| {
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
                            if let Some(window) = tray.app_handle().get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "exit" {
                        println!("[Typr] Tray Exit clicked, terminating application.");
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
                Ok(_) => println!("[Typr] Overlay window created"),
                Err(e) => eprintln!("[Typr] Failed to create overlay: {}", e),
            }

            if let Err(e) = register_hotkey(&app.handle(), &initial_hotkey) {
                eprintln!("[Typr] ERROR: Failed to register initial global shortcut: {}", e);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
