# AGENTS.md — Typr

Typr is a **desktop dictation app** built with [Tauri 2](https://tauri.app/) (Rust backend) and vanilla TypeScript frontend. It records audio via a global hotkey, transcribes it locally with [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) or in the cloud with Groq, OpenAI (or any OpenAI-compatible API), Polza.AI or AssemblyAI, optionally polishes the text with a language model, and auto-pastes the result into the active window. The interface is in English and Russian.

---

## Architecture

```
typr/
├── src/                        # TypeScript frontend (Vite)
│   ├── main.ts                 # Main UI logic, settings form, hotkey binding
│   ├── setup.ts                # First-run setup wizard over the whole window
│   ├── i18n.ts                 # English/Russian UI strings, t(), data-i18n attributes, recognition language names
│   ├── dropdown.ts             # App-styled dropdowns over hidden native <select>s
│   ├── postprocess.ts          # Post-processing section: provider, key, model list with search, styles
│   ├── local.ts                # Engine → Local: models, acceleration, runtime components, unloading
│   ├── overlay.html            # Mic indicator overlay (WebView fallback for non-Windows)
│   └── style.css               # Global styles
├── src-tauri/                  # Rust backend (Tauri)
│   └── src/
│       ├── lib.rs              # App entry — registers Tauri plugins, wires commands
│       ├── main.rs             # Binary entry point
│       ├── recorder.rs         # Orchestrates recording → transcription → paste flow, local fallback
│       ├── audio.rs            # Low-level audio capture (cpal), WAV/FLAC packing in memory
│       ├── transcribe_groq.rs  # Groq Whisper API client
│       ├── transcribe_openai.rs# OpenAI / OpenAI-compatible API client
│       ├── transcribe_polza.rs # Polza.AI client (JSON body, base64 data URL)
│       ├── transcribe_assemblyai.rs # AssemblyAI client (upload → transcript job → poll)
│       ├── transcribe_local.rs # Local engine entry: in-memory samples → local::LocalEngine
│       ├── local/              # Local recognition with transcribe.cpp
│       │   ├── mod.rs          # LocalEngine: runtime + model in memory, preload, unload timer, downloads, status
│       │   ├── catalog.rs      # Models and the runtime with pinned URLs, sizes, SHA-256
│       │   ├── hardware.rs     # GPUs from the registry, whether Vulkan is available
│       │   ├── download.rs     # Resumable, cancellable, verified downloads
│       │   ├── runtime.rs      # Files on disk, unpacking and checking the runtime
│       │   ├── worker.rs       # The recognition process (`typr --local-recognition`) and Typr's side of it
│       │   └── ffi.rs          # transcribe.dll loaded at run time (C API of v0.2.4)
│       ├── keyboard.rs         # Hotkey must be pressed on its own (GetAsyncKeyState check)
│       ├── i18n.rs             # System language detection, tray menu strings
│       ├── net.rs              # One shared HTTP client (timeouts, kept-alive connections, warm-up) + response/error helpers
│       ├── autostart.rs        # Start with Windows (HKCU Run value, `--autostart`)
│       ├── updates.rs          # Latest GitHub release vs this version
│       ├── links.rs            # Opens allowed web pages in the browser
│       ├── logger.rs           # `log` backend feeding the Developer section
│       ├── overlay.rs          # Recording indicator that slides in while dictating (native layered window on Windows)
│       ├── cleanup.rs          # Tidies the transcript: whitespace, capitals at sentence starts, final punctuation
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
2. Packs the samples in memory for a cloud engine (FLAC for Groq, AssemblyAI and OpenAI itself; WAV for Polza and other OpenAI-compatible servers); nothing is written to disk
3. Calls the configured transcription engine
   (the local engine takes the samples straight to transcribe.cpp). If a cloud engine fails for any reason, `fallbackLocal` is on and a local model is downloaded, the local model transcribes the recording instead (`transcription-fallback` event); the next dictation tries the cloud again
4. Runs `cleanup_text()` on the result
5. If post-processing is on, rewrites the text with `postprocess::process()`; on failure the transcribed text is pasted anyway and the error is reported afterwards
6. Calls `paste_text()` to inject into the active window
7. Emits `recording-state` events to the frontend and updates the overlay (the indicator slides in for Recording/Transcribing and back out on Ready)

### `settings.rs` — Configuration
Persisted to `config.json` in the Tauri app data directory (`TYPR_CONFIG_DIR` overrides the folder in debug builds). `#[serde(default)]` on the structs: a missing field takes its value from `Default`. Engine, recording mode, post-processing provider and preset are enums saved as string ids (`string_enum!`); an unknown id reads as the default instead of failing the file. Key fields:
- `engine`: `"local"` | `"groq"` | `"openai"` (`"openai-compatible"` reads as it) | `"polza"` | `"assemblyai"` (default: `"groq"`)
- `groqModel`: `"whisper-large-v3-turbo"` (default) | `"whisper-large-v3"`
- `groqApiKey`, `openaiApiKey`, `openaiEndpoint`, `openaiModel`, `polzaApiKey`, `polzaModel`, `polzaProvider`, `assemblyaiApiKey`
- `assemblyaiModel`: `"universal-3-5-pro"` (default, no Russian) | `"universal-2"`
- `recognitionLanguage`: ISO 639-1 code of the speech (`RECOGNITION_LANGUAGES`), empty detects it. Sent as `language` (Groq, OpenAI, Polza), `language_code` (AssemblyAI) and the run params' `language` (local; a language the model doesn't list falls back to detection)
- `fallbackLocal`: a failed cloud transcription goes to a downloaded local model (default `true`)
- `localModel`: model id from `local/catalog.rs` used for dictation (default `"parakeet-tdt-0.6b-v3"`)
- `localBackend`: `"vulkan"` | `"cpu"`, or empty for the one recommended for this computer
- `localUnload`: when an idle model leaves memory: `"immediate"` | `"30s"` | `"5m"` (default) | `"10m"` | `"never"`
- `language`: `"en"` | `"ru"` | `""` (empty follows the system, see `i18n.rs`)
- `developerMode`: shows the Developer section and turns log collection on
- `microphone`: device name or `"default"`
- `recordingMode`: `"toggle"` | `"push-to-talk"`
- `hotkey`: default `"Ctrl+Shift+Space"`
- `appendSpace`: adds a space after the pasted text (default `false`, switch in Post-processing; works with post-processing off too)
- `showIndicator`: whether the recording indicator may appear at all (default `true`, switch in General)
- `checkUpdates`: looks for a newer GitHub release 15 s after start and every 12 h (default `true`)
- `setupDone`: the first-run wizard was finished or skipped; `false` only for a new config (a config file without the field counts as set up)
- `postProcess`: `enabled` (default `false`), `provider` (`"gemini"` | `"openrouter"` | `"groq"` | `"polza"`), `preset` (`"chill"` | `"proper"` | `"custom"`; the old `"official"` is migrated to `"proper"` on load), `customPrompt`, and `{ apiKey, model }` per provider, plus `providerId` for Polza (sub-provider sent as `provider.only`, omitted from the file when empty). Groq and Polza fall back to the Engine key when their own is empty (`Settings::post_process_key`)

### `transcribe_groq.rs` / `transcribe_openai.rs`
Both send a multipart form POST with the recording (`audio::EncodedAudio`) to the respective API. `transcribe_openai.rs` is also used for any OpenAI-compatible endpoint (e.g. local Whisper servers) via the configurable `openaiEndpoint`; those get WAV, only `api.openai.com` gets FLAC.

### `net.rs` — one HTTP client
`net::client()` returns one `reqwest::Client` for the whole app, so connections stay open between dictations. `recorder::warm_up_connections` sends a HEAD request (no credentials) to the engine's and post-processing's servers when a dictation starts, so the TLS handshake happens while the user speaks.

### `transcribe_assemblyai.rs`
Three steps: `POST /v2/upload` with the raw FLAC, `POST /v2/transcript` with `speech_models: [model]` and `language_code` (or `language_detection: true` when no language is set), then poll `GET /v2/transcript/{id}` until `completed` or `error`. Universal-3.5 Pro supports 18 languages without Russian; if it rejects the language, the error suggests Universal-2.

### `local/` — local recognition (transcribe.cpp)
- Nothing native is compiled with Typr. `ffi.rs` loads `transcribe.dll` with LoadLibrary from the **runtime** downloaded on demand, mirrors the v0.2.4 C header (`include/transcribe.h` at the tag) and refuses a DLL whose version or struct sizes (`transcribe_abi_struct_size`) differ. Bump `TRANSCRIBE_VERSION`/`HEADER_HASH` in `catalog.rs`, the structs in `ffi.rs` and the pinned bundle together.
- The runtime (`catalog.rs` `RUNTIME`) is the official `windows-x86_64-cpu-vulkan` bundle: Vulkan for any GPU with a Vulkan driver, CPU modules for every processor. Its `contract.json` must match `TRANSCRIBE_VERSION` and `HEADER_HASH`. CUDA and ROCm are left out on purpose: their kernels are compiled per GPU generation, so each needs its own builds and checks (the official CUDA bundle covers compute capability 7.5–9.0 only; ROCm has no official Windows build). Measured on an RX 7600 (Parakeet, 11 s of audio, warm): Vulkan 0.12 s, CPU 0.8 s.
- `hardware.rs`: GPUs from the display adapter class in the registry (PCI devices) and `vulkan-1.dll` in System32. A GPU with Vulkan → Vulkan recommended, otherwise the CPU (Vulkan greyed out).
- transcribe.cpp never loads into Typr itself: it can't be unloaded from a process (backend modules and the Vulkan driver stay, about 40 MB). `worker.rs` starts Typr's own executable with `--local-recognition` (checked first thing in `main()`, before the logger and Tauri) as a child process that loads the runtime and one model and transcribes. It talks over stdin/stdout in frames (u32 length + JSON; the samples as one frame of f32), forwards its log records into Typr's log and its stderr as warnings. Closing its stdin makes it free the model and exit (killed after 3 s). So an unloaded model leaves Typr at its idle few megabytes, and a GPU driver crash ends only the child: Typr then runs that dictation on the CPU and keeps the CPU (reason under Acceleration) until the Acceleration choice changes. Starting the child costs about 0.1 s on top of loading the model.
- `LocalEngine` keeps one model (one child): started when a dictation starts (`begin_recording`), ended by `schedule_unload` after `localUnload` (a generation counter cancels stale timers), right away when the engine/model/backend changes or the model/runtime is deleted. A GPU load failure falls back to the CPU inside the child (the reason shows under Acceleration while that model is in memory).
- Models (Q8_0 GGUF from `huggingface.co/handy-computer`) and the runtime live in `%LOCALAPPDATA%\com.typr.app\local` (`models`, `runtimes`, `downloads`); downloads resume from `.part` files and are checked against size and SHA-256.
- The runtime comes with the first model download. Its row (download, delete) is in the Developer section; Engine → Local only shows a note with Download/Retry when a downloaded model lacks it, and a dictation without it starts the download.
- Tauri commands: `local_status`, `local_download_model`, `local_download_runtime`, `local_cancel_download`, `local_delete_model`, `local_delete_runtime`; events `local-download` (progress) and `local-changed` (re-read the status).
- `cargo test --test local_recognition -- --ignored --nocapture` with `TYPR_TEST_WAV` (16 kHz mono), the runtime and Parakeet downloaded: transcribes on the real GPU and CPU through the child process and checks that Typr's own memory stays small and the child ends on unload.

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
Collapses whitespace, capitalizes the first letter of every sentence and adds a final period. A sentence ends at `.`, `!` or `?` followed by a space, not inside numbers (3.14), addresses (example.com) or abbreviations (т.е., e.g., см.), and not after an ellipsis. Words that already have a capital (iPhone) are left alone; a trailing comma becomes the period.

### Tray menu, autostart, updates
- Tray: Open settings, Paste last dictation (the last text goes back into the window it was pasted into; if that window is gone, it is put on the clipboard), Engine submenu (switches `engine`, the window follows through `engine-changed`), Hotkey On/Off, Exit, and a "Version X is available" item added on top once an update is found.
- The main window is `"create": false` in tauri.conf.json and built in `setup`; started with `--autostart` (the Run value from `autostart.rs`), Typr stays in the tray unless the setup isn't done. `autostart::refresh` repoints the Run value at the current executable (release builds only).
- `updates::check` reads `releases/latest` of `neodym121/typr-updated`; the result is kept in `AppState.update`, sent as `update-available` and shown in the sidebar and General. `links::open` opens only the release page and the providers' key pages.

### `setup.ts` — first-run wizard
Shown while `setupDone` is false (and from General → Advanced): interface language, engine (Local recommended when Vulkan is available, else Groq), the engine's model download or API key plus the speech language, then hotkey, recording mode, start with Windows and update checks. Finish or Skip sets `setupDone`.

### `logger.rs` — Developer section
Installed as the global `log` backend in `main()`. Use `log::info!` / `log::warn!` / `log::error!` / `log::debug!` everywhere in Rust (not `println!`). While `developerMode` is on, entries go to a 5000-entry ring buffer and are streamed to the main window as `log-entry` events; the frontend forwards its own console output via the `frontend_log` command. Never log API keys — use `Settings::summary()` / `describe_changes()`, which mask them.

`stop_and_transcribe` must always return the recorder to `Ready`, even on errors; failures are reported to the UI with `recorder::notify_error` (`recording-error` event).

### Memory: no WebView while idle
- `overlay.rs` draws the recording indicator itself on Windows (`windows-sys` layered window, per-pixel alpha, SDF rendering) — no WebView. Other platforms fall back to the WebView overlay `src/overlay.html`.
- The indicator is out of sight while idle: its window starts at the top edge of the primary screen and the disc is drawn above that edge, so it slides in from behind the screen when a dictation starts (or briefly after an error) and slides back out on `Ready`. Once fully out, the window is hidden and its timers stop, so it costs no CPU.
- `overlay::set_enabled` gates it: `main.rs::sync_overlay` allows it only while `showIndicator` is on and the hotkey is on in the tray.
- Closing the main window destroys it (and its WebView); `show_main_window` re-creates it from `tauri.conf.json` on a separate thread (building windows in event handlers deadlocks on Windows).
- The page runs under a strict CSP (`tauri.conf.json`): scripts from the app only, styles and fonts also from Google Fonts, IPC. New external resources need a CSP entry. `RunEvent::ExitRequested` without a code is prevented, so Typr keeps running in the tray; tray → Exit calls `app.exit(0)`.
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

`build.rs` embeds the Common Controls v6 manifest (`windows-app-manifest.xml`) through the linker for every target, so test binaries that link Tauri start on Windows (otherwise `STATUS_ENTRYPOINT_NOT_FOUND`).

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
