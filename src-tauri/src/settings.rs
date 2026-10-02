use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_engine() -> String {
    "groq".to_string()
}

fn default_groq_model() -> String {
    "whisper-large-v3-turbo".to_string()
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

fn default_assemblyai_model() -> String {
    "universal-3-5-pro".to_string()
}

fn default_local_model() -> String {
    crate::local::catalog::DEFAULT_MODEL.to_string()
}

fn default_local_unload() -> String {
    "5m".to_string()
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

fn default_post_process_provider() -> String {
    "gemini".to_string()
}

fn default_post_process_preset() -> String {
    "proper".to_string()
}

/// API key and chosen model of one post-processing provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PostProcessProvider {
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    /// Sub-provider that serves the model (Polza only); empty means automatic
    #[serde(rename = "providerId", default, skip_serializing_if = "String::is_empty")]
    pub provider_id: String,
}

/// A language model rewrites the transcribed text before it is pasted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostProcess {
    #[serde(default)]
    pub enabled: bool,
    /// "gemini" | "openrouter" | "groq" | "polza"
    #[serde(default = "default_post_process_provider")]
    pub provider: String,
    /// "chill" | "proper" | "custom"
    #[serde(default = "default_post_process_preset")]
    pub preset: String,
    /// The user's own instructions for the "custom" preset
    #[serde(rename = "customPrompt", default)]
    pub custom_prompt: String,
    #[serde(default)]
    pub gemini: PostProcessProvider,
    #[serde(default)]
    pub openrouter: PostProcessProvider,
    #[serde(default)]
    pub groq: PostProcessProvider,
    #[serde(default)]
    pub polza: PostProcessProvider,
}

impl Default for PostProcess {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_post_process_provider(),
            preset: default_post_process_preset(),
            custom_prompt: String::new(),
            gemini: PostProcessProvider::default(),
            openrouter: PostProcessProvider::default(),
            groq: PostProcessProvider::default(),
            polza: PostProcessProvider::default(),
        }
    }
}

impl PostProcess {
    pub fn provider_settings(&self, provider: &str) -> Option<&PostProcessProvider> {
        match provider {
            "gemini" => Some(&self.gemini),
            "openrouter" => Some(&self.openrouter),
            "groq" => Some(&self.groq),
            "polza" => Some(&self.polza),
            _ => None,
        }
    }

    fn providers(&self) -> [(&'static str, &PostProcessProvider); 4] {
        [
            ("gemini", &self.gemini),
            ("openrouter", &self.openrouter),
            ("groq", &self.groq),
            ("polza", &self.polza),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub microphone: String,
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(rename = "groqApiKey", default)]
    pub groq_api_key: String,
    /// "whisper-large-v3-turbo" or "whisper-large-v3"
    #[serde(rename = "groqModel", default = "default_groq_model")]
    pub groq_model: String,
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
    #[serde(rename = "assemblyaiApiKey", default)]
    pub assemblyai_api_key: String,
    /// "universal-3-5-pro" (no Russian) or "universal-2"
    #[serde(rename = "assemblyaiModel", default = "default_assemblyai_model")]
    pub assemblyai_model: String,
    /// Local engine: the model used for dictation (an id from local::catalog)
    #[serde(rename = "localModel", default = "default_local_model")]
    pub local_model: String,
    /// Local engine: "vulkan" | "cpu", or empty for the one recommended
    /// for this computer
    #[serde(rename = "localBackend", default)]
    pub local_backend: String,
    /// Local engine: when an idle model leaves memory:
    /// "immediate" | "30s" | "5m" | "10m" | "never"
    #[serde(rename = "localUnload", default = "default_local_unload")]
    pub local_unload: String,
    #[serde(rename = "recordingMode", default = "default_recording_mode")]
    pub recording_mode: String,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    /// Adds a space after the pasted text, so the next dictation doesn't stick to it
    #[serde(rename = "appendSpace", default)]
    pub append_space: bool,
    /// Recording indicator that slides in at the top of the screen while dictating
    #[serde(rename = "showIndicator", default = "default_true")]
    pub show_indicator: bool,
    /// Interface language: "en", "ru", or empty to follow the system
    #[serde(default)]
    pub language: String,
    #[serde(rename = "postProcess", default)]
    pub post_process: PostProcess,
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
            groq_model: "whisper-large-v3-turbo".to_string(),
            openai_endpoint: "https://api.openai.com/v1".to_string(),
            openai_model: "whisper-1".to_string(),
            openai_api_key: String::new(),
            polza_api_key: String::new(),
            polza_model: "openai/whisper-large-v3".to_string(),
            polza_provider: String::new(),
            assemblyai_api_key: String::new(),
            assemblyai_model: "universal-3-5-pro".to_string(),
            local_model: default_local_model(),
            local_backend: String::new(),
            local_unload: default_local_unload(),
            recording_mode: "toggle".to_string(),
            hotkey: "Ctrl+Shift+Space".to_string(),
            append_space: false,
            show_indicator: true,
            language: String::new(),
            post_process: PostProcess::default(),
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
        // The "official" post-processing preset became "proper"
        if settings.post_process.preset == "official" {
            settings.post_process.preset = "proper".to_string();
        }
        settings
    }

    /// The Engine key of a provider that is also used for post-processing.
    pub fn engine_key(&self, provider: &str) -> &str {
        match provider {
            "groq" => self.groq_api_key.as_str(),
            "polza" => self.polza_api_key.as_str(),
            _ => "",
        }
    }

    /// API key for post-processing with `provider`. Groq and Polza fall back
    /// to the key from Engine when no separate one is set.
    pub fn post_process_key(&self, provider: &str) -> String {
        let own = self
            .post_process
            .provider_settings(provider)
            .map(|p| p.api_key.trim())
            .unwrap_or("");
        if own.is_empty() {
            self.engine_key(provider).trim().to_string()
        } else {
            own.to_string()
        }
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
            "engine={}, mode={}, hotkey={}, appendSpace={}, mic='{}', groqModel={}, groqKey={}, openaiEndpoint={}, openaiModel={}, openaiKey={}, polzaModel={}, polzaProvider={}, polzaKey={}, assemblyaiModel={}, assemblyaiKey={}, localModel={}, localBackend={}, localUnload={}, showIndicator={}, language={}, postProcess={}, developerMode={}",
            self.engine,
            self.recording_mode,
            self.hotkey,
            self.append_space,
            self.microphone,
            self.groq_model,
            key_state(&self.groq_api_key),
            self.openai_endpoint,
            self.openai_model,
            key_state(&self.openai_api_key),
            self.polza_model,
            if self.polza_provider.is_empty() { "auto" } else { self.polza_provider.as_str() },
            key_state(&self.polza_api_key),
            self.assemblyai_model,
            key_state(&self.assemblyai_api_key),
            self.local_model,
            if self.local_backend.is_empty() { "auto" } else { self.local_backend.as_str() },
            self.local_unload,
            self.show_indicator,
            if self.language.is_empty() { "system" } else { self.language.as_str() },
            self.post_process_summary(),
            self.developer_mode
        )
    }

    fn post_process_summary(&self) -> String {
        let pp = &self.post_process;
        let model = pp
            .provider_settings(&pp.provider)
            .map(|p| p.model.as_str())
            .unwrap_or("");
        format!(
            "{} (provider {}, model {}, preset {}, key {})",
            if pp.enabled { "on" } else { "off" },
            pp.provider,
            if model.is_empty() { "none" } else { model },
            pp.preset,
            key_state(&self.post_process_key(&pp.provider))
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
            ("groqModel", &old.groq_model, &self.groq_model),
            ("openaiEndpoint", &old.openai_endpoint, &self.openai_endpoint),
            ("openaiModel", &old.openai_model, &self.openai_model),
            ("polzaModel", &old.polza_model, &self.polza_model),
            ("polzaProvider", &old.polza_provider, &self.polza_provider),
            ("assemblyaiModel", &old.assemblyai_model, &self.assemblyai_model),
            ("localModel", &old.local_model, &self.local_model),
            ("localBackend", &old.local_backend, &self.local_backend),
            ("localUnload", &old.local_unload, &self.local_unload),
            ("language", &old.language, &self.language),
            ("postProcess.provider", &old.post_process.provider, &self.post_process.provider),
            ("postProcess.preset", &old.post_process.preset, &self.post_process.preset),
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
            ("assemblyaiApiKey", &old.assemblyai_api_key, &self.assemblyai_api_key),
        ];
        for (name, before, after) in secret {
            if before != after {
                changes.push(format!("{} updated ({})", name, key_state(after)));
            }
        }

        for ((name, before), (_, after)) in old
            .post_process
            .providers()
            .into_iter()
            .zip(self.post_process.providers())
        {
            if before.model != after.model {
                changes.push(format!(
                    "postProcess.{}.model: '{}' → '{}'",
                    name, before.model, after.model
                ));
            }
            if before.api_key != after.api_key {
                changes.push(format!(
                    "postProcess.{}.apiKey updated ({})",
                    name,
                    key_state(&after.api_key)
                ));
            }
            if before.provider_id != after.provider_id {
                changes.push(format!(
                    "postProcess.{}.providerId: '{}' → '{}'",
                    name, before.provider_id, after.provider_id
                ));
            }
        }

        if old.post_process.custom_prompt != self.post_process.custom_prompt {
            changes.push(format!(
                "postProcess.customPrompt updated ({} characters)",
                self.post_process.custom_prompt.chars().count()
            ));
        }

        if old.post_process.enabled != self.post_process.enabled {
            changes.push(format!(
                "postProcess.enabled: {} → {}",
                old.post_process.enabled, self.post_process.enabled
            ));
        }

        if old.append_space != self.append_space {
            changes.push(format!(
                "appendSpace: {} → {}",
                old.append_space, self.append_space
            ));
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
        assert_eq!(settings.groq_model, "whisper-large-v3-turbo");
        assert!(!settings.append_space);
        assert!(!settings.post_process.enabled);
        assert_eq!(settings.post_process.provider, "gemini");
        assert_eq!(settings.post_process.preset, "proper");
        assert_eq!(settings.openai_endpoint, "https://api.openai.com/v1");
        assert_eq!(settings.openai_model, "whisper-1");
        assert_eq!(settings.openai_api_key, "");
        assert_eq!(settings.polza_api_key, "");
        assert_eq!(settings.polza_model, "openai/whisper-large-v3");
        assert_eq!(settings.polza_provider, "");
        assert_eq!(settings.assemblyai_api_key, "");
        assert_eq!(settings.assemblyai_model, "universal-3-5-pro");
        assert_eq!(settings.local_model, "parakeet-tdt-0.6b-v3");
        assert_eq!(settings.local_backend, "");
        assert_eq!(settings.local_unload, "5m");
        assert_eq!(settings.language, "");
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
        new.assemblyai_api_key = "aai_secret".to_string();
        new.engine = "polza".to_string();

        let summary = new.summary();
        let changes = new.describe_changes(&old).join("; ");
        assert!(!summary.contains("gsk_secret"));
        assert!(!changes.contains("gsk_secret"));
        assert!(!summary.contains("aai_secret"));
        assert!(!changes.contains("aai_secret"));
        assert!(changes.contains("assemblyaiApiKey updated (set)"));
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
    fn test_missing_post_process_gets_defaults() {
        let settings: Settings =
            serde_json::from_str(r#"{"microphone":"default","engine":"groq"}"#).unwrap();
        assert_eq!(settings.post_process, PostProcess::default());
    }

    #[test]
    fn test_official_preset_migrated_to_proper() {
        let dir = temp_dir().join("typr_test_preset");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("config.json"),
            r#"{"microphone":"default","postProcess":{"preset":"official"}}"#,
        )
        .unwrap();

        let loaded = Settings::load(&dir);
        assert_eq!(loaded.post_process.preset, "proper");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_empty_provider_id_is_not_saved() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!json.contains("providerId"));
    }

    #[test]
    fn test_post_process_key_falls_back_to_engine_key() {
        let mut settings = Settings::default();
        settings.groq_api_key = "gsk_engine".to_string();
        assert_eq!(settings.post_process_key("groq"), "gsk_engine");
        settings.post_process.groq.api_key = "gsk_own".to_string();
        assert_eq!(settings.post_process_key("groq"), "gsk_own");
        assert_eq!(settings.post_process_key("gemini"), "");
    }

    #[test]
    fn test_post_process_keys_are_masked_in_logs() {
        let old = Settings::default();
        let mut new = Settings::default();
        new.post_process.gemini.api_key = "AIza_secret".to_string();
        new.post_process.enabled = true;
        let changes = new.describe_changes(&old).join("; ");
        assert!(!changes.contains("AIza_secret"));
        assert!(!new.summary().contains("AIza_secret"));
        assert!(changes.contains("postProcess.gemini.apiKey updated (set)"));
        assert!(changes.contains("postProcess.enabled: false → true"));
    }

    #[test]
    fn test_missing_local_settings_get_defaults() {
        let settings: Settings =
            serde_json::from_str(r#"{"microphone":"default","engine":"local"}"#).unwrap();
        assert_eq!(settings.engine, "local");
        assert_eq!(settings.local_model, "parakeet-tdt-0.6b-v3");
        assert_eq!(settings.local_backend, "");
        assert_eq!(settings.local_unload, "5m");
    }

    #[test]
    fn test_local_changes_are_logged() {
        let old = Settings::default();
        let mut new = Settings::default();
        new.local_backend = "vulkan".to_string();
        new.local_unload = "never".to_string();
        let changes = new.describe_changes(&old).join("; ");
        assert!(changes.contains("localBackend: '' → 'vulkan'"));
        assert!(changes.contains("localUnload: '5m' → 'never'"));
        assert!(new.summary().contains("localBackend=vulkan"));
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
    fn test_local_engine_survives_a_restart() {
        let dir = temp_dir().join("typr_test_local_engine");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), r#"{"microphone":"default","engine":"local"}"#).unwrap();

        let loaded = Settings::load(&dir);
        assert_eq!(loaded.engine, "local");

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
