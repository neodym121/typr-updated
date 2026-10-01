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

fn default_true() -> bool {
    true
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
    /// Recording indicator that slides in at the top of the screen while dictating
    #[serde(rename = "showIndicator", default = "default_true")]
    pub show_indicator: bool,
    /// Shows the Developer section and turns on log collection
    #[serde(rename = "developerMode", default)]
    pub developer_mode: bool,
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
            show_indicator: true,
            developer_mode: false,
        }
    }
}

fn key_state(key: &str) -> &'static str {
    if key.trim().is_empty() {
        "not set"
    } else {
        "set"
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

    /// One-line description for logs. API keys are never included,
    /// only whether they are set.
    pub fn summary(&self) -> String {
        format!(
            "engine={}, mode={}, hotkey={}, mic='{}', groqKey={}, openaiEndpoint={}, openaiModel={}, openaiKey={}, polzaModel={}, polzaProvider={}, polzaKey={}, showIndicator={}, developerMode={}",
            self.engine,
            self.recording_mode,
            self.hotkey,
            self.microphone,
            key_state(&self.groq_api_key),
            self.openai_endpoint,
            self.openai_model,
            key_state(&self.openai_api_key),
            self.polza_model,
            if self.polza_provider.is_empty() { "auto" } else { self.polza_provider.as_str() },
            key_state(&self.polza_api_key),
            self.show_indicator,
            self.developer_mode
        )
    }

    /// Human-readable list of what differs from `old`, with API keys masked.
    pub fn describe_changes(&self, old: &Settings) -> Vec<String> {
        let mut changes = Vec::new();

        let plain = [
            ("engine", &old.engine, &self.engine),
            ("microphone", &old.microphone, &self.microphone),
            ("recordingMode", &old.recording_mode, &self.recording_mode),
            ("hotkey", &old.hotkey, &self.hotkey),
            ("openaiEndpoint", &old.openai_endpoint, &self.openai_endpoint),
            ("openaiModel", &old.openai_model, &self.openai_model),
            ("polzaModel", &old.polza_model, &self.polza_model),
            ("polzaProvider", &old.polza_provider, &self.polza_provider),
        ];
        for (name, before, after) in plain {
            if before != after {
                changes.push(format!("{}: '{}' → '{}'", name, before, after));
            }
        }

        let secret = [
            ("groqApiKey", &old.groq_api_key, &self.groq_api_key),
            ("openaiApiKey", &old.openai_api_key, &self.openai_api_key),
            ("polzaApiKey", &old.polza_api_key, &self.polza_api_key),
        ];
        for (name, before, after) in secret {
            if before != after {
                changes.push(format!("{} updated ({})", name, key_state(after)));
            }
        }

        if old.show_indicator != self.show_indicator {
            changes.push(format!(
                "showIndicator: {} → {}",
                old.show_indicator, self.show_indicator
            ));
        }

        if old.developer_mode != self.developer_mode {
            changes.push(format!(
                "developerMode: {} → {}",
                old.developer_mode, self.developer_mode
            ));
        }

        changes
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
        assert!(settings.show_indicator);
        assert!(!settings.developer_mode);
    }

    #[test]
    fn test_logs_never_contain_api_keys() {
        let old = Settings::default();
        let mut new = Settings::default();
        new.groq_api_key = "gsk_secret".to_string();
        new.engine = "polza".to_string();

        let summary = new.summary();
        let changes = new.describe_changes(&old).join("; ");
        assert!(!summary.contains("gsk_secret"));
        assert!(!changes.contains("gsk_secret"));
        assert!(changes.contains("engine: 'groq' → 'polza'"));
        assert!(changes.contains("groqApiKey updated (set)"));
    }

    #[test]
    fn test_missing_developer_mode_defaults_to_false() {
        let settings: Settings =
            serde_json::from_str(r#"{"microphone":"default","engine":"groq"}"#).unwrap();
        assert!(!settings.developer_mode);
    }

    #[test]
    fn test_missing_show_indicator_defaults_to_true() {
        let settings: Settings =
            serde_json::from_str(r#"{"microphone":"default","engine":"groq"}"#).unwrap();
        assert!(settings.show_indicator);
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
