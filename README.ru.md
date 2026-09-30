<p align="center">
  <img src="c.svg" width="96" height="96" alt="Логотип Typr" />
</p>

<h1 align="center">Typr</h1>

<p align="center">
  <a href="README.md">English</a> · <b>Русский</b>
</p>

Typr — лёгкое приложение для голосового ввода на Windows. Нажимаешь глобальный хоткей, говоришь, и распознанный текст вставляется в активное окно.

## Возможности

- **Глобальный хоткей** в двух режимах: *Toggle* (нажал — запись, нажал ещё раз — стоп) или *Push to Talk* (запись, пока клавиши зажаты)
- **Провайдеры распознавания:** Groq, OpenAI или любой OpenAI-совместимый эндпоинт, а также [Polza.AI](https://polza.ai)
- **Минимум памяти:** около 5 МБ ОЗУ в трее. WebView окна настроек выгружается после закрытия, а индикатор записи рисуется нативно
- **Меню в трее:** включить/выключить хоткей (удобно в играх), открыть настройки, выйти
- **Режим разработчика:** живой лог всего, что делает приложение, для поиска проблем
- Тёмный минималистичный интерфейс

## Установка

Скачай установщик для Windows (`.msi` или `.exe`) из артефакта `typr-windows` последнего успешного запуска во вкладке [Actions](../../actions), затем укажи API-ключ в разделе **Engine**.

## Сборка

Установщик собирает GitHub Actions (`.github/workflows/build.yml`) при пуше тега вида `v*` или при ручном запуске workflow:

```bash
git tag v1.0.4
git push origin v1.0.4
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
