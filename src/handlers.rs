//! Команды бота и обработка нажатий на inline-кнопки.

use anyhow::Result;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use teloxide::{
    dispatching::UpdateHandler,
    prelude::*,
    types::{BotCommand, InlineKeyboardButton, InlineKeyboardMarkup, InputFile},
    utils::command::BotCommands,
    RequestError,
};

use crate::{
    http::Http,
    i18n::{channel_label, namemc_url, t, Lang, M},
    microsoft::{self, LinkError},
    mojang::{normalize_uuid, valid_name, Profile},
    render,
    state::AppState,
    store::{Subs, CHANNELS},
    versions::{BedrockKind, JavaKind},
};

#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "Commands:")]
pub enum Command {
    #[command(description = "start the bot")]
    Start,
    #[command(description = "show help")]
    Help,
    #[command(description = "latest Java and Bedrock versions")]
    Version,
    #[command(description = "find a version, e.g. /search 1.21.4")]
    Search(String),
    #[command(description = "link your Minecraft account")]
    Link,
    #[command(description = "unlink your account")]
    Unlink,
    #[command(description = "your Minecraft profile")]
    Profile,
    #[command(description = "skin: /skin [name]")]
    Skin(String),
    #[command(description = "cape: /cape [name]")]
    Cape(String),
    #[command(description = "choose notifications")]
    Settings,
    #[command(description = "bot language")]
    Language,
    #[command(description = "what data is stored")]
    Privacy,
    #[command(description = "delete all my data")]
    Delete,
}

/// Русские описания для меню команд (для пользователей с языком Telegram `ru`).
pub fn ru_commands() -> Vec<BotCommand> {
    vec![
        BotCommand::new("start", "запустить бота"),
        BotCommand::new("help", "справка"),
        BotCommand::new("version", "актуальные версии Java и Bedrock"),
        BotCommand::new("search", "найти версию, например /search 1.21.4"),
        BotCommand::new("link", "привязать аккаунт Minecraft"),
        BotCommand::new("unlink", "отвязать аккаунт"),
        BotCommand::new("profile", "ваш профиль Minecraft"),
        BotCommand::new("skin", "скин: /skin [ник]"),
        BotCommand::new("cape", "плащ: /cape [ник]"),
        BotCommand::new("settings", "настроить уведомления"),
        BotCommand::new("language", "язык бота"),
        BotCommand::new("privacy", "какие данные хранятся"),
        BotCommand::new("delete", "удалить мои данные"),
    ]
}

pub fn schema() -> UpdateHandler<RequestError> {
    dptree::entry()
        .branch(
            Update::filter_message()
                .filter(|msg: Message| msg.chat.is_private())
                .filter_command::<Command>()
                .endpoint(on_command),
        )
        .branch(Update::filter_callback_query().endpoint(on_callback))
}

async fn on_command(bot: Bot, msg: Message, cmd: Command, st: Arc<AppState>) -> ResponseResult<()> {
    let chat = msg.chat.id;
    let tg_code = msg.from.as_ref().and_then(|u| u.language_code.clone());
    let lang = match st.store.user(chat.0).await {
        Some(user) => Lang::parse(&user.lang),
        None => Lang::from_telegram(tg_code.as_deref()),
    };
    if let Err(e) = run_command(&bot, &st, chat, lang, cmd).await {
        // Саму команду (в ней может быть ник) в лог не пишем.
        log::warn!("command failed: {e:#}");
        let _ = bot.send_message(chat, t(lang, M::NetworkError)).await;
    }
    Ok(())
}

async fn say(bot: &Bot, chat: ChatId, text: String) -> Result<()> {
    bot.send_message(chat, text).await?;
    Ok(())
}

async fn run_command(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang, cmd: Command) -> Result<()> {
    match cmd {
        Command::Start => {
            st.store.ensure_user(chat.0, lang.code()).await?;
            say(bot, chat, t(lang, M::Welcome)).await
        }
        Command::Help => say(bot, chat, t(lang, M::Help)).await,
        Command::Privacy => say(bot, chat, t(lang, M::Privacy)).await,
        Command::Version => cmd_version(bot, st, chat, lang).await,
        Command::Search(query) => cmd_search(bot, st, chat, lang, &query).await,
        Command::Link => cmd_link(bot, st, chat, lang).await,
        Command::Unlink => {
            let was_linked = st.store.update_user(chat.0, |u| u.mc_uuid.take()).await?;
            let text = match was_linked {
                Some(Some(_)) => t(lang, M::Unlinked),
                _ => t(lang, M::NotLinked),
            };
            say(bot, chat, text).await
        }
        Command::Profile => cmd_profile(bot, st, chat, lang).await,
        Command::Skin(arg) => cmd_skin(bot, st, chat, lang, &arg).await,
        Command::Cape(arg) => cmd_cape(bot, st, chat, lang, &arg).await,
        Command::Settings => {
            let user = st.store.ensure_user(chat.0, lang.code()).await?;
            bot.send_message(chat, t(lang, M::SettingsTitle))
                .reply_markup(settings_kb(lang, &user.subs))
                .await?;
            Ok(())
        }
        Command::Language => {
            st.store.ensure_user(chat.0, lang.code()).await?;
            let kb = InlineKeyboardMarkup::new(vec![vec![
                InlineKeyboardButton::callback("Русский", "l:ru"),
                InlineKeyboardButton::callback("English", "l:en"),
            ]]);
            bot.send_message(chat, t(lang, M::LanguageTitle))
                .reply_markup(kb)
                .await?;
            Ok(())
        }
        Command::Delete => {
            st.store.delete_user(chat.0).await?;
            say(bot, chat, t(lang, M::DeletedAll)).await
        }
    }
}

// ---------- версии ----------

async fn cmd_version(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang) -> Result<()> {
    let (java, release, preview) = tokio::join!(
        st.versions.java(),
        st.versions.bedrock(BedrockKind::Release),
        st.versions.bedrock(BedrockKind::Preview)
    );

    let mut parts = vec![t(lang, M::VersionsTitle)];

    match java {
        Ok(list) => {
            for (label, kind) in [("Java", JavaKind::Release), ("Java snapshot", JavaKind::Snapshot)] {
                match list.iter().find(|v| v.kind == kind) {
                    Some(v) => parts.push(t(
                        lang,
                        M::VersionLine {
                            label,
                            value: &v.id,
                            url: &v.changelog_url(),
                        },
                    )),
                    None => parts.push(t(lang, M::VersionUnavailable { label })),
                }
            }
        }
        Err(e) => {
            log::warn!("java versions unavailable: {e:#}");
            parts.push(t(lang, M::VersionUnavailable { label: "Java" }));
        }
    }

    for (label, result) in [("Bedrock", release), ("Bedrock Beta/Preview", preview)] {
        match result {
            Ok(list) => {
                // bedrock() гарантирует непустой список.
                let article = &list[0];
                let value = article.version().unwrap_or_else(|| article.title.clone());
                parts.push(t(
                    lang,
                    M::VersionLine {
                        label,
                        value: &value,
                        url: &article.url(),
                    },
                ));
            }
            Err(e) => {
                log::warn!("bedrock versions unavailable: {e:#}");
                parts.push(t(lang, M::VersionUnavailable { label }));
            }
        }
    }

    say(bot, chat, parts.join("\n\n")).await
}

async fn cmd_search(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang, query: &str) -> Result<()> {
    let query: String = query.trim().chars().filter(|c| !c.is_control()).take(64).collect();
    if query.is_empty() {
        return say(bot, chat, t(lang, M::SearchUsage)).await;
    }
    let found = match st.versions.search(&query).await {
        Ok(found) => found,
        Err(e) => {
            log::warn!("search failed: {e:#}");
            return say(bot, chat, t(lang, M::NetworkError)).await;
        }
    };
    if found.java.is_empty() && found.bedrock.is_empty() {
        return say(bot, chat, t(lang, M::SearchNone { query: &query })).await;
    }
    let mut lines = vec![t(lang, M::SearchTitle { query: &query })];
    for v in &found.java {
        lines.push(format!("Java {}: {}", v.id, v.changelog_url()));
    }
    for a in &found.bedrock {
        lines.push(format!(
            "Bedrock {}: {}",
            a.version().unwrap_or_else(|| a.title.clone()),
            a.url()
        ));
    }
    say(bot, chat, lines.join("\n")).await
}

// ---------- привязка аккаунта ----------

async fn cmd_link(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang) -> Result<()> {
    let Some(client_id) = st.ms_client_id.clone() else {
        return say(bot, chat, t(lang, M::LinkNotConfigured)).await;
    };
    if !st.linking.lock().await.insert(chat.0) {
        return say(bot, chat, t(lang, M::LinkBusy)).await;
    }
    // Ожидание кода может занять минуты, поэтому уходим в отдельную задачу,
    // чтобы не блокировать обработку других команд этого чата.
    let (bot, st) = (bot.clone(), st.clone());
    tokio::spawn(async move {
        let outcome = link_flow(&bot, &st, chat, lang, &client_id).await;
        st.linking.lock().await.remove(&chat.0);
        if let Err(e) = outcome {
            log::warn!("link flow failed: {e:#}");
            let _ = bot.send_message(chat, t(lang, M::LinkFailed)).await;
        }
    });
    Ok(())
}

async fn link_flow(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang, client_id: &str) -> Result<()> {
    let code = microsoft::start(&st.http, client_id).await?;

    // Адрес приходит от Microsoft, но всё равно показываем только проверенный.
    let uri = if Http::check_url(&code.verification_uri).is_ok() {
        code.verification_uri.clone()
    } else {
        "https://www.microsoft.com/link".to_string()
    };
    say(
        bot,
        chat,
        t(
            lang,
            M::LinkInstructions {
                url: &uri,
                code: &code.user_code,
                minutes: (code.expires_in / 60).max(1),
            },
        ),
    )
    .await?;

    let deadline = Instant::now() + Duration::from_secs(code.expires_in);
    let mut interval = code.interval.clamp(5, 30);
    let mut failures = 0u32;
    let ms_token = loop {
        if Instant::now() >= deadline {
            return say(bot, chat, t(lang, M::LinkExpired)).await;
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
        match microsoft::poll(&st.http, client_id, &code.device_code).await {
            Ok(microsoft::Poll::Pending) => failures = 0,
            Ok(microsoft::Poll::SlowDown) => {
                interval = (interval + 5).min(60);
                failures = 0;
            }
            Ok(microsoft::Poll::Declined) => return say(bot, chat, t(lang, M::LinkDeclined)).await,
            Ok(microsoft::Poll::Expired) => return say(bot, chat, t(lang, M::LinkExpired)).await,
            Ok(microsoft::Poll::Token(token)) => break token,
            Err(e) => {
                failures += 1;
                log::warn!("device code poll failed ({failures}): {e:#}");
                if failures >= 5 {
                    return Err(e);
                }
            }
        }
    };

    let message = match microsoft::minecraft_account(&st.http, &ms_token).await {
        Ok(account) => match normalize_uuid(&account.id) {
            Some(uuid) => {
                st.store.ensure_user(chat.0, lang.code()).await?;
                st.store.update_user(chat.0, |u| u.mc_uuid = Some(uuid)).await?;
                t(
                    lang,
                    M::LinkSuccess {
                        name: &account.name,
                        capes: account.capes,
                    },
                )
            }
            None => {
                log::warn!("Minecraft returned a malformed profile id");
                t(lang, M::LinkFailed)
            }
        },
        Err(LinkError::NoXbox) => t(lang, M::LinkNoXbox),
        Err(LinkError::Child) => t(lang, M::LinkChild),
        Err(LinkError::RegionBlocked) => t(lang, M::LinkRegionBlocked),
        Err(LinkError::NotApproved) => {
            log::warn!("Mojang rejected the app registration (403 on login_with_xbox)");
            t(lang, M::LinkNotApproved)
        }
        Err(LinkError::NoProfile) => t(lang, M::LinkNoProfile),
        Err(LinkError::Other(e)) => {
            log::warn!("account linking failed: {e:#}");
            t(lang, M::LinkFailed)
        }
    };
    say(bot, chat, message).await
}

// ---------- профиль, скин, плащ ----------

/// Определяет, чей профиль нужен, и загружает его. Если что-то не так, сама пишет
/// пользователю причину и возвращает `None`.
///
/// Пустой аргумент означает привязанный аккаунт. Иначе это ник или UUID: данные
/// запрашиваются на лету у Mojang и нигде не сохраняются.
async fn target_profile(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang, arg: &str) -> Result<Option<Profile>> {
    let arg = arg.split_whitespace().next().unwrap_or("");

    let uuid = if arg.is_empty() {
        match st.store.user(chat.0).await.and_then(|u| u.mc_uuid) {
            Some(uuid) => uuid,
            None => {
                say(bot, chat, t(lang, M::NotLinked)).await?;
                return Ok(None);
            }
        }
    } else if let Some(uuid) = normalize_uuid(arg) {
        uuid
    } else if valid_name(arg) {
        match st.mojang.uuid_by_name(arg).await {
            Ok(Some(uuid)) => uuid,
            Ok(None) => {
                say(bot, chat, t(lang, M::NickNotFound)).await?;
                return Ok(None);
            }
            Err(e) => {
                log::warn!("name lookup failed: {e:#}");
                say(bot, chat, t(lang, M::NetworkError)).await?;
                return Ok(None);
            }
        }
    } else {
        say(bot, chat, t(lang, M::NickInvalid)).await?;
        return Ok(None);
    };

    match st.mojang.profile(&uuid).await {
        Ok(Some(profile)) => Ok(Some(profile)),
        Ok(None) => {
            say(bot, chat, t(lang, M::NickNotFound)).await?;
            Ok(None)
        }
        Err(e) => {
            log::warn!("profile lookup failed: {e:#}");
            say(bot, chat, t(lang, M::NetworkError)).await?;
            Ok(None)
        }
    }
}

async fn cmd_profile(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang) -> Result<()> {
    let Some(profile) = target_profile(bot, st, chat, lang, "").await? else {
        return Ok(());
    };
    say(
        bot,
        chat,
        t(
            lang,
            M::Profile {
                name: &profile.name,
                uuid: &profile.id,
                slim: profile.slim,
                has_cape: profile.cape_url.is_some(),
            },
        ),
    )
    .await
}

async fn cmd_skin(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang, arg: &str) -> Result<()> {
    let Some(profile) = target_profile(bot, st, chat, lang, arg).await? else {
        return Ok(());
    };
    let Some(url) = profile.skin_url.as_deref() else {
        let text = t(lang, M::NoSkin { name: &profile.name });
        return say_with_namemc(bot, chat, lang, &profile.name, text).await;
    };
    let skin = match st.mojang.texture(url).await {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!("skin download failed: {e:#}");
            return say(bot, chat, t(lang, M::NetworkError)).await;
        }
    };
    let caption = t(
        lang,
        M::SkinCaption {
            name: &profile.name,
            slim: profile.slim,
        },
    );
    send_preview_and_file(
        bot,
        chat,
        render::face_preview(&skin),
        skin,
        "skin",
        &profile.name,
        caption,
        namemc_kb(lang, &profile.name),
    )
    .await
}

async fn cmd_cape(bot: &Bot, st: &Arc<AppState>, chat: ChatId, lang: Lang, arg: &str) -> Result<()> {
    let Some(profile) = target_profile(bot, st, chat, lang, arg).await? else {
        return Ok(());
    };
    let Some(url) = profile.cape_url.as_deref() else {
        let text = t(lang, M::NoCape { name: &profile.name });
        return say_with_namemc(bot, chat, lang, &profile.name, text).await;
    };
    let cape = match st.mojang.texture(url).await {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!("cape download failed: {e:#}");
            return say(bot, chat, t(lang, M::NetworkError)).await;
        }
    };
    let caption = t(lang, M::CapeCaption { name: &profile.name });
    send_preview_and_file(
        bot,
        chat,
        render::cape_preview(&cape),
        cape,
        "cape",
        &profile.name,
        caption,
        namemc_kb(lang, &profile.name),
    )
    .await
}

/// Кнопка-ссылка на профиль игрока на NameMC. Бот сам NameMC не опрашивает:
/// страницу открывает пользователь в своём браузере.
fn namemc_kb(lang: Lang, name: &str) -> Option<InlineKeyboardMarkup> {
    let url = url::Url::parse(&namemc_url(lang, name)?).ok()?;
    Some(InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::url(
        t(lang, M::NameMcButton),
        url,
    )]]))
}

/// Текстовое сообщение с кнопкой NameMC (если ник подходит для ссылки).
async fn say_with_namemc(bot: &Bot, chat: ChatId, lang: Lang, name: &str, text: String) -> Result<()> {
    let request = bot.send_message(chat, text);
    match namemc_kb(lang, name) {
        Some(kb) => request.reply_markup(kb).await?,
        None => request.await?,
    };
    Ok(())
}

/// Отправляет превью (если оно получилось) и исходный PNG файлом.
/// Кнопка NameMC, если она есть, цепляется к файлу (последнему сообщению).
#[allow(clippy::too_many_arguments)]
async fn send_preview_and_file(
    bot: &Bot,
    chat: ChatId,
    preview: Result<Vec<u8>>,
    original: Vec<u8>,
    kind: &str,
    name: &str,
    caption: String,
    kb: Option<InlineKeyboardMarkup>,
) -> Result<()> {
    let caption_for_file = match preview {
        Ok(png) => {
            bot.send_photo(chat, InputFile::memory(png).file_name("preview.png"))
                .caption(caption)
                .await?;
            None
        }
        Err(e) => {
            log::warn!("{kind} preview failed: {e:#}");
            Some(caption)
        }
    };
    let mut request = bot.send_document(
        chat,
        InputFile::memory(original).file_name(format!("{name}_{kind}.png")),
    );
    if let Some(caption) = caption_for_file {
        request = request.caption(caption);
    }
    if let Some(kb) = kb {
        request = request.reply_markup(kb);
    }
    request.await?;
    Ok(())
}

// ---------- inline-кнопки ----------

fn settings_kb(lang: Lang, subs: &Subs) -> InlineKeyboardMarkup {
    let rows: Vec<Vec<InlineKeyboardButton>> = CHANNELS
        .iter()
        .map(|key| {
            let mark = if subs.get(key) { "✅" } else { "❌" };
            vec![InlineKeyboardButton::callback(
                format!("{mark} {}", channel_label(lang, key)),
                format!("t:{key}"),
            )]
        })
        .collect();
    InlineKeyboardMarkup::new(rows)
}

async fn on_callback(bot: Bot, q: CallbackQuery, st: Arc<AppState>) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;

    let (Some(data), Some(msg)) = (q.data.as_deref(), q.regular_message()) else {
        return Ok(());
    };
    if !msg.chat.is_private() {
        return Ok(());
    }
    let chat = msg.chat.id;

    if let Some(key) = data.strip_prefix("t:") {
        if !CHANNELS.contains(&key) {
            return Ok(());
        }
        let result = st
            .store
            .update_user(chat.0, |u| {
                u.subs.toggle(key);
                (Lang::parse(&u.lang), u.subs.clone())
            })
            .await;
        match result {
            Ok(Some((lang, subs))) => {
                if let Err(e) = bot
                    .edit_message_reply_markup(chat, msg.id)
                    .reply_markup(settings_kb(lang, &subs))
                    .await
                {
                    log::debug!("cannot refresh settings keyboard: {e}");
                }
            }
            Ok(None) => {}
            Err(e) => log::warn!("cannot update subscriptions: {e:#}"),
        }
    } else if let Some(code) = data.strip_prefix("l:") {
        if code != "ru" && code != "en" {
            return Ok(());
        }
        let lang = Lang::parse(code);
        match st.store.update_user(chat.0, |u| u.lang = lang.code().to_string()).await {
            Ok(Some(())) => {
                bot.send_message(chat, t(lang, M::LanguageSet)).await?;
            }
            Ok(None) => {}
            Err(e) => log::warn!("cannot update language: {e:#}"),
        }
    }
    Ok(())
}
