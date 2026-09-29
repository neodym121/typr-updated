import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface Settings {
  microphone: string;
  engine: string;
  groqApiKey: string;
  openaiEndpoint: string;
  openaiModel: string;
  openaiApiKey: string;
  recordingMode: string;
  hotkey: string;
}

interface MicDevice {
  name: string;
  is_default: boolean;
}

// DOM elements
const statusDot = document.getElementById("status-dot")!;
const statusText = document.getElementById("status-text")!;
const micSelect = document.getElementById("mic-select") as HTMLSelectElement;
const engineGroq = document.getElementById("engine-groq")!;
const engineOpenai = document.getElementById("engine-openai")!;
const groqSettings = document.getElementById("groq-settings")!;
const openaiSettings = document.getElementById("openai-settings")!;
const groqKey = document.getElementById("groq-key") as HTMLInputElement;
const openaiEndpoint = document.getElementById("openai-endpoint") as HTMLInputElement;
const openaiModel = document.getElementById("openai-model") as HTMLInputElement;
const openaiKey = document.getElementById("openai-key") as HTMLInputElement;
const modeToggle = document.getElementById("mode-toggle")!;
const modePtt = document.getElementById("mode-ptt")!;
const hotkeyBtn = document.getElementById("hotkey-btn")!;
const hotkeyText = document.getElementById("hotkey-text")!;
const hotkeyHint = document.getElementById("hotkey-hint")!;

// Section navigation
const navItems = document.querySelectorAll(".nav-item");
const sections = document.querySelectorAll(".content-section");

navItems.forEach((item) => {
  item.addEventListener("click", () => {
    const target = item.getAttribute("data-section");
    navItems.forEach((n) => n.classList.remove("active"));
    sections.forEach((s) => s.classList.remove("active"));
    item.classList.add("active");
    document.getElementById(`section-${target}`)?.classList.add("active");
  });
});

// Window drag — titlebar and sidebar empty space
const titlebar = document.getElementById("titlebar")!;
const sidebar = document.getElementById("sidebar")!;
const appWindow = getCurrentWindow();

titlebar.addEventListener("mousedown", (e) => {
  if ((e.target as HTMLElement).closest("button, select, input, a, .nav-item")) return;
  appWindow.startDragging();
});

sidebar.addEventListener("mousedown", (e) => {
  if ((e.target as HTMLElement).closest("button, select, input, a, .nav-item")) return;
  appWindow.startDragging();
});

let currentSettings: Settings;

function formatKeyForDisplay(key: string): string {
  const isMac = navigator.platform.toUpperCase().indexOf("MAC") >= 0;
  if (key === "Control" || key === "Ctrl" || key === "CmdOrCtrl") return isMac ? "Cmd" : "Ctrl";
  if (key === "Shift") return "Shift";
  if (key === "Alt") return "Alt";
  if (key === "Meta" || key === "Super") return isMac ? "Cmd" : "Win";
  if (key === "Space") return "Space";
  if (key.startsWith("Key")) return key.slice(3).toUpperCase();
  if (key.startsWith("Digit")) return key.slice(5);
  return key;
}

function displayHotkey(hotkeyStr: string) {
  if (!hotkeyStr) return;
  const parts = hotkeyStr.split("+");
  const formatted = parts.map(formatKeyForDisplay);
  hotkeyText.textContent = formatted.join("+");
}

async function loadSettings() {
  currentSettings = await invoke<Settings>("get_settings");

  // Populate mic dropdown
  const mics = await invoke<MicDevice[]>("list_microphones");
  micSelect.innerHTML = "";
  mics.forEach((mic) => {
    const option = document.createElement("option");
    option.value = mic.name;
    option.textContent = mic.name + (mic.is_default ? " (default)" : "");
    micSelect.appendChild(option);
  });
  micSelect.value = currentSettings.microphone;

  // Engine
  setEngine(currentSettings.engine || "groq");

  // Groq key
  groqKey.value = currentSettings.groqApiKey || "";

  // OpenAI Compatible settings
  openaiEndpoint.value = currentSettings.openaiEndpoint || "https://api.openai.com/v1";
  openaiModel.value = currentSettings.openaiModel || "whisper-1";
  openaiKey.value = currentSettings.openaiApiKey || "";

  // Recording mode
  setRecordingMode(currentSettings.recordingMode || "toggle");

  // Hotkey
  displayHotkey(currentSettings.hotkey);
}

function setEngine(engine: string) {
  currentSettings.engine = engine;
  const isGroq = engine === "groq";
  engineGroq.classList.toggle("active", isGroq);
  engineOpenai.classList.toggle("active", !isGroq);
  groqSettings.classList.toggle("hidden", !isGroq);
  openaiSettings.classList.toggle("hidden", isGroq);
}

function setRecordingMode(mode: string) {
  currentSettings.recordingMode = mode;
  modeToggle.classList.toggle("active", mode === "toggle");
  modePtt.classList.toggle("active", mode === "push-to-talk");
}

async function saveSettings() {
  currentSettings.microphone = micSelect.value;
  currentSettings.groqApiKey = groqKey.value.trim();
  currentSettings.openaiEndpoint = openaiEndpoint.value.trim();
  currentSettings.openaiModel = openaiModel.value.trim();
  currentSettings.openaiApiKey = openaiKey.value.trim();
  await invoke("save_settings", { settings: currentSettings });
}

// Event listeners for settings
engineGroq.addEventListener("click", () => {
  setEngine("groq");
  saveSettings();
});

engineOpenai.addEventListener("click", () => {
  setEngine("openai");
  saveSettings();
});

micSelect.addEventListener("change", () => saveSettings());
groqKey.addEventListener("change", () => saveSettings());
openaiEndpoint.addEventListener("change", () => saveSettings());
openaiModel.addEventListener("change", () => saveSettings());
openaiKey.addEventListener("change", () => saveSettings());

modeToggle.addEventListener("click", () => {
  setRecordingMode("toggle");
  saveSettings();
});

modePtt.addEventListener("click", () => {
  setRecordingMode("push-to-talk");
  saveSettings();
});

// Hotkey Recording Logic
let isRecordingHotkey = false;

function startHotkeyRecording() {
  if (isRecordingHotkey) return;
  isRecordingHotkey = true;

  const pressedKeys = new Set<string>();
  const strokeKeys = new Set<string>();

  hotkeyBtn.classList.add("recording");
  hotkeyText.textContent = "Press 2–3 keys...";
  hotkeyHint.textContent = "Hold 2 or 3 keys simultaneously, then release";

  const getKeyIdentifier = (e: KeyboardEvent): string => {
    if (e.key === "Control") return "Ctrl";
    if (e.key === "Shift") return "Shift";
    if (e.key === "Alt") return "Alt";
    if (e.key === "Meta") return "Super";
    if (e.code === "Space") return "Space";
    if (e.code.startsWith("Key")) return e.code; // KeyA - KeyZ
    if (e.code.startsWith("Digit")) return e.code; // Digit0 - Digit9
    if (e.code.startsWith("F") && !isNaN(Number(e.code.slice(1)))) return e.code; // F1 - F12
    if (e.code) return e.code;
    return e.key;
  };

  const onKeyDown = (e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape" && pressedKeys.size === 0) {
      cleanup(false);
      return;
    }

    const keyName = getKeyIdentifier(e);
    if (!keyName) return;

    if (pressedKeys.size >= 3 && !pressedKeys.has(keyName)) {
      return; // maximum 3 keys
    }

    pressedKeys.add(keyName);
    strokeKeys.add(keyName);

    const displayList = Array.from(strokeKeys).map(formatKeyForDisplay);
    hotkeyText.textContent = displayList.join("+") + (strokeKeys.size < 2 ? " + ..." : "");
  };

  const onKeyUp = (e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const count = strokeKeys.size;
    if (count >= 2 && count <= 3) {
      // Valid shortcut recorded!
      const modOrder = ["Ctrl", "Alt", "Shift", "Super"];
      const sorted = Array.from(strokeKeys).sort((a, b) => {
        const aMod = modOrder.indexOf(a);
        const bMod = modOrder.indexOf(b);
        if (aMod !== -1 && bMod !== -1) return aMod - bMod;
        if (aMod !== -1) return -1;
        if (bMod !== -1) return 1;
        return a.localeCompare(b);
      });

      const newHotkey = sorted.join("+");
      currentSettings.hotkey = newHotkey;

      saveSettings()
        .then(() => {
          cleanup(true);
        })
        .catch((err) => {
          console.error("Failed to save hotkey:", err);
          hotkeyText.textContent = "Error saving";
          setTimeout(() => cleanup(false), 1200);
        });
      return;
    }

    const keyName = getKeyIdentifier(e);
    pressedKeys.delete(keyName);

    if (pressedKeys.size === 0) {
      strokeKeys.clear();
      hotkeyText.textContent = "Need 2 or 3 keys!";
      setTimeout(() => {
        if (isRecordingHotkey && pressedKeys.size === 0) {
          hotkeyText.textContent = "Press 2–3 keys...";
        }
      }, 1000);
    }
  };

  const onOutsideClick = (e: MouseEvent) => {
    if (!hotkeyBtn.contains(e.target as Node)) {
      cleanup(false);
    }
  };

  const cleanup = (_saved: boolean) => {
    isRecordingHotkey = false;
    hotkeyBtn.classList.remove("recording");
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("keyup", onKeyUp, true);
    window.removeEventListener("mousedown", onOutsideClick, true);
    hotkeyHint.textContent = "Global keyboard shortcut to trigger recording";
    displayHotkey(currentSettings.hotkey);
  };

  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("keyup", onKeyUp, true);
  window.addEventListener("mousedown", onOutsideClick, true);
}

hotkeyBtn.addEventListener("click", () => {
  startHotkeyRecording();
});

// Listen for recording state changes
listen<string>("recording-state", (event) => {
  const state = event.payload;
  statusDot.className = "";
  if (state === "Recording") {
    statusDot.classList.add("recording");
    statusText.textContent = "Recording...";
  } else if (state === "Transcribing") {
    statusDot.classList.add("transcribing");
    statusText.textContent = "Transcribing...";
  } else {
    statusDot.classList.add("ready");
    statusText.textContent = "Ready";
  }
});

// Initialize
loadSettings();
