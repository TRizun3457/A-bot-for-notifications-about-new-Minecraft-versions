# Бот уведомлений о новых версиях Minecraft

Telegram-бот на Rust ([teloxide, форк lapismyt](https://github.com/lapismyt/teloxide)). Интерфейс на русском и английском.

## Что умеет

- **Уведомления в личные сообщения**, как только выходит новая версия: Java (релизы и снапшоты), Bedrock (релизы и Beta/Preview). В сообщении ссылка на список изменений. Что именно присылать, настраивается в `/settings`.
- `/version` — актуальные версии Java и Bedrock со ссылками.
- `/search <запрос>` — поиск версии и ссылки на её список изменений (`/search 1.21.4`, `/search 26.60`).
- `/link` — привязка аккаунта Minecraft (Microsoft/Xbox) через официальный вход Microsoft (device code): пароль бот не видит, токены не сохраняются.
- `/profile`, `/skin [ник]`, `/cape [ник]` — профиль, скин и активный плащ (свой или по нику).
- `/language`, `/privacy`, `/delete` — язык, что хранится, удалить все данные.

> **Про «gramio».** [GramIO](https://github.com/gramiojs/gramio) — библиотека для TypeScript/JavaScript, в Rust её использовать нельзя. Весь бот написан на teloxide.

## Откуда берутся данные (и больше ниоткуда)

Все HTTP-запросы идут через один модуль `src/http.rs`, который пропускает только HTTPS и только домены из `src/allowlist.rs` (включая каждый редирект):

| Что | Источник |
| --- | --- |
| Версии Java | `launchermeta.mojang.com` (манифест версий), ссылка на `minecraft.wiki` |
| Версии и changelog Bedrock | `feedback.minecraft.net` (Help Center API) |
| UUID, профиль, скин, плащ | `api.mojang.com`, `sessionserver.mojang.com`, `textures.minecraft.net` |
| Привязка аккаунта | `login.microsoftonline.com`, `*.xboxlive.com`, `api.minecraftservices.com` |

Чужие данные не собираются. Хранится только: Telegram ID, язык, подписки, UUID привязанного профиля и «последняя виденная версия» (файл `data/state.json`). Подробности — команда `/privacy`.

## Ограничения

- **Игровой статистики нет.** У Mojang/Microsoft нет открытого API со статистикой игрока (убийства, время игры и т. п.), а брать её у сторонних сервисов проект запрещает. Поэтому `/profile` показывает то, что Mojang реально отдаёт: ник, UUID, модель скина, наличие плаща.
- Плащи: бот показывает плащ Mojang. Плащи OptiFine, LabyMod и т. п. у Mojang не хранятся, поэтому к `/skin` и `/cape` добавлена кнопка «Открыть на NameMC» (`ru.namemc.com` для русского, `namemc.com` для английского). Это только ссылка: бот NameMC не опрашивает и страницы не разбирает, страницу открывает сам пользователь.
- Разрешение Mojang нужно **только для `/link`** (вход через `login_with_xbox`). Поиск скина, плаща и профиля по нику идёт через открытые `api.mojang.com` и `sessionserver.mojang.com` и разрешения не требует.
- Ссылки на изменения Java ведут на Minecraft Wiki (`Java_Edition_<версия>`): официальный slug статьи на minecraft.net нельзя вычислить по номеру версии.

## Запуск

```bash
cp .env.example .env     # заполните TELOXIDE_TOKEN и MS_CLIENT_ID
cargo run --release
```

`.env` бот сам не читает: экспортируйте переменные (`set -a; . ./.env; set +a`) или задайте их в systemd/Docker.

### Привязка аккаунта (`/link`): нужно приложение Microsoft

1. [Azure Portal](https://portal.azure.com) → Microsoft Entra ID → App registrations → New registration. Тип аккаунтов: **Personal Microsoft accounts only** (или «Any … and personal»).
2. Authentication → **Allow public client flows = Yes** (нужно для device code).
3. Скопируйте Application (client) ID в `MS_CLIENT_ID`.
4. Minecraft API принимает только одобренные приложения: подайте заявку Mojang (форма «Minecraft API app review» на aka.ms/mce-reviewappid). До одобрения `/link` отвечает, что приложение не одобрено; остальные команды работают без этого.

### Необязательные переменные

| Переменная | Значение |
| --- | --- |
| `DATA_DIR` | каталог для `state.json` (по умолчанию `data`) |
| `POLL_INTERVAL_SECS` | период проверки версий, 60–86400 (по умолчанию 300) |
| `BEDROCK_RELEASE_SECTION_ID`, `BEDROCK_PREVIEW_SECTION_ID` | ID разделов feedback.minecraft.net, если автоопределение по названию не сработало (сейчас: `360001186971` и `360001185332`) |

## Первый запуск и рассылка

Первую проверку бот только запоминает текущие версии и **ничего не рассылает**. Уведомления приходят о версиях, вышедших позже. После долгого простоя за один цикл уходит не больше 3 последних версий на канал.

## Разработка

```bash
cargo test
cargo clippy --all-targets
cargo fmt --check
```

## Лицензия

MIT, см. [LICENSE](LICENSE).
