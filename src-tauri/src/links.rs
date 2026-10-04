//! Opens web pages in the default browser: the release page and the pages
//! where providers hand out API keys. Only these sites can be opened, so the
//! interface can't be used to launch anything else.

const ALLOWED: &[&str] = &[
    crate::updates::RELEASES_PAGE,
    "https://console.groq.com/",
    "https://polza.ai/",
    "https://www.assemblyai.com/",
    "https://platform.openai.com/",
    "https://aistudio.google.com/",
    "https://openrouter.ai/",
];

pub fn is_allowed(url: &str) -> bool {
    !url.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"')
        && ALLOWED.iter().any(|prefix| url.starts_with(prefix))
}

pub fn open(url: &str) -> Result<(), String> {
    if !is_allowed(url) {
        return Err(format!("Typr doesn't open this address: {}", url));
    }
    log::info!("Opening {}", url);
    open_in_browser(url)
}

#[cfg(target_os = "windows")]
fn open_in_browser(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (operation, file) = (wide("open"), wide(url));
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success
    if result as isize > 32 {
        Ok(())
    } else {
        Err(format!("Couldn't open the browser (error {})", result as isize))
    }
}

#[cfg(not(target_os = "windows"))]
fn open_in_browser(url: &str) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't open the browser: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_known_sites_open() {
        assert!(is_allowed("https://console.groq.com/keys"));
        assert!(is_allowed("https://github.com/neodym121/typr-updated/releases/tag/v2.0.0"));
        assert!(!is_allowed("https://github.com/someone/else"));
        assert!(!is_allowed("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_allowed("https://console.groq.com/keys\" & calc"));
        assert!(!is_allowed("https://console.groq.com.evil.example/"));
    }
}
