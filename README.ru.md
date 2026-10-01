<p align="center">
  <img src="c.svg" width="96" height="96" alt="Логотип Typr" />
</p>

<h1 align="center">Typr</h1>

<p align="center">
  <a href="README.md">English</a> · <b>Русский</b>
</p>

Typr — лёгкое приложение для голосового ввода на Windows. Нажимаешь глобальный хоткей, говоришь, и распознанный текст вставляется в активное окно.

## Возможности

- **Глобальный хоткей** в двух режимах.
- **Провайдеры распознавания:** Groq (Whisper Large v3/v3 Turbo), [Polza.AI](https://polza.ai/), [AssemblyAI](https://www.assemblyai.com/) (Universal-3.5 Pro/Universal-2), а также любой OpenAI-совместимый эндпоинт.
- **Чистый буфер обмена:** продиктованный текст вставляется через буфер, не попадает в журнал буфера Windows.
- **Пробел после текста**, чтобы следующая фраза не слипалась с предыдущей.
- **Индикатор записи:** маленький микрофон выезжает из-за верхнего края экрана только во время диктовки (запись, распознавание, вставка) и снова прячется.
- **Минимум памяти:** около 5 МБ ОЗУ в трее! WebView окна настроек выгружается после закрытия, а индикатор записи рисуется нативно.
- **Режим разработчика:** живой лог всего, что делает приложение, для поиска проблем.
- **Интерфейс на русском и английском.**
- Тёмный минималистичный интерфейс в стиле Anthropic.

## Сборка

Установщик собирает GitHub Actions (`.github/workflows/build.yml`) при пуше тега вида `v*` или при ручном запуске workflow:

```bash
git tag v1.1.1
git push origin v1.1.1
```

Для локальной разработки нужны [Rust](https://rustup.rs/) и [Node.js](https://nodejs.org/) 20+:

```bash
npm install
npm run tauri dev
```

Сделано на [Tauri 2](https://tauri.app/) (Rust) и чистом TypeScript.

## Авторы

- Оригинальный проект — [albertshiney](https://github.com/albertshiney/typr)
- Форк ведёт [neodym121](https://github.com/neodym121), доработан вместе с [Claude](https://claude.com/claude-code) (Anthropic)

## Лицензия

[MIT](LICENSE)
