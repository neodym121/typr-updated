<p align="center">
  <img src="c.svg" width="96" height="96" alt="Typr logo" />
</p>

<h1 align="center">Typr</h1>

<p align="center">
  <b>English</b> · <a href="README.ru.md">Русский</a>
</p>

Typr is a lightweight desktop dictation app for Windows. Press a global hotkey, speak, and the recognized text is pasted into whatever window is active.

## Features

- **Global hotkey** in two modes: *Toggle* (press to start/stop) or *Push to Talk* (record while held)
- **Speech-to-text providers:** Groq, OpenAI or any OpenAI-compatible endpoint, and [Polza.AI](https://polza.ai)
- **Recording indicator:** a small mic slides down from behind the top edge of the screen only while you dictate (recording, transcribing, pasting) and hides again afterwards. It can be turned off in **General**
- **Tiny footprint:** ~5 MB of RAM while idle in the tray. The settings window's WebView is released when it's closed, and the recording indicator is drawn natively
- **Tray menu:** turn the hotkey on/off (handy while gaming), open settings, exit
- **Developer mode:** a live log of everything the app does, for troubleshooting
- Dark, minimal interface

## Installation

Download the Windows installer (`.msi` or `.exe`) from the `typr-windows` artifact of the latest successful run in the [Actions](../../actions) tab, then add your API key in **Engine**.

## Building

The installer is built by GitHub Actions (`.github/workflows/build.yml`) when a `v*` tag is pushed or the workflow is started manually:

```bash
git tag v1.0.5
git push origin v1.0.5
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
