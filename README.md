# Rustman

Лёгкий HTTP-клиент в стиле Postman на Rust (egui + reqwest).

- Несколько вкладок (Cmd+T — новая, Cmd+W — закрыть, ПКМ — дублировать)
- Query-строка URL ↔ таблица Params синхронизируются в обе стороны
- Headers, Body: raw JSON / text / x-www-form-urlencoded
- Импорт cURL (кнопка или просто вставить `curl ...` в поле URL) и экспорт (`</>`)
- Ответ: Pretty JSON с подсветкой и номерами строк, Raw, дерево (Tree) с копированием значений и путей
- Сворачивание `{ }` / `[ ]` блоков стрелками у номеров строк, «Свернуть всё / Развернуть всё»
- Поиск по ответу (Cmd+F): подсветка совпадений, Enter / Shift+Enter — следующее / предыдущее, учёт регистра; свёрнутые блоки с найденным раскрываются сами
- Тёмная и светлая темы, вкладки сохраняются между запусками
- Cmd+Enter — отправить запрос

## Установка

Скачайте сборку для своей системы на странице **Releases** этого репозитория.

**macOS** (`Rustman-macos-universal.zip`, Apple Silicon и Intel):
распакуйте и перенесите `Rustman.app` в «Программы». Приложение не подписано,
поэтому при первом запуске: ПКМ по приложению → «Открыть» → «Открыть», либо

```bash
xattr -dr com.apple.quarantine /Applications/Rustman.app
```

**Windows** (`Rustman-windows-x64.zip`): распакуйте и запустите `rustman.exe`.
SmartScreen может предупредить — «Подробнее» → «Выполнить в любом случае».

**Linux** (`Rustman-linux-x64.tar.gz`): распакуйте и запустите `./rustman`.

## Выпуск новой версии

Сборки запускаются только при пуше в `main`. Чтобы выпустить релиз, поднимите
`version` в `Cargo.toml` и запушьте в `main`: GitHub Actions соберёт все
платформы и опубликует релиз `v<version>`. Если релиз с такой версией уже есть,
сборка пройдёт, но ничего не опубликуется.

## Сборка из исходников (macOS / Linux / Windows)

Нужен Rust (https://rustup.rs или `brew install rust`).

```bash
cargo run --release
```

На macOS `./scripts/bundle.sh` собирает `target/Rustman.app`.

## Тесты

```bash
cargo test
```

## Лицензия

[MIT](LICENSE)
