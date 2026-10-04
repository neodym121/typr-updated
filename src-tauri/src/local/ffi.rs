//! Run-time binding to transcribe.cpp's C API (`include/transcribe.h` at
//! the v0.2.4 tag; later versions add fields, e.g. to the run params).
//! transcribe.dll comes from the downloaded runtime, so it is loaded with
//! LoadLibrary instead of being linked: nothing native is built with Typr.
//!
//! The structs below mirror the header. Before any of them crosses the ABI,
//! their sizes are compared with what the library reports
//! (`transcribe_abi_struct_size`), so a mismatched DLL is refused instead of
//! corrupting memory.
//!
//! A loaded library is never unloaded: ggml keeps its backend modules
//! loaded for the life of the process anyway.

use libloading::Library;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;
use std::ptr::{null, null_mut};

use super::catalog::{Backend, TRANSCRIBE_VERSION};

// transcribe_status
const OK: c_int = 0;
const ERR_BACKEND: c_int = 8;
const ERR_UNSUPPORTED_LANGUAGE: c_int = 10;
const ERR_OUTPUT_TRUNCATED: c_int = 18;

// transcribe_abi_struct
const ABI_MODEL_LOAD_PARAMS: c_int = 0;
const ABI_SESSION_PARAMS: c_int = 1;
const ABI_RUN_PARAMS: c_int = 2;
const ABI_DEVICE_INFO: c_int = 13;

// transcribe_backend_request
const BACKEND_CPU: c_int = 1;
const BACKEND_VULKAN: c_int = 3;

// transcribe_device_type
pub const DEVICE_TYPE_CPU: c_int = 0;
pub const DEVICE_TYPE_GPU: c_int = 1;
pub const DEVICE_TYPE_IGPU: c_int = 2;

// transcribe_timestamp_kind
const TIMESTAMPS_NONE: c_int = 0;

// transcribe_log_level
const LOG_INFO: c_int = 1;
const LOG_WARN: c_int = 2;
const LOG_ERROR: c_int = 3;

#[repr(C)]
struct DeviceInfo {
    struct_size: u64,
    name: *const c_char,
    description: *const c_char,
    kind: *const c_char,
    device_id: *const c_char,
    memory_total: u64,
    memory_free: u64,
    device_type: c_int,
}

#[repr(C)]
struct ModelLoadParams {
    struct_size: u64,
    backend: c_int,
    device: *mut c_void,
}

#[repr(C)]
struct SessionParams {
    struct_size: u64,
    n_threads: c_int,
    kv_type: c_int,
    n_ctx: i32,
}

#[repr(C)]
struct RunParams {
    struct_size: u64,
    task: c_int,
    timestamps: c_int,
    pnc: c_int,
    itn: c_int,
    diarize: c_int,
    language: *const c_char,
    target_language: *const c_char,
    keep_special_tags: bool,
    family: *const c_void,
    spec_k_drafts: i32,
}

type LogCallback = unsafe extern "C" fn(level: c_int, message: *const c_char, user: *mut c_void);

/// Function table of a loaded transcribe.dll
pub struct Api {
    status_string: unsafe extern "C" fn(c_int) -> *const c_char,
    version: unsafe extern "C" fn() -> *const c_char,
    abi_struct_size: unsafe extern "C" fn(c_int) -> usize,
    log_set: unsafe extern "C" fn(Option<LogCallback>, *mut c_void),
    init_backends: unsafe extern "C" fn(*const c_char) -> c_int,
    device_count: unsafe extern "C" fn() -> c_int,
    device_get: unsafe extern "C" fn(c_int) -> *mut c_void,
    device_info_init: unsafe extern "C" fn(*mut DeviceInfo),
    device_get_info: unsafe extern "C" fn(*mut c_void, *mut DeviceInfo) -> c_int,
    model_load_params_init: unsafe extern "C" fn(*mut ModelLoadParams),
    session_params_init: unsafe extern "C" fn(*mut SessionParams),
    run_params_init: unsafe extern "C" fn(*mut RunParams),
    model_load_file: unsafe extern "C" fn(*const c_char, *const ModelLoadParams, *mut *mut c_void) -> c_int,
    model_free: unsafe extern "C" fn(*mut c_void),
    model_backend: unsafe extern "C" fn(*const c_void) -> *const c_char,
    model_device: unsafe extern "C" fn(*const c_void) -> *mut c_void,
    session_init: unsafe extern "C" fn(*mut c_void, *const SessionParams, *mut *mut c_void) -> c_int,
    session_free: unsafe extern "C" fn(*mut c_void),
    run: unsafe extern "C" fn(*mut c_void, *const f32, c_int, *const RunParams) -> c_int,
    full_text: unsafe extern "C" fn(*const c_void) -> *const c_char,
    detected_language: unsafe extern "C" fn(*const c_void) -> *const c_char,
}

/// A compute device registered by the loaded backend modules.
#[derive(Debug, Clone)]
pub struct Device {
    handle: DeviceHandle,
    pub name: String,
    pub description: String,
    /// "cpu", "vulkan", "accel", ...
    pub kind: String,
    pub device_type: c_int,
    pub memory_total: u64,
}

#[derive(Debug, Clone, Copy)]
struct DeviceHandle(*mut c_void);
// Device handles are process-lifetime registry entries owned by the library
unsafe impl Send for DeviceHandle {}
unsafe impl Sync for DeviceHandle {}

/// A loaded model and its single session. Freed with [`Api::free`].
pub struct Loaded {
    model: *mut c_void,
    session: *mut c_void,
}
// A session is used by one thread at a time (the engine's mutex sees to it)
unsafe impl Send for Loaded {}

/// What a run produced.
pub struct Transcript {
    pub text: String,
    pub language: String,
    /// Set when the decode stopped early and the text may be incomplete
    pub warning: Option<String>,
}

/// A failed call: its status code and a readable message.
#[derive(Debug, Clone)]
pub struct CallError {
    pub status: c_int,
    pub message: String,
}

impl CallError {
    /// A GPU/driver failure; the library says reloading on the CPU recovers
    pub fn is_backend_failure(&self) -> bool {
        self.status == ERR_BACKEND
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

fn text(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}

fn c_path(path: &Path) -> Result<CString, String> {
    let utf8 = path
        .to_str()
        .ok_or_else(|| format!("Path is not valid Unicode: {}", path.display()))?;
    CString::new(utf8).map_err(|_| format!("Path contains a NUL character: {}", path.display()))
}

unsafe extern "C" fn log_callback(level: c_int, message: *const c_char, _user: *mut c_void) {
    // Never let a panic cross the C boundary
    let _ = std::panic::catch_unwind(|| {
        let message = text(message);
        let message = message.trim_end();
        if message.is_empty() {
            return;
        }
        // Shown as "transcribe.cpp" in the Developer section
        const TARGET: &str = "typr_lib::transcribe.cpp";
        match level {
            LOG_ERROR => log::error!(target: TARGET, "{}", message),
            LOG_WARN => log::warn!(target: TARGET, "{}", message),
            LOG_INFO => log::info!(target: TARGET, "{}", message),
            _ => log::debug!(target: TARGET, "{}", message),
        }
    });
}

/// Opens transcribe.dll so that its own imports (ggml.dll, ggml-base.dll)
/// resolve from the runtime folder, not from the executable's folder.
#[cfg(windows)]
fn open_library(path: &Path) -> Result<Library, libloading::Error> {
    use libloading::os::windows::{Library as WindowsLibrary, LOAD_WITH_ALTERED_SEARCH_PATH};
    unsafe { WindowsLibrary::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH) }.map(Into::into)
}

#[cfg(not(windows))]
fn open_library(path: &Path) -> Result<Library, libloading::Error> {
    unsafe { Library::new(path) }
}

/// Lets the backend modules ggml loads later (and their own dependencies) be
/// found in the runtime folder, ahead of System32 and PATH.
#[cfg(windows)]
fn add_dll_directory(dir: &Path) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let ok = unsafe { windows_sys::Win32::System::LibraryLoader::SetDllDirectoryW(wide.as_ptr()) };
    if ok == 0 {
        log::warn!("SetDllDirectory failed for {}", dir.display());
    }
}

#[cfg(not(windows))]
fn add_dll_directory(_dir: &Path) {}

macro_rules! symbol {
    ($lib:expr, $name:literal) => {
        *unsafe { $lib.get(concat!($name, "\0").as_bytes()) }
            .map_err(|e| format!("transcribe.dll has no {}: {}", $name, e))?
    };
}

impl Api {
    /// Loads transcribe.dll from the runtime folder, checks that it is the
    /// version Typr was written for, routes its log into Typr's and
    /// registers the runtime's backend modules.
    pub fn open(dir: &Path) -> Result<&'static Api, String> {
        add_dll_directory(dir);
        let library = open_library(&dir.join("transcribe.dll")).map_err(|e| {
            format!(
                "Could not load the transcribe.cpp runtime: {}. If this keeps happening, install the Microsoft Visual C++ Redistributable (x64)",
                e
            )
        })?;
        // Never unloaded (see the module docs)
        let library: &'static Library = Box::leak(Box::new(library));

        let api = Api {
            status_string: symbol!(library, "transcribe_status_string"),
            version: symbol!(library, "transcribe_version"),
            abi_struct_size: symbol!(library, "transcribe_abi_struct_size"),
            log_set: symbol!(library, "transcribe_log_set"),
            init_backends: symbol!(library, "transcribe_init_backends"),
            device_count: symbol!(library, "transcribe_device_count"),
            device_get: symbol!(library, "transcribe_device_get"),
            device_info_init: symbol!(library, "transcribe_device_info_init"),
            device_get_info: symbol!(library, "transcribe_device_get_info"),
            model_load_params_init: symbol!(library, "transcribe_model_load_params_init"),
            session_params_init: symbol!(library, "transcribe_session_params_init"),
            run_params_init: symbol!(library, "transcribe_run_params_init"),
            model_load_file: symbol!(library, "transcribe_model_load_file"),
            model_free: symbol!(library, "transcribe_model_free"),
            model_backend: symbol!(library, "transcribe_model_backend"),
            model_device: symbol!(library, "transcribe_model_device"),
            session_init: symbol!(library, "transcribe_session_init"),
            session_free: symbol!(library, "transcribe_session_free"),
            run: symbol!(library, "transcribe_run"),
            full_text: symbol!(library, "transcribe_full_text"),
            detected_language: symbol!(library, "transcribe_detected_language"),
        };

        let version = text(unsafe { (api.version)() });
        if version != TRANSCRIBE_VERSION {
            return Err(format!(
                "The runtime is transcribe.cpp {}, Typr needs {}",
                version, TRANSCRIBE_VERSION
            ));
        }
        api.check_struct("model load params", ABI_MODEL_LOAD_PARAMS, std::mem::size_of::<ModelLoadParams>())?;
        api.check_struct("session params", ABI_SESSION_PARAMS, std::mem::size_of::<SessionParams>())?;
        api.check_struct("run params", ABI_RUN_PARAMS, std::mem::size_of::<RunParams>())?;
        api.check_struct("device info", ABI_DEVICE_INFO, std::mem::size_of::<DeviceInfo>())?;

        // Once, before any model exists (the header's threading contract)
        unsafe { (api.log_set)(Some(log_callback), null_mut()) };

        let modules = c_path(dir)?;
        let status = unsafe { (api.init_backends)(modules.as_ptr()) };
        if status != OK {
            return Err(format!(
                "The runtime found no device to run on: {}",
                api.describe(status)
            ));
        }

        Ok(Box::leak(Box::new(api)))
    }

    fn check_struct(&self, name: &str, which: c_int, ours: usize) -> Result<(), String> {
        let theirs = unsafe { (self.abi_struct_size)(which) };
        if theirs != ours {
            return Err(format!(
                "The runtime's {} are {} bytes, Typr expects {}; the runtime doesn't match this version of Typr",
                name, theirs, ours
            ));
        }
        Ok(())
    }

    fn describe(&self, status: c_int) -> String {
        format!("{} (status {})", text(unsafe { (self.status_string)(status) }), status)
    }

    fn error(&self, what: &str, status: c_int) -> CallError {
        CallError {
            status,
            message: format!("{}: {}", what, self.describe(status)),
        }
    }

    fn device_info(&self, handle: *mut c_void) -> Option<Device> {
        if handle.is_null() {
            return None;
        }
        let mut info: DeviceInfo = unsafe { std::mem::zeroed() };
        unsafe { (self.device_info_init)(&mut info) };
        if unsafe { (self.device_get_info)(handle, &mut info) } != OK {
            return None;
        }
        Some(Device {
            handle: DeviceHandle(handle),
            name: text(info.name),
            description: text(info.description),
            kind: text(info.kind),
            device_type: info.device_type,
            memory_total: info.memory_total,
        })
    }

    /// Every device the backend modules registered
    pub fn devices(&self) -> Vec<Device> {
        let count = unsafe { (self.device_count)() }.max(0);
        (0..count)
            .filter_map(|i| self.device_info(unsafe { (self.device_get)(i) }))
            .collect()
    }

    /// Loads a GGUF model on `device` (or the CPU) and opens a session.
    pub fn load_model(&self, path: &Path, backend: Backend, device: Option<&Device>) -> Result<Loaded, CallError> {
        let path = c_path(path).map_err(|message| CallError { status: -1, message })?;
        let mut params: ModelLoadParams = unsafe { std::mem::zeroed() };
        unsafe { (self.model_load_params_init)(&mut params) };
        params.backend = match backend {
            Backend::Cpu => BACKEND_CPU,
            Backend::Vulkan => BACKEND_VULKAN,
        };
        params.device = device.map_or(null_mut(), |d| d.handle.0);

        let mut model: *mut c_void = null_mut();
        let status = unsafe { (self.model_load_file)(path.as_ptr(), &params, &mut model) };
        if status != OK || model.is_null() {
            return Err(self.error("Could not load the model", status));
        }

        let mut session_params: SessionParams = unsafe { std::mem::zeroed() };
        unsafe { (self.session_params_init)(&mut session_params) };
        let mut session: *mut c_void = null_mut();
        let status = unsafe { (self.session_init)(model, &session_params, &mut session) };
        if status != OK || session.is_null() {
            unsafe { (self.model_free)(model) };
            return Err(self.error("Could not open a session", status));
        }
        Ok(Loaded { model, session })
    }

    /// Backend the model actually runs on, e.g. "vulkan" or "cpu"
    pub fn backend_of(&self, loaded: &Loaded) -> String {
        text(unsafe { (self.model_backend)(loaded.model) })
    }

    /// The device holding the model's weights
    pub fn device_of(&self, loaded: &Loaded) -> Option<Device> {
        self.device_info(unsafe { (self.model_device)(loaded.model) })
    }

    /// Transcribes 16 kHz mono samples in [-1, 1]. `language` is a hint such
    /// as "ru"; `None`, or a language the model doesn't list, detects it.
    pub fn transcribe(&self, loaded: &mut Loaded, samples: &[f32], language: Option<&str>) -> Result<Transcript, CallError> {
        if samples.is_empty() || samples.len() > c_int::MAX as usize {
            return Err(CallError {
                status: -1,
                message: format!("Can't transcribe {} samples", samples.len()),
            });
        }
        let hint = language.and_then(|code| CString::new(code).ok());
        let mut params: RunParams = unsafe { std::mem::zeroed() };
        unsafe { (self.run_params_init)(&mut params) };
        params.timestamps = TIMESTAMPS_NONE;
        params.language = hint.as_ref().map_or(null(), |code| code.as_ptr());

        let run = |params: &RunParams| unsafe {
            (self.run)(loaded.session, samples.as_ptr(), samples.len() as c_int, params)
        };
        let mut status = run(&params);
        if status == ERR_UNSUPPORTED_LANGUAGE && hint.is_some() {
            log::warn!(
                "The model doesn't take the language '{}', detecting the language instead",
                language.unwrap_or_default()
            );
            params.language = null();
            status = run(&params);
        }
        // These keep the (shortened) transcript readable
        let warning = match status {
            OK => None,
            ERR_OUTPUT_TRUNCATED => Some(self.describe(status)),
            _ => return Err(self.error("Transcription failed", status)),
        };
        Ok(Transcript {
            text: text(unsafe { (self.full_text)(loaded.session) }),
            language: text(unsafe { (self.detected_language)(loaded.session) }),
            warning,
        })
    }

    /// Frees the session and the model (and their GPU memory).
    pub fn free(&self, loaded: Loaded) {
        unsafe {
            (self.session_free)(loaded.session);
            (self.model_free)(loaded.model);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sizes from transcribe.h on 64-bit targets (load params grew to 24
    /// bytes in 0.2, per the migration guide)
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn test_struct_layout_matches_header() {
        assert_eq!(std::mem::size_of::<ModelLoadParams>(), 24);
        assert_eq!(std::mem::size_of::<SessionParams>(), 24);
        assert_eq!(std::mem::size_of::<RunParams>(), 72);
        assert_eq!(std::mem::size_of::<DeviceInfo>(), 64);
    }

    #[test]
    fn test_loading_a_missing_runtime_fails_cleanly() {
        let dir = std::env::temp_dir().join("typr_test_no_runtime");
        let _ = std::fs::create_dir_all(&dir);
        let error = Api::open(&dir).err().expect("there is no transcribe.dll here");
        assert!(error.contains("Could not load the transcribe.cpp runtime"));
    }
}
