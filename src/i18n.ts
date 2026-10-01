// Interface strings in English and Russian. Static markup is translated
// through data-i18n (text), data-i18n-placeholder and data-i18n-title
// attributes; code uses t().

export type Lang = "en" | "ru";

const en = {
  "nav.general": "General",
  "nav.engine": "Engine",
  "nav.recording": "Recording",
  "nav.developer": "Developer",

  "status.ready": "Ready",
  "status.recording": "Recording…",
  "status.transcribing": "Transcribing…",
  "status.error": "Error",
  "status.hotkeyOff": "Hotkey off",
  "status.hotkeyOffHint": "Turn it back on from the tray menu",

  "general.title": "General",
  "general.desc": "Audio input and app preferences",
  "general.microphone": "Microphone",
  "general.microphoneHint": "Input device used for dictation",
  "general.systemDefault": "System default",
  "general.systemDefaultNamed": "System default ({name})",
  "general.notConnected": "{name} (not connected)",
  "general.indicator": "Recording indicator",
  "general.indicatorHint": "Mic that slides in at the top of the screen while you dictate",
  "general.language": "Language",
  "general.languageHint": "Interface language",
  "general.advanced": "Advanced",
  "general.devMode": "Developer mode",
  "general.devModeHint": "Records everything the app does and shows it in the Developer section",

  "engine.title": "Engine",
  "engine.desc": "The cloud service that turns your speech into text",
  "engine.openai": "OpenAI Compatible",
  "engine.apiKey": "API Key",
  "engine.groqKey": "Groq API Key",
  "engine.groqKeyHint": "Get your key from console.groq.com",
  "engine.endpoint": "API Endpoint",
  "engine.endpointHint": "Base URL or /audio/transcriptions",
  "engine.modelId": "Model ID",
  "engine.openaiModelHint": "Model identifier (e.g. whisper-1)",
  "engine.openaiKeyHint": "API key (leave empty if not required)",
  "engine.polzaKeyHint": "Get your key from polza.ai/dashboard",
  "engine.polzaModelHint": "e.g. openai/whisper-large-v3",
  "engine.providerId": "Provider ID",
  "engine.providerIdHint": "Sub-provider (leave empty for auto)",
  "engine.providerIdPlaceholder": "e.g. openai-direct",
  "engine.assemblyaiKeyHint": "Get your key from assemblyai.com/dashboard",
  "engine.model": "Model",
  "engine.assemblyaiModelHint": "Speech recognition model",
  "engine.noRussian": "Doesn't support Russian",

  "recording.title": "Recording",
  "recording.desc": "How you start and stop a dictation",
  "recording.mode": "Recording Mode",
  "recording.modeHint": "Toggle starts/stops, Push to Talk records while held",
  "recording.toggle": "Toggle",
  "recording.ptt": "Push to Talk",
  "recording.hotkey": "Hotkey",
  "recording.hotkeyHint": "Global shortcut. Works only when no other keys are held",
  "recording.hotkeyChange": "Click to change shortcut",
  "recording.pressKeys": "Press 2–3 keys…",
  "recording.pressKeysHint": "Hold 2 or 3 keys simultaneously, then release",
  "recording.needKeys": "Need 2 or 3 keys!",
  "recording.saveError": "Error saving",

  "developer.title": "Developer",
  "developer.desc": "Live log of everything Typr does: hotkeys, audio, requests, errors",
  "developer.all": "All",
  "developer.errors": "Errors",
  "developer.warnings": "Warnings",
  "developer.info": "Info",
  "developer.debug": "Debug",
  "developer.filter": "Filter by text…",
  "developer.copy": "Copy",
  "developer.clear": "Clear",
  "developer.empty": "No logs yet",
  "developer.emptyHint": "Press your hotkey and dictate something. Every step will show up here.",
  "developer.noMatch": "Nothing matches",
  "developer.noMatchHint": "Try another level or clear the text filter.",
  "developer.jump": "Jump to latest ↓",
  "developer.nothingToCopy": "Nothing to copy",
  "developer.copied": "Copied {count}",
  "developer.copyFailed": "Copy failed",
} as const;

export type MessageKey = keyof typeof en;

const ru: Record<MessageKey, string> = {
  "nav.general": "Основные",
  "nav.engine": "Распознавание",
  "nav.recording": "Запись",
  "nav.developer": "Разработчик",

  "status.ready": "Готов",
  "status.recording": "Запись…",
  "status.transcribing": "Распознавание…",
  "status.error": "Ошибка",
  "status.hotkeyOff": "Хоткей выключен",
  "status.hotkeyOffHint": "Включите его снова в меню в трее",

  "general.title": "Основные",
  "general.desc": "Микрофон и настройки приложения",
  "general.microphone": "Микрофон",
  "general.microphoneHint": "Устройство ввода для диктовки",
  "general.systemDefault": "Системный по умолчанию",
  "general.systemDefaultNamed": "По умолчанию ({name})",
  "general.notConnected": "{name} (не подключён)",
  "general.indicator": "Индикатор записи",
  "general.indicatorHint": "Микрофон, который выезжает сверху экрана во время диктовки",
  "general.language": "Язык",
  "general.languageHint": "Язык интерфейса",
  "general.advanced": "Дополнительно",
  "general.devMode": "Режим разработчика",
  "general.devModeHint": "Записывает всё, что делает приложение, и показывает это в разделе «Разработчик»",

  "engine.title": "Распознавание",
  "engine.desc": "Облачный сервис, который превращает речь в текст",
  "engine.openai": "Совместимый с OpenAI",
  "engine.apiKey": "API-ключ",
  "engine.groqKey": "API-ключ Groq",
  "engine.groqKeyHint": "Получите ключ на console.groq.com",
  "engine.endpoint": "Адрес API",
  "engine.endpointHint": "Базовый URL или /audio/transcriptions",
  "engine.modelId": "ID модели",
  "engine.openaiModelHint": "Идентификатор модели (например, whisper-1)",
  "engine.openaiKeyHint": "Оставьте пустым, если ключ не нужен",
  "engine.polzaKeyHint": "Получите ключ на polza.ai/dashboard",
  "engine.polzaModelHint": "Например, openai/whisper-large-v3",
  "engine.providerId": "ID провайдера",
  "engine.providerIdHint": "Конкретный провайдер (пусто — автовыбор)",
  "engine.providerIdPlaceholder": "например, openai-direct",
  "engine.assemblyaiKeyHint": "Получите ключ на assemblyai.com/dashboard",
  "engine.model": "Модель",
  "engine.assemblyaiModelHint": "Модель распознавания речи",
  "engine.noRussian": "Не поддерживает русский язык",

  "recording.title": "Запись",
  "recording.desc": "Как начинать и останавливать диктовку",
  "recording.mode": "Режим записи",
  "recording.modeHint": "Переключение: нажал — старт, нажал ещё раз — стоп. Удержание: запись, пока клавиши зажаты",
  "recording.toggle": "Переключение",
  "recording.ptt": "Удержание",
  "recording.hotkey": "Горячие клавиши",
  "recording.hotkeyHint": "Срабатывают, только если не зажаты другие клавиши",
  "recording.hotkeyChange": "Нажмите, чтобы изменить сочетание",
  "recording.pressKeys": "Нажмите 2–3 клавиши…",
  "recording.pressKeysHint": "Зажмите 2 или 3 клавиши одновременно и отпустите",
  "recording.needKeys": "Нужно 2 или 3 клавиши!",
  "recording.saveError": "Ошибка сохранения",

  "developer.title": "Разработчик",
  "developer.desc": "Живой лог всего, что делает Typr: хоткеи, звук, запросы, ошибки",
  "developer.all": "Все",
  "developer.errors": "Ошибки",
  "developer.warnings": "Предупреждения",
  "developer.info": "Инфо",
  "developer.debug": "Отладка",
  "developer.filter": "Фильтр по тексту…",
  "developer.copy": "Копировать",
  "developer.clear": "Очистить",
  "developer.empty": "Логов пока нет",
  "developer.emptyHint": "Нажмите хоткей и продиктуйте что-нибудь. Здесь появится каждый шаг.",
  "developer.noMatch": "Ничего не найдено",
  "developer.noMatchHint": "Выберите другой уровень или очистите фильтр.",
  "developer.jump": "К последним ↓",
  "developer.nothingToCopy": "Нечего копировать",
  "developer.copied": "Скопировано: {count}",
  "developer.copyFailed": "Не удалось скопировать",
};

const dictionaries: Record<Lang, Record<MessageKey, string>> = { en, ru };

let current: Lang = "en";

export function getLanguage(): Lang {
  return current;
}

export function t(key: MessageKey, params: Record<string, string | number> = {}): string {
  let text = dictionaries[current][key] ?? en[key];
  for (const [name, value] of Object.entries(params)) {
    text = text.replace(`{${name}}`, String(value));
  }
  return text;
}

export function setLanguage(lang: Lang) {
  current = lang;
  document.documentElement.lang = lang;
  applyTranslations();
}

export function applyTranslations(root: ParentNode = document) {
  root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n as MessageKey);
  });
  root.querySelectorAll<HTMLInputElement>("[data-i18n-placeholder]").forEach((el) => {
    el.placeholder = t(el.dataset.i18nPlaceholder as MessageKey);
  });
  root.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((el) => {
    el.title = t(el.dataset.i18nTitle as MessageKey);
  });
}
