mod allowlist;
mod config;
mod handlers;
mod http;
mod i18n;
mod microsoft;
mod mojang;
mod notifier;
mod render;
mod state;
mod store;
mod versions;
mod vparse;

use std::{collections::HashSet, sync::Arc};
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio::sync::Mutex;

use crate::{
    config::Config,
    handlers::{ru_commands, schema, Command},
    http::Http,
    mojang::Mojang,
    state::AppState,
    store::Store,
    versions::Versions,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // По умолчанию логируем только своё и предупреждения teloxide; содержимое обновлений не пишем.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("minecraft_version_bot=info,teloxide=warn"),
    )
    .init();

    let config = Config::from_env();
    let bot = Bot::from_env(); // токен из TELOXIDE_TOKEN

    let http = Http::new()?;
    let state = Arc::new(AppState {
        store: Store::open(config.data_dir.join("state.json")).await?,
        versions: Versions::new(
            http.clone(),
            config.bedrock_release_section,
            config.bedrock_preview_section,
        ),
        mojang: Mojang::new(http.clone()),
        http,
        ms_client_id: config.ms_client_id.clone(),
        linking: Mutex::new(HashSet::new()),
    });

    if state.ms_client_id.is_none() {
        log::warn!("MS_CLIENT_ID is not set: /link is disabled");
    }

    if let Err(e) = bot.set_my_commands(Command::bot_commands()).await {
        log::warn!("cannot set default command menu: {e}");
    }
    if let Err(e) = bot.set_my_commands(ru_commands()).language_code("ru").await {
        log::warn!("cannot set Russian command menu: {e}");
    }

    tokio::spawn(notifier::run(bot.clone(), state.clone(), config.poll_interval));

    log::info!("bot started");
    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![state])
        // Необработанные апдейты не логируем: в них лежит пользовательский контент.
        .default_handler(|_update| async {})
        .error_handler(LoggingErrorHandler::with_custom_text("error in update handler"))
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}
