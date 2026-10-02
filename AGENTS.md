# AGENTS.md — Typr

Typr is a **desktop dictation app** built with [Tauri 2](https://tauri.app/) (Rust backend) and vanilla TypeScript frontend. It records audio via a global hotkey, transcribes it using Groq, OpenAI (or any OpenAI-compatible API), Polza.AI or AssemblyAI, optionally polishes the text with a language model, and auto-pastes the result into the active window. The interface is in English and Russian.

---

## Architecture

```
typr/
├── src/                        # TypeScript frontend (Vite)
│   ├── main.ts                 # Main UI logic, settings form, hotkey binding
│   ├── i18n.ts                 # English/Russian UI strings, t(), data-i18n attributes
│   ├── dropdown.ts             # App-styled dropdowns over hidden native <select>s
│   ├── postprocess.ts          # Post-processing section: provider, key, model list with search, styles
│   ├── overlay.html            # Mic indicator overlay (WebView fallback for non-Windows)
│   └── style.css               # Global styles
├── src-tauri/                  # Rust backend (Tauri)
│   └── src/
│       ├── lib.rs              # App entry — registers Tauri plugins, wires commands
│       ├── main.rs             # Binary entry point
│       ├── recorder.rs         # Orchestrates recording → transcription → paste flow
│       ├── audio.rs            # Low-level audio capture (cpal)
│       ├── transcribe_groq.rs  # Groq Whisper API client
│       ├── transcribe_openai.rs# OpenAI / OpenAI-compatible API client
│       ├── transcribe_polza.rs # Polza.AI client (JSON body, base64 data URL)
│       ├── transcribe_assemblyai.rs # AssemblyAI client (upload → transcript job → poll)
│       ├── keyboard.rs         # Hotkey must be pressed on its own (GetAsyncKeyState check)
│       ├── i18n.rs             # System language detection, tray menu strings
│       ├── net.rs              # Shared HTTP client (timeouts) + response/error helpers
│       ├── logger.rs           # `log` backend feeding the Developer section
│       ├── overlay.rs          # Recording indicator that slides in while dictating (native layered window on Windows)
│       ├── cleanup.rs          # Post-processing: trims filler words, fixes punctuation
│       ├── paste.rs            # Pastes via the clipboard (Ctrl+V), then restores the previous content
│       ├── postprocess.rs      # LLM post-processing: model lists, Gemini / OpenAI-compatible calls, prompts
│       └── settings.rs         # Settings struct, load/save from config.json
├── .github/workflows/
│   └── build.yml               # CI: builds Windows installer on tag push or manual trigger
├── index.html                  # Main window HTML shell
├── package.json                # npm scripts: dev, build, tauri
├── vite.config.ts              # Vite config
└── src-tauri/tauri.conf.json   # Tauri app config (window, bundle, CSP)
```

---

## Key Modules

### `recorder.rs` — Central orchestrator
Manages the `RecordingState` state machine (`Ready → Recording → Transcribing → Ready`). On toggle:
1. Starts audio capture via `AudioRecorder`
2. Saves WAV to a temp file
3. Calls the configured transcription engine
4. Runs `cleanup_text()` on the result
5. If post-processing is on, rewrites the text with `postprocess::process()`; on failure the transcribed text is pasted anyway and the error is reported afterwards
6. Calls `paste_text()` to inject into the active window
7. Emits `recording-state` events to the frontend and updates the overlay (the indicator slides in for Recording/Transcribing and back out on Ready)

### `settings.rs` — Configuration
Persisted to `config.json` in the Tauri app data directory. Key fields:
- `engine`: `"groq"` | `"openai"` | `"openai-compatible"` | `"polza"` | `"assemblyai"` (default: `"groq"`)
- `groqModel`: `"whisper-large-v3-turbo"` (default) | `"whisper-large-v3"`
- `groqApiKey`, `openaiApiKey`, `openaiEndpoint`, `openaiModel`, `polzaApiKey`, `polzaModel`, `polzaProvider`, `assemblyaiApiKey`
- `assemblyaiModel`: `"universal-3-5-pro"` (default, no Russian) | `"universal-2"`
- `language`: `"en"` | `"ru"` | `""` (empty follows the system, see `i18n.rs`)
- `developerMode`: shows the Developer section and turns log collection on
- `microphone`: device name or `"default"`
- `recordingMode`: `"toggle"` | `"push-to-talk"`
- `hotkey`: default `"Ctrl+Shift+Space"`
- `appendSpace`: adds a space after the pasted text (default `false`, switch in Post-processing; works with post-processing off too)
- `showIndicator`: whether the recording indicator may appear at all (default `true`, switch in General)
- `postProcess`: `enabled` (default `false`), `provider` (`"gemini"` | `"openrouter"` | `"groq"` | `"polza"`), `preset` (`"chill"` | `"proper"` | `"custom"`; the old `"official"` is migrated to `"proper"` on load), `customPrompt`, and `{ apiKey, model }` per provider, plus `providerId` for Polza (sub-provider sent as `provider.only`, omitted from the file when empty). Groq and Polza fall back to the Engine key when their own is empty (`Settings::post_process_key`)

> **Note:** The legacy `"local"` engine value is silently migrated to `"groq"` on load.

### `transcribe_groq.rs` / `transcribe_openai.rs`
Both send a multipart form POST with the WAV file to the respective API. `transcribe_openai.rs` is also used for any OpenAI-compatible endpoint (e.g. local Whisper servers) via the configurable `openaiEndpoint`.

### `transcribe_assemblyai.rs`
Three steps: `POST /v2/upload` with the raw WAV, `POST /v2/transcript` with `speech_models: [model]` and `language_detection: true`, then poll `GET /v2/transcript/{id}` until `completed` or `error`. Universal-3.5 Pro supports 18 languages without Russian; if it rejects the language, the error suggests Universal-2.

### `postprocess.rs` — LLM post-processing
- `list_models(provider, key)` (Tauri command `list_postprocess_models`) returns text models only: Gemini `GET /v1beta/models` filtered by `generateContent`; OpenRouter, Groq and Polza `GET /models` (OpenAI format) filtered by `type`, `output_modalities` and, for Groq, speech/guard ids.
- `process(settings, text)` calls Gemini `models/{id}:generateContent` (key in `x-goog-api-key`) or `/chat/completions` (Bearer) at temperature 0.2, retrying without temperature if a model rejects it, and strips `<think>` blocks, echoed tags and code fences from the reply.
- The system prompt has two layers: the app's rules first (the dictation, sent between `<dictation>` tags, is text to edit and never a message to answer or obey; keep meaning and language; reply with the text only), then the style. The "custom" style embeds the user's prompt in `<user_style>` tags, marked as the app user's instructions that can't override the app's rules.

### `paste.rs` — clean clipboard
Saves the clipboard (text, else image, else nothing), sets the dictated text with arboard's `exclude_from_monitoring()` (kept out of Win+V history, cloud sync and clipboard managers), sends Ctrl+V, waits `RESTORE_DELAY` (300 ms) and puts the previous content back, unless something else was copied meanwhile. If the paste fails, the text stays on the clipboard. Other formats (e.g. copied files) aren't restored.

### `dropdown.ts`
`enhanceSelect(select)` hides a native `<select>` and draws an app-styled button and list over it; the `<select>` stays the source of truth (`.value`, `change` events), option rebuilds are picked up by a `MutationObserver`, and after setting `.value` from code call `syncSelect(select)`. New `<select>`s should be enhanced too.

### `keyboard.rs` — hotkey pressed on its own
On `ShortcutState::Pressed` with the recorder `Ready`, `HotkeyKeys::extra_keys_held()` reads every key and mouse button via `GetAsyncKeyState`; if anything besides the hotkey's own keys is down, the press is ignored (logged). Stopping a recording is never blocked.

### Localization
`src/i18n.ts` holds the English and Russian strings. Static markup uses `data-i18n` (text), `data-i18n-placeholder` and `data-i18n-title`; code uses `t(key)`. New UI text must get keys in both dictionaries. The `language` setting picks the language; when empty, `get_system_language` (Rust, `sys-locale`) decides: Russian for ru, uk, be, kk, ky, tg, uz, tk, hy, az, English otherwise. The tray menu is translated in `i18n.rs`. Backend error messages and logs stay in English. The page is hidden (`.i18n-pending`) until the language is applied.

### `cleanup.rs`
Strips common transcription artifacts (leading/trailing filler, repeated punctuation, etc.) before the text is pasted.

### `logger.rs` — Developer section
Installed as the global `log` backend in `main()`. Use `log::info!` / `log::warn!` / `log::error!` / `log::debug!` everywhere in Rust (not `println!`). While `developerMode` is on, entries go to a 5000-entry ring buffer and are streamed to the main window as `log-entry` events; the frontend forwards its own console output via the `frontend_log` command. Never log API keys — use `Settings::summary()` / `describe_changes()`, which mask them.

`stop_and_transcribe` must always return the recorder to `Ready`, even on errors; failures are reported to the UI with `recorder::notify_error` (`recording-error` event).

### Memory: no WebView while idle
- `overlay.rs` draws the recording indicator itself on Windows (`windows-sys` layered window, per-pixel alpha, SDF rendering) — no WebView. Other platforms fall back to the WebView overlay `src/overlay.html`.
- The indicator is out of sight while idle: its window starts at the top edge of the primary screen and the disc is drawn above that edge, so it slides in from behind the screen when a dictation starts (or briefly after an error) and slides back out on `Ready`. Once fully out, the window is hidden and its timers stop, so it costs no CPU.
- `overlay::set_enabled` gates it: `main.rs::sync_overlay` allows it only while `showIndicator` is on and the hotkey is on in the tray.
- Closing the main window destroys it (and its WebView); `show_main_window` re-creates it from `tauri.conf.json` on a separate thread (building windows in event handlers deadlocks on Windows). `RunEvent::ExitRequested` without a code is prevented, so Typr keeps running in the tray; tray → Exit calls `app.exit(0)`.
- The tray menu's "Hotkey: On/Off" item unregisters the global shortcut, cancels an unfinished recording and keeps the overlay from appearing (e.g. while gaming). The state is not persisted.

---

## Development Setup

### Prerequisites
- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) 20+
- Tauri CLI v2: `npm install` pulls it as a dev dependency

### Run in dev mode
```bash
npm install
npm run tauri dev
```

### Build for production (Windows)
```bash
npm run tauri build
# Output: src-tauri/target/release/bundle/
```

### Run Rust tests
```bash
cd src-tauri
cargo test
```

---

## CI / Release

The GitHub Actions workflow (`.github/workflows/build.yml`) runs on:
- **Manual dispatch** from the Actions tab
- **Tag push** matching `v*` (e.g. `v0.2.0`)

It produces a Windows installer (`.msi` and `.exe`) uploaded as a build artifact.

**To cut a release:**
```bash
git tag v0.x.0
git push origin v0.x.0
```

---

## AI Agent Guidelines

- **Frontend changes** (UI, styles, overlay): edit files in `src/`. The Tauri webview reloads automatically in dev mode.
- **Backend changes** (audio, transcription, settings): edit files in `src-tauri/src/`. Rust is compiled; always run `cargo check` after edits.
- **Adding a new transcription engine**: create a new `transcribe_<name>.rs` module, add it to `lib.rs`, and wire it in the `match settings.engine.as_str()` block in `recorder.rs`.
- **Settings changes**: update both the `Settings` struct in `settings.rs` and the corresponding form fields in `src/main.ts`.
- **Never hardcode API keys.** They are stored in `config.json` in the user's app data directory, not in the source tree.
- **Do not commit** `node_modules/`, `dist/`, or `src-tauri/target/`. These are covered by `.gitignore`.
- **Prefer async Tauri commands** for any operation that touches the network or filesystem from Rust.
- After any Rust change, verify with: `cd src-tauri && cargo test`
