// Post-processing section: provider, API key, model list with search and the
// style presets. The settings live in the app's settings object; this module
// edits them in place and asks the host to save.

import { invoke } from "@tauri-apps/api/core";
import { enhanceSelect, syncSelect } from "./dropdown";
import { t, type MessageKey } from "./i18n";

export interface PostProviderSettings {
  apiKey: string;
  model: string;
}

export interface PostProcessSettings {
  enabled: boolean;
  /** "gemini" | "openrouter" | "groq" | "polza" */
  provider: string;
  /** "chill" | "official" | "custom" */
  preset: string;
  customPrompt: string;
  gemini: PostProviderSettings;
  openrouter: PostProviderSettings;
  groq: PostProviderSettings;
  polza: PostProviderSettings;
}

type Provider = "gemini" | "openrouter" | "groq" | "polza";
type Preset = "chill" | "official" | "custom";

interface ModelInfo {
  id: string;
  name: string;
}

export interface PostProcessHost {
  settings: () => { postProcess: PostProcessSettings; groqApiKey: string; polzaApiKey: string };
  save: () => void;
}

const PROVIDERS: Provider[] = ["gemini", "openrouter", "groq", "polza"];
const PRESETS: Preset[] = ["chill", "official", "custom"];
const LABELS: Record<Provider, string> = {
  gemini: "Gemini",
  openrouter: "OpenRouter",
  groq: "Groq",
  polza: "Polza",
};
const KEY_HINTS: Record<Provider, MessageKey> = {
  gemini: "post.keyHintGemini",
  openrouter: "post.keyHintOpenrouter",
  groq: "post.keyHintEngine",
  polza: "post.keyHintEngine",
};

const enabledToggle = document.getElementById("post-enabled") as HTMLInputElement;
const providerSelect = document.getElementById("post-provider") as HTMLSelectElement;
const providerTitle = document.getElementById("post-provider-title")!;
const keyInput = document.getElementById("post-key") as HTMLInputElement;
const keyHint = document.getElementById("post-key-hint")!;
const modelCurrent = document.getElementById("post-model-current")!;
const refreshButton = document.getElementById("post-model-refresh")!;
const searchInput = document.getElementById("post-model-search") as HTMLInputElement;
const modelList = document.getElementById("post-model-list")!;
const modelStatus = document.getElementById("post-model-status")!;
const presetButtons = document.querySelectorAll<HTMLButtonElement>("#post-presets .segment");
const presetHint = document.getElementById("post-preset-hint")!;
const customRow = document.getElementById("post-custom-row")!;
const customPrompt = document.getElementById("post-custom-prompt") as HTMLTextAreaElement;

let host: PostProcessHost | undefined;

/** Loaded model lists, per provider, with the key they were loaded for */
const loaded = new Map<Provider, { key: string; models: ModelInfo[] }>();
/** What the model area shows, per provider */
const views = new Map<Provider, { kind: "needKey" | "loading" | "error" | "ready"; error?: string }>();
let requestId = 0;
let keyTimer: number | undefined;
let promptTimer: number | undefined;

function settings(): PostProcessSettings {
  return host!.settings().postProcess;
}

function currentProvider(): Provider {
  const provider = settings().provider as Provider;
  return PROVIDERS.includes(provider) ? provider : "gemini";
}

function currentPreset(): Preset {
  const preset = settings().preset as Preset;
  return PRESETS.includes(preset) ? preset : "official";
}

/** Groq and Polza can use the key from Engine */
function engineKey(provider: Provider): string {
  const all = host!.settings();
  if (provider === "groq") return (all.groqApiKey || "").trim();
  if (provider === "polza") return (all.polzaApiKey || "").trim();
  return "";
}

// ── Rendering ────────────────────────────────────────

function renderProvider() {
  const provider = currentProvider();
  providerSelect.value = provider;
  syncSelect(providerSelect);
  providerTitle.textContent = LABELS[provider];
  keyInput.value = settings()[provider].apiKey || "";
  keyHint.textContent = t(KEY_HINTS[provider]);
  searchInput.value = "";
  renderModels(true);
}

function modelName(provider: Provider, id: string): string {
  return loaded.get(provider)?.models.find((m) => m.id === id)?.name || id;
}

function renderCurrentModel() {
  const provider = currentProvider();
  const model = settings()[provider].model;
  modelCurrent.textContent = model
    ? t("post.modelChosen", { name: modelName(provider, model) })
    : t("post.modelNone");
}

function renderModels(scrollToSelected = false) {
  const provider = currentProvider();
  const view = views.get(provider) ?? { kind: "needKey" };
  const models = view.kind === "ready" ? loaded.get(provider)?.models ?? [] : [];
  const query = searchInput.value.trim().toLowerCase();
  const shown = query
    ? models.filter((m) => m.id.toLowerCase().includes(query) || m.name.toLowerCase().includes(query))
    : models;
  const selected = settings()[provider].model;

  modelList.replaceChildren(
    ...shown.map((model) => {
      const item = document.createElement("div");
      item.className = "model-option";
      item.setAttribute("role", "option");
      item.setAttribute("aria-selected", String(model.id === selected));
      item.dataset.id = model.id;
      item.title = model.id;

      const name = document.createElement("span");
      name.className = "model-name";
      name.textContent = model.name;
      item.append(name);
      if (model.name !== model.id) {
        const id = document.createElement("span");
        id.className = "model-id";
        id.textContent = model.id;
        item.append(id);
      }
      item.addEventListener("click", () => chooseModel(model.id));
      return item;
    }),
  );
  modelList.classList.toggle("hidden", shown.length === 0);

  let status = "";
  if (view.kind === "needKey") status = t("post.needKey");
  else if (view.kind === "loading") status = t("post.loading");
  else if (view.kind === "error") status = t("post.loadError", { error: view.error || "" });
  else if (shown.length === 0) status = t("post.nothingFound");
  modelStatus.textContent = status;
  modelStatus.classList.toggle("hidden", !status);
  modelStatus.classList.toggle("error", view.kind === "error");

  renderCurrentModel();

  if (scrollToSelected) {
    const item = modelList.querySelector<HTMLElement>('[aria-selected="true"]');
    modelList.scrollTop = item ? item.offsetTop - modelList.clientHeight / 2 + item.offsetHeight / 2 : 0;
  }
}

function renderPreset() {
  const preset = currentPreset();
  presetButtons.forEach((b) => b.classList.toggle("active", b.dataset.preset === preset));
  presetHint.textContent = t(`post.${preset}Hint` as MessageKey);
  customRow.classList.toggle("hidden", preset !== "custom");
}

// ── Models ───────────────────────────────────────────

async function loadModels(force = false) {
  const provider = currentProvider();
  const typed = keyInput.value.trim();
  const fallback = typed ? "" : engineKey(provider);
  if (!typed && !fallback) {
    views.set(provider, { kind: "needKey" });
    renderModels();
    return;
  }

  const cacheKey = typed || `engine:${fallback}`;
  if (!force && loaded.get(provider)?.key === cacheKey && views.get(provider)?.kind === "ready") {
    renderModels();
    return;
  }

  const id = ++requestId;
  views.set(provider, { kind: "loading" });
  renderModels();
  try {
    // An empty key makes the backend use the Engine key (Groq, Polza)
    const models = await invoke<ModelInfo[]>("list_postprocess_models", { provider, apiKey: typed });
    if (id !== requestId) return;
    loaded.set(provider, { key: cacheKey, models });
    views.set(provider, { kind: "ready" });
  } catch (err) {
    if (id !== requestId) return;
    views.set(provider, { kind: "error", error: String(err) });
  }
  if (currentProvider() === provider) renderModels(true);
}

function chooseModel(id: string) {
  const provider = currentProvider();
  settings()[provider].model = id;
  host!.save();
  modelList.querySelectorAll<HTMLElement>(".model-option").forEach((item) => {
    item.setAttribute("aria-selected", String(item.dataset.id === id));
  });
  renderCurrentModel();
}

// ── Public ───────────────────────────────────────────

export function initPostProcess(appHost: PostProcessHost) {
  host = appHost;
  enhanceSelect(providerSelect);

  enabledToggle.addEventListener("change", () => {
    settings().enabled = enabledToggle.checked;
    host!.save();
  });

  providerSelect.addEventListener("change", () => {
    settings().provider = providerSelect.value;
    host!.save();
    renderProvider();
    void loadModels();
  });

  keyInput.addEventListener("input", () => {
    settings()[currentProvider()].apiKey = keyInput.value.trim();
    window.clearTimeout(keyTimer);
    keyTimer = window.setTimeout(() => void loadModels(), 700);
  });
  keyInput.addEventListener("change", () => {
    settings()[currentProvider()].apiKey = keyInput.value.trim();
    host!.save();
  });

  refreshButton.addEventListener("click", () => void loadModels(true));
  searchInput.addEventListener("input", () => renderModels());

  presetButtons.forEach((button) => {
    button.addEventListener("click", () => {
      settings().preset = button.dataset.preset || "official";
      host!.save();
      renderPreset();
    });
  });

  customPrompt.addEventListener("input", () => {
    settings().customPrompt = customPrompt.value;
    window.clearTimeout(promptTimer);
    promptTimer = window.setTimeout(() => host!.save(), 800);
  });
  customPrompt.addEventListener("change", () => {
    window.clearTimeout(promptTimer);
    settings().customPrompt = customPrompt.value;
    host!.save();
  });
}

/** Puts the saved settings into the form (after they are loaded). */
export function fillPostProcess() {
  enabledToggle.checked = Boolean(settings().enabled);
  customPrompt.value = settings().customPrompt || "";
  renderProvider();
  renderPreset();
}

/** The section was opened: load the model list if it isn't there yet. */
export function showPostProcess() {
  if (host) void loadModels();
}

/** Texts set from code, after a language switch. */
export function translatePostProcess() {
  if (!host) return;
  keyHint.textContent = t(KEY_HINTS[currentProvider()]);
  renderPreset();
  renderModels();
}
