//! In-app log collector that backs the Developer section.
//!
//! Every `log::info!` / `log::warn!` / ... call in the app (plus Info-and-above
//! records from dependencies) goes through [`AppLogger`]. Records are always
//! mirrored to stdout/stderr; while developer mode is on they are also kept in
//! a ring buffer and streamed to the main window as `log-entry` events.

use serde::Serialize;
use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

const MAX_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub id: u64,
    /// Unix time in milliseconds
    pub ts: u64,
    /// "error" | "warn" | "info" | "debug"
    pub level: String,
    /// Short origin label, e.g. "recorder", "audio", "ui"
    pub source: String,
    pub message: String,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static BUFFER: Mutex<VecDeque<LogEntry>> = Mutex::new(VecDeque::new());
static APP: OnceLock<AppHandle> = OnceLock::new();
static LOGGER: AppLogger = AppLogger;

thread_local! {
    // Emitting an event can itself produce log records; this stops the recursion.
    static IN_LOGGER: Cell<bool> = Cell::new(false);
}

/// Installs the logger as the global `log` backend. Call once, before anything logs.
pub fn init() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}

/// Gives the logger a handle for streaming entries to the main window.
pub fn attach_app(app: AppHandle) {
    let _ = APP.set(app);
}

/// Turns collection on or off. Turning it off also drops what was collected.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if !enabled {
        clear();
    }
}

pub fn entries() -> Vec<LogEntry> {
    lock_buffer().iter().cloned().collect()
}

pub fn clear() {
    lock_buffer().clear();
}

/// Adds an entry from outside the `log` facade (used for frontend logs).
pub fn record(level: log::Level, source: &str, message: String) {
    let ts = now_ms();
    mirror_to_console(level, source, &message);

    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    if IN_LOGGER.with(|flag| flag.replace(true)) {
        return;
    }

    let entry = LogEntry {
        id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
        ts,
        level: level_name(level).to_string(),
        source: source.to_string(),
        message,
    };

    {
        let mut buffer = lock_buffer();
        if buffer.len() >= MAX_ENTRIES {
            buffer.pop_front();
        }
        buffer.push_back(entry.clone());
    }

    if let Some(app) = APP.get() {
        let _ = app.emit_to("main", "log-entry", entry);
    }

    IN_LOGGER.with(|flag| flag.set(false));
}

pub fn parse_level(level: &str) -> log::Level {
    match level {
        "error" => log::Level::Error,
        "warn" | "warning" => log::Level::Warn,
        "debug" | "trace" => log::Level::Debug,
        _ => log::Level::Info,
    }
}

fn level_name(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "error",
        log::Level::Warn => "warn",
        log::Level::Info => "info",
        log::Level::Debug | log::Level::Trace => "debug",
    }
}

fn lock_buffer() -> MutexGuard<'static, VecDeque<LogEntry>> {
    BUFFER.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn mirror_to_console(level: log::Level, source: &str, message: &str) {
    match level {
        log::Level::Error | log::Level::Warn => {
            eprintln!("[Typr][{}][{}] {}", level_name(level), source, message)
        }
        _ => println!("[Typr][{}][{}] {}", level_name(level), source, message),
    }
}

/// `typr_lib::recorder` -> `recorder`, `typr` (main.rs) -> `app`,
/// dependencies -> their crate name.
fn source_label(target: &str) -> String {
    if let Some(module) = target.strip_prefix("typr_lib::") {
        return module.to_string();
    }
    if target == "typr" || target == "typr_lib" {
        return "app".to_string();
    }
    target.split("::").next().unwrap_or(target).to_string()
}

fn is_own_target(target: &str) -> bool {
    target == "typr" || target.starts_with("typr::") || target.starts_with("typr_lib")
}

struct AppLogger;

impl log::Log for AppLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Everything from Typr itself, only Info and above from dependencies
        metadata.level() <= log::Level::Info || is_own_target(metadata.target())
    }

    fn log(&self, entry: &log::Record) {
        if !self.enabled(entry.metadata()) {
            return;
        }
        record(
            entry.level(),
            &source_label(entry.target()),
            entry.args().to_string(),
        );
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_label() {
        assert_eq!(source_label("typr_lib::recorder"), "recorder");
        assert_eq!(source_label("typr"), "app");
        assert_eq!(source_label("reqwest::connect"), "reqwest");
    }

    #[test]
    fn test_parse_level() {
        assert_eq!(parse_level("error"), log::Level::Error);
        assert_eq!(parse_level("warn"), log::Level::Warn);
        assert_eq!(parse_level("debug"), log::Level::Debug);
        assert_eq!(parse_level("anything"), log::Level::Info);
    }
}
