pub fn paste_text(text: &str) -> Result<(), String> {
    // Set clipboard (arboard is thread-safe)
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard is unavailable: {}", e))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to copy text to the clipboard: {}", e))?;
    log::debug!("Copied {} characters to the clipboard", text.chars().count());

    // Small delay to ensure clipboard is set
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Simulate Cmd+V via osascript (works from any thread, unlike enigo which
    // calls TSMGetInputSourceProperty requiring the main thread)
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .args(["-e", r#"tell application "System Events" to keystroke "v" using command down"#])
            .output()
            .map_err(|e| format!("Failed to simulate paste: {}", e))?;
        log::debug!("Sent Cmd+V via osascript");
    }

    #[cfg(target_os = "windows")]
    {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};

        // Virtual-key code of the V key. Key::Unicode('v') is looked up in the
        // current keyboard layout and fails when that layout has no Latin "v"
        // (e.g. Russian), which used to break the paste — and the whole dictation.
        const VK_V: u32 = 0x56;

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialise keyboard simulation: {}", e))?;
        enigo
            .key(Key::Control, Direction::Press)
            .map_err(|e| format!("Failed to press Ctrl: {}", e))?;
        let v_result = enigo.key(Key::Other(VK_V), Direction::Click);
        // Release Ctrl even if V failed, so it never stays logically held down
        let ctrl_result = enigo.key(Key::Control, Direction::Release);
        v_result.map_err(|e| format!("Failed to send Ctrl+V: {}", e))?;
        ctrl_result.map_err(|e| format!("Failed to release Ctrl: {}", e))?;
        log::debug!("Sent Ctrl+V to the active window");
    }

    Ok(())
}
