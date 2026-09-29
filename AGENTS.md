# AGENTS.md — Typr

Typr is a **desktop dictation app** built with [Tauri 2](https://tauri.app/) (Rust backend) and vanilla TypeScript frontend. It records audio via a global hotkey, transcribes it using Groq or OpenAI Whisper, and auto-pastes the result into the active window.

---

## Architecture

```
typr/
├── src/                        # TypeScript frontend (Vite)
│   ├── main.ts                 # Main UI logic, settings form, hotkey binding
│   ├── overlay.html            # Floating mic indicator overlay window
│   └── style.css               # Global styles
├── src-tauri/                  # Rust backend (Tauri)
│   └── src/
│       ├── lib.rs              # App entry — registers Tauri plugins, wires commands
│       ├── main.rs             # Binary entry point
│       ├── recorder.rs         # Orchestrates recording → transcription → paste flow
│       ├── audio.rs            # Low-level audio capture (cpal)
│       ├── transcribe_groq.rs  # Groq Whisper API client
│       ├── transcribe_openai.rs# OpenAI / OpenAI-compatible API client
│       ├── cleanup.rs          # Post-processing: trims filler words, fixes punctuation
│       ├── paste.rs            # Simulates Ctrl+V to paste text into active window
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
5. Calls `paste_text()` to inject into the active window
6. Emits `recording-state` events to the frontend and updates the overlay window

### `settings.rs` — Configuration
Persisted to `config.json` in the Tauri app data directory. Key fields:
- `engine`: `"groq"` | `"openai"` | `"openai-compatible"` (default: `"groq"`)
- `groqApiKey`, `openaiApiKey`, `openaiEndpoint`, `openaiModel`
- `microphone`: device name or `"default"`
- `recordingMode`: `"toggle"` | `"push-to-talk"`
- `hotkey`: default `"Ctrl+Shift+Space"`

> **Note:** The legacy `"local"` engine value is silently migrated to `"groq"` on load.

### `transcribe_groq.rs` / `transcribe_openai.rs`
Both send a multipart form POST with the WAV file to the respective API. `transcribe_openai.rs` is also used for any OpenAI-compatible endpoint (e.g. local Whisper servers) via the configurable `openaiEndpoint`.

### `cleanup.rs`
Strips common transcription artifacts (leading/trailing filler, repeated punctuation, etc.) before the text is pasted.

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
