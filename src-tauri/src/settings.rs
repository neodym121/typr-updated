use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_engine() -> String {
    "groq".to_string()
}

fn default_openai_endpoint() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_openai_model() -> String {
    "whisper-1".to_string()
}

fn default_polza_model() -> String {
    "openai/whisper-large-v3".to_string()
}

fn default_recording_mode() -> String {
    "toggle".to_string()
}

fn default_hotkey() -> String {
    "Ctrl+Shift+Space".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub microphone: String,
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(rename = "groqApiKey", default)]
    pub groq_api_key: String,
    #[serde(rename = "openaiEndpoint", default = "default_openai_endpoint")]
    pub openai_endpoint: String,
    #[serde(rename = "openaiModel", default = "default_openai_model")]
    pub openai_model: String,
    #[serde(rename = "openaiApiKey", default)]
    pub openai_api_key: String,
    #[serde(rename = "polzaApiKey", default)]
    pub polza_api_key: String,
    #[serde(rename = "polzaModel", default = "default_polza_model")]
    pub polza_model: String,
    #[serde(rename = "polzaProvider", default)]
    pub polza_provider: String,
    #[serde(rename = "recordingMode", default = "default_recording_mode")]
    pub recording_mode: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            microphone: "default".to_string(),
            engine: "groq".to_string(),
            groq_api_key: String::new(),
            openai_endpoint: "https://api.openai.com/v1".to_string(),
            openai_model: "whisper-1".to_string(),
            openai_api_key: String::new(),
            polza_api_key: String::new(),
            polza_model: "openai/whisper-large-v3".to_string(),
            polza_provider: String::new(),
            recording_mode: "toggle".to_string(),
            hotkey: "Ctrl+Shift+Space".to_string(),
        }
    }
}

impl Settings {
    pub fn config_path(app_dir: &PathBuf) -> PathBuf {
        app_dir.join("config.json")
    }

    pub fn load(app_dir: &PathBuf) -> Self {
        let path = Self::config_path(app_dir);
        let mut settings: Settings = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        // Migrate legacy "local" engine to "groq"
        if settings.engine == "local" {
            settings.engine = "groq".to_string();
        }
        settings
    }

    pub fn save(&self, app_dir: &PathBuf) -> Result<(), String> {
        let path = Self::config_path(app_dir);
        fs::create_dir_all(app_dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.microphone, "default");
        assert_eq!(settings.engine, "groq");
        assert_eq!(settings.groq_api_key, "");
        assert_eq!(settings.openai_endpoint, "https://api.openai.com/v1");
        assert_eq!(settings.openai_model, "whisper-1");
        assert_eq!(settings.openai_api_key, "");
        assert_eq!(settings.polza_api_key, "");
        assert_eq!(settings.polza_model, "openai/whisper-large-v3");
        assert_eq!(settings.polza_provider, "");
        assert_eq!(settings.recording_mode, "toggle");
        assert_eq!(settings.hotkey, "Ctrl+Shift+Space");
    }

    #[test]
    fn test_save_and_load() {
        let dir = temp_dir().join("typr_test_settings");
        let _ = fs::remove_dir_all(&dir);

        let mut settings = Settings::default();
        settings.engine = "openai".to_string();
        settings.openai_api_key = "test-key-123".to_string();
        settings.openai_model = "whisper-custom".to_string();
        settings.openai_endpoint = "https://custom.ai/v1".to_string();

        settings.save(&dir).unwrap();
        let loaded = Settings::load(&dir);

        assert_eq!(loaded.engine, "openai");
        assert_eq!(loaded.openai_api_key, "test-key-123");
        assert_eq!(loaded.openai_model, "whisper-custom");
        assert_eq!(loaded.openai_endpoint, "https://custom.ai/v1");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_legacy_local_migrated_to_groq() {
        let dir = temp_dir().join("typr_test_legacy");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), r#"{"microphone":"default","engine":"local"}"#).unwrap();

        let loaded = Settings::load(&dir);
        assert_eq!(loaded.engine, "groq");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = temp_dir().join("typr_test_missing");
        let _ = fs::remove_dir_all(&dir);
        let settings = Settings::load(&dir);
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn test_load_corrupt_json_returns_default() {
        let dir = temp_dir().join("typr_test_corrupt");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), "not json").unwrap();

        let settings = Settings::load(&dir);
        assert_eq!(settings, Settings::default());

        let _ = fs::remove_dir_all(&dir);
    }
}
