# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## Notes for Claude Code

AGENTS.md (imported above) is the main guide, shared with other coding agents. This part adds what it doesn't cover.

### Commands

- One module's Rust tests: `cargo test --lib cleanup::` in `src-tauri`; a test name works too (`cargo test --lib test_save_and_load`).
- Frontend check: `npx tsc` (strict, unused locals and parameters are errors). There are no frontend tests or linters. `npm run build` (tsc + Vite) is Tauri's `beforeBuildCommand`, so a type error fails the installer build in CI.
- `npm run tauri dev`:
  - Quit the installed Typr from the tray first. Both builds take the same single-instance lock (`com.typr.app`), so the dev build would only focus the installed app's window and exit.
  - Both builds read `%APPDATA%\com.typr.app\config.json`. To keep the dev config apart, set `TYPR_CONFIG_DIR` (debug builds only), e.g. `$env:TYPR_CONFIG_DIR = "$env:TEMP\typr-dev"` in PowerShell.
  - Every log line also prints to the terminal as `[Typr][level][source] message`, with Developer mode on or off.
- The page runs only inside the Tauri window: in a plain browser (`npm run dev`, port 1420) `getCurrentWindow()` throws at load and the page stays hidden, so UI changes can't be previewed there.

### How the pieces connect

- `main.rs` is the hub: `AppState`, every `#[tauri::command]` (listed in `generate_handler!`), the tray, hotkey registration and `apply_settings`. Every settings change, from the window or the tray, goes through `apply_settings`: it re-registers the hotkey, saves `config.json`, logs the masked diff, tells `LocalEngine`, and updates the overlay and the tray. A setting that must take effect at once is handled there.
- The window keeps one settings object, `currentSettings` in `main.ts`, typed by a TS `Settings` interface that mirrors the Rust struct's camelCase JSON. `local.ts`, `postprocess.ts` and `setup.ts` reach it through a host object (`settings()`, `save()`) and declare only the fields they use. Every control change sends the whole object to `save_settings`; there is no Save button.
- A new setting touches: in `settings.rs` the field (`#[serde(rename = "camelCase")]`), `Default`, `summary()`, `describe_changes()` (API keys go in its masked `secrets` list) and the tests; `apply_settings` if it must take effect at once; in the UI the TS interface, the control in `index.html`, `bind(...)` and `fillForm()` in `main.ts`, and both dictionaries in `i18n.ts`.

### Releases and docs

- The version bump goes into the commit with the change, whose title ends with `(vX.Y.Z)`. It changes `package.json`, `package-lock.json` (two places), `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (the `typr` package), `src-tauri/tauri.conf.json`, the `#version-text` placeholder in `index.html` and the tag example in both READMEs. The update check compares the Cargo version with the latest release tag.
- A feature change also updates AGENTS.md, the feature lists in `README.md` and `README.ru.md` (the same text in Russian) and the strings in both languages.
- `docs/polza-ai-reference.md`: Polza.AI API notes (in Russian), for `transcribe_polza.rs` and Polza post-processing.
- `docs/superpowers/`: the original April 2026 design and plan, now outdated; trust the code and AGENTS.md.
