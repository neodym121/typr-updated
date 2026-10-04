//! Start with Windows: a value in the current user's Run key
//! (HKCU\Software\Microsoft\Windows\CurrentVersion\Run) that starts Typr
//! with `--autostart`, so it goes straight to the tray without opening its
//! window. The registry is the only place the choice lives.

/// Argument Windows starts Typr with at sign-in
pub const ARG: &str = "--autostart";

/// The command the Run value holds for this executable.
pub fn command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Can't find Typr's executable: {}", e))?;
    Ok(format!("\"{}\" {}", exe.display(), ARG))
}

#[cfg(target_os = "windows")]
mod imp {
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "Typr";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The command in the Run value, if there is one.
    pub fn registered() -> Option<String> {
        let (key, value) = (wide(RUN_KEY), wide(VALUE));
        let mut buffer = [0u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                buffer.as_mut_ptr() as *mut c_void,
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let chars = (size as usize / 2).min(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..chars]).trim_end_matches('\0').to_string())
    }

    pub fn register(command: &str) -> Result<(), String> {
        let (key, value, data) = (wide(RUN_KEY), wide(VALUE), wide(command));
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_SZ,
                data.as_ptr() as *const c_void,
                (data.len() * 2) as u32,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("Couldn't add Typr to startup (error {})", status))
        }
    }

    pub fn unregister() -> Result<(), String> {
        let (key, value) = (wide(RUN_KEY), wide(VALUE));
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
        if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("Couldn't remove Typr from startup (error {})", status))
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn registered() -> Option<String> {
        None
    }

    pub fn register(_command: &str) -> Result<(), String> {
        Err("Starting with the system is supported on Windows only".to_string())
    }

    pub fn unregister() -> Result<(), String> {
        Ok(())
    }
}

/// Whether Typr starts with Windows.
pub fn is_enabled() -> bool {
    imp::registered().is_some()
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        imp::register(&command()?)?;
    } else {
        imp::unregister()?;
    }
    log::info!("Start with Windows {}", if enabled { "on" } else { "off" });
    Ok(())
}

/// At startup: an entry left by an install in another folder points at an
/// executable that may be gone; point it at this one. Development builds
/// leave it alone, so they never take over the installed Typr's entry.
pub fn refresh() {
    if cfg!(debug_assertions) {
        return;
    }
    let (Some(registered), Ok(current)) = (imp::registered(), command()) else {
        return;
    };
    if registered != current {
        match imp::register(&current) {
            Ok(()) => log::info!("Start with Windows now points at {}", current),
            Err(e) => log::warn!("{}", e),
        }
    }
}
