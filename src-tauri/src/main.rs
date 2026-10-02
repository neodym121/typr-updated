#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use typr_lib::audio;
use typr_lib::i18n;
use typr_lib::keyboard::HotkeyKeys;
use typr_lib::local::{runtime::Paths as LocalPaths, LocalEngine, LocalStatus};
use typr_lib::logger::{self, LogEntry};
use typr_lib::overlay::{self, Placement};
use typr_lib::postprocess::{self, ModelInfo};
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

/// Tray menu items whose text follows the interface language and the hotkey switch
struct TrayMenu {
    hotkey: CheckMenuItem<tauri::Wry>,
    exit: MenuItem<tauri::Wry>,
}

/// Set while the main window is being re-created, so two quick tray clicks
/// don't try to build it twice
static OPENING_MAIN_WINDOW: AtomicBool = AtomicBool::new(false);

fn update_tray_menu(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let language = i18n::resolve(&state.settings.lock().unwrap().language);
    let enabled = state.hotkey_enabled.load(Ordering::SeqCst);
    if let Some(tray) = app.try_state::<TrayMenu>() {
        let _ = tray.hotkey.set_text(i18n::tray_hotkey(language, enabled));
        let _ = tray.hotkey.set_checked(enabled);
        let _ = tray.exit.set_text(i18n::tray_exit(language));
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
        if state.recorder.cancel_recording(app) {
            let settings = state.settings.lock().unwrap().clone();
            typr_lib::transcribe_local::dictation_ended(app, &settings);
        }
        log::info!("Hotkey turned off from the tray, shortcuts are ignored until it is back on");
    }

    sync_overlay(app);
    update_tray_menu(app);
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
    let language_changed = settings.language != old.language;
    app.state::<LocalEngine>().settings_changed(&old, &settings);
    *state.settings.lock().unwrap() = settings;
    if indicator_changed {
        sync_overlay(&app);
    }
    if language_changed {
        update_tray_menu(&app);
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

/// Interface language that suits the system ("en" or "ru"), used while
/// no language is chosen in General
#[tauri::command]
fn get_system_language() -> String {
    i18n::system_language().to_string()
}

/// Models of a post-processing provider. `api_key` is what is typed in the
/// form (maybe not saved yet); Groq and Polza fall back to the Engine key.
#[tauri::command]
async fn list_postprocess_models(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<Vec<ModelInfo>, String> {
    let api_key = if api_key.trim().is_empty() {
        engine_key(&state, &provider)
    } else {
        api_key
    };
    postprocess::list_models(&provider, &api_key)
        .await
        .map_err(|e| {
            log::warn!("Could not load post-processing models: {}", e);
            e
        })
}

fn engine_key(state: &AppState, provider: &str) -> String {
    state.settings.lock().unwrap().engine_key(provider).to_string()
}

// ── Local recognition ───────────────────────────────

#[tauri::command]
async fn local_status(engine: State<'_, LocalEngine>) -> Result<LocalStatus, String> {
    Ok(engine.status())
}

#[tauri::command]
async fn local_download_model(engine: State<'_, LocalEngine>, id: String) -> Result<(), String> {
    engine.download_model(&id)
}

#[tauri::command]
async fn local_download_runtime(engine: State<'_, LocalEngine>) -> Result<(), String> {
    engine.download_runtime()
}

/// `key` is "model:<id>" or "runtime"
#[tauri::command]
async fn local_cancel_download(engine: State<'_, LocalEngine>, key: String) -> Result<(), String> {
    engine.cancel_download(&key);
    Ok(())
}

#[tauri::command]
async fn local_delete_model(engine: State<'_, LocalEngine>, id: String) -> Result<(), String> {
    // Waits for a running dictation to release the model
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.delete_model(&id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn local_delete_runtime(engine: State<'_, LocalEngine>) -> Result<(), String> {
    // Waits for a running dictation to release the model
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.delete_runtime())
        .await
        .map_err(|e| e.to_string())?
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
    let settings = state.settings.lock().unwrap().clone();
    state
        .recorder
        .start_recording(app, &settings.microphone)
        .map_err(|e| {
            log::error!("Failed to start recording: {}", e);
            recorder::notify_error(app, &e);
            e
        })?;
    // The local model loads while the user speaks
    if settings.engine == "local" {
        app.state::<LocalEngine>()
            .preload(&settings.local_model, &settings.local_backend);
    }
    Ok(())
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
    let hotkey_keys = HotkeyKeys::parse(&normalized);

    app.global_shortcut().on_shortcut(normalized.as_str(), move |_app, shortcut, event| {
        let handle = handle.clone();
        let state = handle.state::<AppState>();
        let mode = state.settings.lock().unwrap().recording_mode.clone();
        let push_to_talk = mode == "push-to-talk";

        match event.state {
            ShortcutState::Pressed => {
                log::debug!("Hotkey pressed: {:?} (mode: {})", shortcut, mode);
                // A dictation starts only when nothing but the hotkey is held,
                // so pressing it by accident along with other keys (e.g. W
                // while gaming) does nothing. Stopping is never blocked.
                if state.recorder.get_state() == RecordingState::Ready {
                    let extra = hotkey_keys.extra_keys_held();
                    if !extra.is_empty() {
                        log::info!(
                            "Hotkey ignored: other keys are held too ({})",
                            extra.join(", ")
                        );
                        return;
                    }
                }
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
    // Started by Typr itself to run a local model (local/worker.rs)
    if std::env::args().nth(1).as_deref() == Some(typr_lib::local::worker::ARG) {
        std::process::exit(typr_lib::local::worker::run());
    }

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
        .manage(LocalEngine::new(LocalPaths::new(LocalPaths::default_root())))
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_microphones,
            get_recording_state,
            toggle_recording,
            get_hotkey_enabled,
            get_system_language,
            list_postprocess_models,
            local_status,
            local_download_model,
            local_download_runtime,
            local_cancel_download,
            local_delete_model,
            local_delete_runtime,
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
            app.state::<LocalEngine>().attach(app.handle().clone());
            log_environment(&startup_dir, &startup_settings);

            // System tray: hotkey on/off switch and Exit, in the interface language
            let language = i18n::resolve(&startup_settings.language);
            let hotkey_item = CheckMenuItem::with_id(
                app,
                "toggle-hotkey",
                i18n::tray_hotkey(language, true),
                true,
                true,
                None::<&str>,
            )?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(
                app,
                "exit",
                i18n::tray_exit(language),
                true,
                None::<&str>,
            )?;
            let tray_menu = Menu::with_items(app, &[&hotkey_item, &separator, &quit_i])?;
            app.manage(TrayMenu {
                hotkey: hotkey_item.clone(),
                exit: quit_i.clone(),
            });

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
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "exit" => {
                        log::info!("Tray Exit clicked, terminating application");
                        app.exit(0);
                    }
                    "toggle-hotkey" => {
                        let enabled = !app.state::<AppState>().hotkey_enabled.load(Ordering::SeqCst);
                        set_hotkey_enabled(app, enabled);
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
