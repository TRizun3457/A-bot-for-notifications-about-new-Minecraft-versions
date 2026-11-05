use std::{path::PathBuf, time::Duration};

/// Настройки из переменных окружения.
///
/// * `TELOXIDE_TOKEN`: токен бота (читается самим teloxide).
/// * `MS_CLIENT_ID`: Application (client) ID приложения в Azure для входа через Microsoft.
///   Без него команда /link отключена.
/// * `DATA_DIR`: куда писать `state.json` (по умолчанию `./data`).
/// * `POLL_INTERVAL_SECS`: как часто проверять новые версии (60..86400, по умолчанию 300).
/// * `BEDROCK_RELEASE_SECTION_ID` / `BEDROCK_PREVIEW_SECTION_ID`: необязательно, ID разделов
///   feedback.minecraft.net, если автоопределение не нашло нужные.
pub struct Config {
    pub ms_client_id: Option<String>,
    pub data_dir: PathBuf,
    pub poll_interval: Duration,
    pub bedrock_release_section: Option<u64>,
    pub bedrock_preview_section: Option<u64>,
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Config {
    pub fn from_env() -> Config {
        let secs = env_nonempty("POLL_INTERVAL_SECS")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300)
            .clamp(60, 86_400);
        Config {
            ms_client_id: env_nonempty("MS_CLIENT_ID"),
            data_dir: PathBuf::from(env_nonempty("DATA_DIR").unwrap_or_else(|| "data".to_string())),
            poll_interval: Duration::from_secs(secs),
            bedrock_release_section: env_nonempty("BEDROCK_RELEASE_SECTION_ID").and_then(|v| v.parse().ok()),
            bedrock_preview_section: env_nonempty("BEDROCK_PREVIEW_SECTION_ID").and_then(|v| v.parse().ok()),
        }
    }
}
