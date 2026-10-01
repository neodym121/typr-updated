pub mod settings;
pub mod logger;
pub mod net;
pub mod audio;
pub mod transcribe_groq;
pub mod transcribe_openai;
pub mod transcribe_polza;
pub mod transcribe_assemblyai;
pub mod cleanup;
pub mod i18n;
pub mod keyboard;
pub mod overlay;
pub mod paste;
pub mod recorder;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
