# Polza.ai — справка для ИИ-модели

Этот документ — сжатый, но полный конспект официальной документации Polza.ai (polza.ai/docs, снято 18 сентября 2026). Цель: чтобы модель, никогда не слышавшая о Polza.ai, после прочтения на 100% понимала, что это за сервис и как с ним работать.

## 1. Что такое Polza.ai

**Polza.ai** — российский LLM-агрегатор («№1 LLM-агрегатор в России»): унифицированный API-интерфейс, дающий доступ к 400+ нейросетям (текст, изображения, видео, аудио, эмбеддинги) от разных провайдеров (OpenAI, Anthropic, Google, DeepSeek, Meta*, Mistral, Qwen, Microsoft, Perplexity, Cohere, xAI, Minimax и др.) через один API-ключ и один биллинг.

Ключевая идея: разработчик пишет код один раз (используя привычный OpenAI SDK), а Polza.ai сама решает, к какому провайдеру и с каким конкретно бэкендом отправить запрос — либо разработчик управляет выбором сам через параметр `provider`.

### Ключевые преимущества (заявлены официально)
- **400+ моделей** через единое API — GPT, Claude, Gemini, Llama и др.
- **Оплата в рублях**, без конвертации валют, без скрытых комиссий.
- **Без VPN и без прокси** — прямой доступ из России.
- **OpenAI-совместимость** — можно использовать стандартные SDK (openai для Python/TS) без изменения кода, просто сменить `base_url`.
- **Единый биллинг** для всех моделей и провайдеров.
- **Повышенная надёжность**: резервные провайдеры и умная маршрутизация (fallback при сбоях).
- **Увеличенные лимиты запросов** за счёт прямых договорённостей с провайдерами.
- Минимальный порог входа для пополнения баланса — около 100 ₽.
- Поддержка юрлиц/ИП с закрывающими документами (для бизнеса).

### Для кого сервис
Разработчики, стартапы (снижение инфраструктурных затрат), корпорации (Enterprise: SLA, выделенная поддержка), исследователи (сравнение моделей).

## 2. Базовые технические факты

- **Базовый URL API:** `https://polza.ai/api` (для OpenAI-совместимых эндпоинтов — `https://polza.ai/api/v1`).
- **Аутентификация:** заголовок `Authorization: Bearer <POLZA_AI_API_KEY>`. Ключ создаётся в личном кабинете: `polza.ai/dashboard/api-keys`.
- **Формат:** REST API, JSON, полностью совместим со стандартом OpenAI Chat Completion API — можно просто заменить `base_url` в официальном OpenAI SDK и всё заработает.
- **Официальные библиотеки:** PHP (`composer require polzaai/client`) и Ruby (`gem install polzaai`); для остальных языков — прямые HTTP-запросы или стандартный OpenAI SDK.
- **Лимиты:** макс. размер файла — 50 MB; макс. размер изображения (хранилище) — 50 MB; таймаут запроса — 600 секунд.
- **Документация:** полный индекс страниц доступен по адресу `https://polza.ai/docs/llms.txt` (машиночитаемый список всех разделов документации).
- **MCP-сервер (для ИИ-агентов):** `https://polza.ai/api/mcp` — отдельный протокол Model Context Protocol для управления аккаунтом (баланс, ключи, организации), не для генерации контента.
- **Веб-интерфейс:** консоль `polza.ai/dashboard`; каталог моделей `polza.ai/models`.
- **Поддержка:** Telegram-чат Polza AI.

### Коды ответов HTTP
| Код | Значение |
|---|---|
| 200 | Успех |
| 201 | Задача создана (асинхронные операции, напр. генерация видео) |
| 400 | Неверный запрос |
| 401 | Ошибка аутентификации |
| 402 | Недостаточно средств на балансе |
| 403 | Доступ запрещён |
| 404 | Ресурс не найден |
| 408 | Таймаут |
| 429 | Превышен лимит запросов |
| 500 | Ошибка сервера |
| 502 | Провайдер недоступен |
| 503 | Нет доступных провайдеров |

Формат ошибки: `{"error": {"code": "...", "message": "..."}}`.

## 3. Быстрый старт

1. Регистрация на `polza.ai/dashboard`.
2. Пополнение баланса.
3. Создание API-ключа в разделе API Keys.
4. Запрос через OpenAI SDK (Python/TS) или curl, указав `base_url`/`baseURL` = `https://polza.ai/api/v1` и свой ключ.

Пример модели по умолчанию в доке: `openai/gpt-4o`. ID моделей строятся по схеме `<провайдер>/<модель>`, например: `anthropic/claude-3-5-sonnet`, `google/gemini-2.5-pro-preview`, `meta-llama/llama-3.3-70b-instruct`.

Список моделей можно получить программно: `GET https://polza.ai/api/v1/models` (простой список) или через `GET /models/catalog` (каталог с фильтрацией и пагинацией).

## 4. Chat Completions API (основной эндпоинт)

`POST https://polza.ai/api/v1/chat/completions` — принимает диалог, возвращает ответ модели.

### Основные параметры запроса
- `model` (string, обязательный) — ID модели.
- `messages` (array) **или** `prompt` (string) — одно из двух обязательно; `prompt` автоматически оборачивается в `messages` с ролью `user`.
- `temperature` (0.0–2.0, по умолчанию 1.0).
- `max_tokens` / `max_completion_tokens`.
- `stream` (bool) — потоковая передача через SSE.
- `top_p`, `frequency_penalty` (-2..2), `presence_penalty` (-2..2).
- `response_format` — структурированный вывод (JSON-схема).
- `tools`, `tool_choice` (`none`/`auto`/`required`) — вызов функций (tool calling).
- `reasoning` — настройки reasoning-токенов.
- `web_search_options` — встроенный веб-поиск для любой модели.
- `provider` — управление маршрутизацией (см. ниже).
- `user` — ID конечного пользователя.

### Роли сообщений
`system` (контекст/поведение), `developer` (аналог system для некоторых моделей), `user`, `assistant` (может содержать `tool_calls`), `tool` (результат вызова инструмента, требует `tool_call_id`).

`content` может быть строкой или массивом content-parts для мультимодального ввода (текст + `image_url` и т.п.) — так передаются изображения/документы/аудио/видео на вход (гайд «Передача медиа на вход»).

### Структура ответа
```json
{
  "id": "gen_...",
  "object": "chat.completion",
  "created": 1703001234,
  "model": "openai/gpt-4o",
  "provider": "openai-direct",
  "choices": [{"index": 0, "message": {"role": "assistant", "content": "..."}, "finish_reason": "stop"}],
  "usage": {
    "prompt_tokens": 25, "completion_tokens": 20, "total_tokens": 45,
    "cost_rub": 0.15, "cost": 0.15
  }
}
```
Поле `provider` в ответе показывает, кто именно обработал запрос. `finish_reason`: `stop`, `length`, `tool_calls`, `content_filter`.

При стриминге (`stream: true`) ответ приходит как SSE-чанки с `choices[].delta.content`; финальный usage приходит в последнем сообщении перед `[DONE]`.

Есть также **Responses API** (`POST /v1/responses`) — OpenAI-совместимый формат с tool calling и streaming, альтернатива Chat Completions.

## 5. Выбор провайдера (routing)

Одна и та же модель может обслуживаться разными провайдерами (разная цена/скорость/доступность). По умолчанию Polza.ai выбирает провайдера автоматически (рекомендуется для продакшна).

Ручное управление — объект `provider` в теле запроса:
| Поле | Описание |
|---|---|
| `order` | приоритетный список провайдеров (fallback по порядку) |
| `only` | белый список |
| `ignore` | чёрный список |
| `sort` | `price` / `latency` / `throughput` |
| `max_price` | лимиты цены: `prompt`, `completion`, `image`, `audio`, `request` (в рублях, `prompt`/`completion` — за млн токенов) |
| `allow_fallbacks` | разрешить переключение на другого провайдера при сбое |

### Alias-синтаксис прямо в строке `model` (для SDK без поддержки доп. полей в body)
Формат: `<model>@<key>=<value>&<key>=<value>`. Поддерживаются: `provider` (эквивалент `provider.only`), `reasoning_effort` (`low`/`medium`/`high` и т.п.), `allow_fallbacks` (`true`/`false`).
Пример: `anthropic/claude-opus-4-6@provider=Amazon Bedrock`, `minimax/minimax-m2.5@reasoning_effort=high&allow_fallbacks=false`.
Работает только для `/v1/chat/completions`, `/v1/responses`, `/v1/messages` (не для embeddings/audio/media). Если параметр задан одновременно и в alias, и в body — вернётся ошибка 400.

## 6. Прочие возможности API (по разделам документации)

- **Tool Calling** — вызов функций моделью (`tools`, `tool_choice`).
- **Structured Output** — гарантированный JSON-формат ответа (`response_format`).
- **Плагины** — расширение возможностей моделей.
- **Web Search** — добавление веб-поиска к любой модели через `web_search_options`.
- **Reasoning Tokens** — управление «токенами рассуждений» (для моделей типа o1/o3, DeepSeek R1 и т.п.); они тарифицируются отдельно и дороже обычных (~4x).
- **Кеширование промптов (Prompt Caching)** — снижает расходы на повторяющиеся части промпта до ~90%; в usage отражается как `cached_tokens`.
- **RAG** — эмбеддинги + поиск + генерация через один ключ.
- **OAuth 2.0 PKCE** — авторизация конечных пользователей и выдача им API-ключей (для встраивания в сторонние приложения).
- **Media API** — единый эндпоинт `POST /v1/media` для генерации изображений/видео/аудио (асинхронно: создание задачи → `GET /media/status` → результат), плюс операции над медиа (`extend`, `upscale`) через `POST /media/operations`.
- **Images Generations** — OpenAI-совместимый эндпоинт `POST /v1/images/generations`.
- **Audio** — `POST /v1/audio/transcriptions` (речь → текст) и `POST /v1/audio/speech` (текст → речь, TTS).
- **Embeddings** — `POST /v1/embeddings`.
- **Файловое хранилище** — загрузка (`POST /storage/upload`), список файлов, получение инфо, удаление, «сохранить навсегда» (постоянное хранение), статистика хранилища.
- **История генераций** — `GET /history/generations` с фильтрацией/пагинацией (только метаданные, без самих промптов).
- **Баланс** — `GET /v1/balance` (простой ответ `{"amount": "1250.50"}`) и `GET /balance-v2` (баланс организации + доступная к трате сумма).

### Поддерживаемые модели генерации медиа (примеры из доки, список пополняется)
Видео: Gemini Omni Video, Seedance 1.5 Pro / 2 / 2 Fast / 2 Mini, Kling 3.0 (+Motion Control), Kling 2.6 Motion Control, Kling 2.5 Turbo, Wan 2.5/2.6, Veo 3.1, Topaz Upscale (апскейл).
Изображения: Seedream 3.0/4/4.5/5.0 Lite, Flux-2 Pro/Flex, Grok Imagine, Qwen Image, GPT Image 1.5, GPT-5 Image / GPT-5 Image Mini / GPT-5.4 Image 2, Nano Banana / Pro / 2.
Аудио/речь: ElevenLabs TTS Turbo / Multilingual, Aiesa (асинхронная транскрипция с диаризацией и LLM-анализом).
Музыка: Suno Music Generate, Google Lyria 3 (Pro/Clip).

## 7. Usage Accounting (учёт расходов)

Каждый ответ API содержит объект `usage` с точной стоимостью в рублях (`cost_rub`, алиас `cost`), уже списанной с баланса:
```json
"usage": {
  "prompt_tokens": 150, "completion_tokens": 250, "total_tokens": 400,
  "cost_rub": 15.75, "cost": 15.75,
  "prompt_tokens_details": {"cached_tokens": 100, "audio_tokens": 10, "video_tokens": 5},
  "completion_tokens_details": {"reasoning_tokens": 50, "audio_tokens": 10, "image_tokens": 5}
}
```
Типы токенов по стоимости: Prompt (базовая цена) < Completion (в 2–4 раза дороже prompt) < Reasoning (~4x дороже обычных); Cached — примерно на 90% дешевле обычных prompt-токенов.
В стриминге usage приходит в последнем SSE-чанке перед `[DONE]`.

## 8. MCP-сервер (для AI-агентов, управление аккаунтом)

Отдельно от генеративного API есть MCP-сервер (Model Context Protocol, сейчас в бете), позволяющий AI-агентам (Claude Code, Cursor, Cline, Windsurf и др.) управлять аккаунтом Polza.ai напрямую: смотреть баланс, управлять API-ключами, организациями, историей генераций.

- URL: `https://polza.ai/api/mcp`, транспорт — HTTP (Streamable HTTP).
- Авторизация отдельным MCP-токеном (создаётся в `polza.ai/dashboard/mcp`, показывается один раз).
- Токен имеет **scopes** (разрешения) вида `ресурс.уровень`: `profile.read/write`, `orgs.read/write/danger`, `keys.read/write/danger`, `billing.read/write/danger`, `history.read`. Пресеты: «Только чтение», «Разработчик», «Полный (без опасных)».
- Группы инструментов: Профиль (`get_profile`, `get_balance`, `list_organizations`...), Организации, API-ключи, Биллинг (`get_balance_details`, `create_topup_link`...), История (`list_generations`, `get_generation` — только метаданные).
- Rate limit: операции записи — 60 запросов/мин; «опасные» операции — 5 запросов/10 мин.
- Подключение в Claude Code: `claude mcp add --transport http polza-ai https://polza.ai/api/mcp --header "Authorization: Bearer <токен>"`.
- Подключение в Cursor: файл `.cursor/mcp.json` с `url` и `headers.Authorization`.

Важно: MCP-сервер — это управление аккаунтом (баланс/ключи/история), а НЕ способ генерировать текст/изображения — для этого используется обычный REST API.

## 9. Интеграции (готовые гайды в документации)

Claude Code, Claude Desktop, Cline (VS Code/Cursor) + Cline Desktop, Codex CLI, ChatGPT Desktop, DeepSeek Harness, Dify, Gemini CLI, Hermes Agent, Kilo Code, LangChain, N8N, OpenClaw, OpenCode, Polza IDE (alpha, собственная десктопная IDE), Polza.AI Proxy (локальный прокси для IDE/CLI с поддержкой выбора провайдера), Roo Code, Qwen Code CLI.

Общий принцип интеграции — почти везде используется тот же приём: подставить `base_url`/endpoint `https://polza.ai/api/v1` и свой Polza-ключ в конфиг инструмента (который изначально рассчитан на прямой OpenAI/Anthropic API).

## 10. Прочее

- **Конфиденциальность** — есть отдельная страница о том, как обрабатываются и защищаются данные пользователей.
- **Для бизнеса (Enterprise)** — отдельные условия: SLA, выделенная поддержка, гибкая оплата, закрывающие документы для юрлиц/ИП.
- **Для провайдеров** — есть страница о том, как стать провайдером моделей на платформе (т.е. Polza.ai подключает и сторонние провайдеры моделей).
- **FAQ** — отдельная страница с частыми вопросами.
- OpenAPI-спецификация полного API доступна по адресу `https://polza.ai/api/openapi.json`.

---

### Итог одной фразой
Polza.ai — это российский мультимодельный AI-агрегатор с единым OpenAI-совместимым API (base_url `https://polza.ai/api/v1`, ключ через `Authorization: Bearer`), дающий доступ к 400+ моделям текста/изображений/видео/аудио/эмбеддингов от разных провайдеров с единым рублёвым биллингом, гибкой маршрутизацией между провайдерами (`provider.order/only/ignore/sort/max_price`), встроенным учётом расходов в каждом ответе (`usage.cost_rub`), поддержкой tool calling, structured output, reasoning, кеширования промптов, веб-поиска, RAG, файлового хранилища и отдельным MCP-сервером для управления аккаунтом через AI-агентов.
