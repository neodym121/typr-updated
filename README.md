<p align="center">
  <img src="assets/logo.svg" width="96" height="96" alt="Typr logo" />
</p>

<h1 align="center">Typr</h1>

<p align="center">
  <b>English</b> · <a href="README.ru.md">Русский</a>
</p>

Typr is a lightweight desktop dictation app for Windows. Press a global hotkey, speak, and the recognized text is pasted into whatever window is active.

<p align="center">
  <img src="assets/screenshot.png" width="720" alt="Typr settings window" />
</p>

## Features

- **Global hotkey** in two modes.
- **Speech-to-text providers:** Groq (Whisper Large v3/v3 Turbo), [Polza.AI](https://polza.ai/), [AssemblyAI](https://www.assemblyai.com/) (Universal-3.5 Pro/Universal-2), and any OpenAI-compatible endpoint.
- **Post-processing:** an AI model (Gemini, OpenRouter, Groq or Polza) polishes the text before it is pasted, in the Chill, Proper or your own style.
- **Clean clipboard:** the dictated text is pasted through the clipboard and stays out of the Windows clipboard history.
- **Space after the text**, so the next phrase doesn't stick to the previous one.
- **Recording indicator:** a small mic slides down from behind the top edge of the screen only while you dictate (recording, transcribing, pasting) and hides again.
- **Tiny footprint:** about 5 MB of RAM in the tray! The settings window's WebView is released when it's closed, and the recording indicator is drawn natively.
- **Developer mode:** a live log of everything the app does, for troubleshooting.
- **English and Russian interface.**
- Dark, minimal interface in the Anthropic style.

## Building

The installer is built by GitHub Actions (`.github/workflows/build.yml`) when a `v*` tag is pushed or the workflow is started manually:

```bash
git tag v1.2.2
git push origin v1.2.2
```

Local development needs [Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/) 20+:

```bash
npm install
npm run tauri dev
```

Built with [Tauri 2](https://tauri.app/) (Rust) and vanilla TypeScript.

## Credits

- Original project by [albertshiney](https://github.com/albertshiney/typr)
- Fork maintained by [neodym121](https://github.com/neodym121), improved together with [Claude](https://claude.com/claude-code) (Anthropic)

## License

[MIT](LICENSE)
