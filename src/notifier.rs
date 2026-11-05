//! Фоновая проверка новых версий и рассылка в личные сообщения.

use std::{sync::Arc, time::Duration};
use teloxide::{prelude::*, ApiError, RequestError};

use crate::{
    i18n::{t, Lang, M},
    state::AppState,
    versions::{BedrockKind, JavaKind},
    vparse::new_since,
};

/// Сколько самых свежих версий разослать за один проверочный цикл.
/// Защита от лавины сообщений после долгого простоя бота.
const MAX_PER_CHANNEL: usize = 3;

/// Пауза между сообщениями: Telegram разрешает порядка 30 сообщений в секунду.
const SEND_GAP: Duration = Duration::from_millis(50);

enum Notice {
    Java {
        id: String,
        snapshot: bool,
        url: String,
    },
    Bedrock {
        article_id: u64,
        title: String,
        preview: bool,
        url: String,
    },
}

impl Notice {
    /// Идентификатор для сравнения с «последней виденной» версией.
    fn id(&self) -> String {
        match self {
            Notice::Java { id, .. } => id.clone(),
            Notice::Bedrock { article_id, .. } => article_id.to_string(),
        }
    }

    fn text(&self, lang: Lang) -> String {
        match self {
            Notice::Java { id, snapshot, url } => t(
                lang,
                M::NewJava {
                    id,
                    snapshot: *snapshot,
                    url,
                },
            ),
            Notice::Bedrock {
                title, preview, url, ..
            } => t(
                lang,
                M::NewBedrock {
                    title,
                    preview: *preview,
                    url,
                },
            ),
        }
    }
}

pub async fn run(bot: Bot, st: Arc<AppState>, every: Duration) {
    log::info!("version watcher started, interval {}s", every.as_secs());
    loop {
        check_all(&bot, &st).await;
        tokio::time::sleep(every).await;
    }
}

async fn check_all(bot: &Bot, st: &AppState) {
    match st.versions.java().await {
        Ok(list) => {
            for (key, kind) in [
                ("java_release", JavaKind::Release),
                ("java_snapshot", JavaKind::Snapshot),
            ] {
                let notices: Vec<Notice> = list
                    .iter()
                    .filter(|v| v.kind == kind)
                    .map(|v| Notice::Java {
                        id: v.id.clone(),
                        snapshot: kind == JavaKind::Snapshot,
                        url: v.changelog_url(),
                    })
                    .collect();
                check_channel(bot, st, key, notices).await;
            }
        }
        Err(e) => log::warn!("java manifest check failed: {e:#}"),
    }

    for (key, kind) in [
        ("bedrock_release", BedrockKind::Release),
        ("bedrock_preview", BedrockKind::Preview),
    ] {
        match st.versions.bedrock(kind).await {
            Ok(list) => {
                let notices: Vec<Notice> = list
                    .iter()
                    .map(|a| Notice::Bedrock {
                        article_id: a.id,
                        // В сообщении нужен номер версии, а не длинный заголовок статьи.
                        title: a.version().unwrap_or_else(|| a.title.clone()),
                        preview: kind == BedrockKind::Preview,
                        url: a.url(),
                    })
                    .collect();
                check_channel(bot, st, key, notices).await;
            }
            Err(e) => log::warn!("bedrock {key} check failed: {e:#}"),
        }
    }
}

async fn check_channel(bot: &Bot, st: &AppState, key: &str, notices: Vec<Notice>) {
    let Some(newest) = notices.first().map(Notice::id) else {
        return;
    };
    let last = st.store.seen(key).await;
    let fresh = new_since(&notices, last.as_deref(), MAX_PER_CHANNEL, Notice::id);

    if last.as_deref() != Some(newest.as_str()) {
        // Сначала фиксируем «виденное», потом рассылаем: при сбое лучше пропустить
        // уведомление, чем слать его повторно при каждой проверке.
        if let Err(e) = st.store.set_seen(key, &newest).await {
            log::warn!("cannot persist state for {key}: {e:#}");
            return;
        }
        if last.is_none() {
            log::info!("channel {key} initialised at {newest}, no notifications sent");
            return;
        }
    }

    for notice in fresh {
        log::info!("new version in {key}: {}", notice.id());
        broadcast(bot, st, key, notice).await;
    }
}

async fn broadcast(bot: &Bot, st: &AppState, key: &str, notice: &Notice) {
    for (chat, lang) in st.store.subscribers(key).await {
        let text = notice.text(Lang::parse(&lang));
        deliver(bot, st, chat, &text).await;
        tokio::time::sleep(SEND_GAP).await;
    }
}

async fn deliver(bot: &Bot, st: &AppState, chat: i64, text: &str) {
    for attempt in 0..2 {
        match bot.send_message(ChatId(chat), text).await {
            Ok(_) => return,
            Err(RequestError::RetryAfter(wait)) if attempt == 0 => {
                tokio::time::sleep(wait.duration()).await;
            }
            Err(RequestError::Api(ApiError::BotBlocked | ApiError::UserDeactivated | ApiError::ChatNotFound)) => {
                // Пользователь заблокировал бота или исчез: не храним о нём ничего.
                if let Err(e) = st.store.delete_user(chat).await {
                    log::warn!("cannot remove unreachable user: {e:#}");
                }
                return;
            }
            Err(e) => {
                log::warn!("cannot deliver notification: {e}");
                return;
            }
        }
    }
}
