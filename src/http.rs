//! Единственная точка выхода в сеть.
//!
//! Каждый запрос (и каждый редирект) проходит проверку по белому списку
//! из `allowlist.rs`, только HTTPS. Сторонние сервисы недоступны физически.

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{redirect, Client, Response, Url};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::allowlist::is_allowed_host;

const USER_AGENT: &str = concat!(
    "minecraft-version-bot/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/TRizun3457/A-bot-for-notifications-about-new-Minecraft-versions)"
);

/// Максимальный размер ответа: манифест версий весит сотни КБ, всё остальное меньше.
pub const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct Http {
    client: Client,
}

impl Http {
    pub fn new() -> Result<Http> {
        let policy = redirect::Policy::custom(|attempt| {
            let too_many = attempt.previous().len() >= 5;
            let allowed =
                attempt.url().scheme() == "https" && attempt.url().host_str().map(is_allowed_host).unwrap_or(false);
            if too_many {
                attempt.error("too many redirects")
            } else if allowed {
                attempt.follow()
            } else {
                attempt.error("redirect target is not on the allowlist")
            }
        });
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .redirect(policy)
            .https_only(true)
            .build()
            .context("failed to build HTTP client")?;
        Ok(Http { client })
    }

    /// Разбирает URL и проверяет схему и хост по белому списку.
    pub fn check_url(url: &str) -> Result<Url> {
        let parsed = Url::parse(url).map_err(|e| anyhow!("invalid url: {e}"))?;
        if parsed.scheme() != "https" {
            bail!("only https urls are allowed");
        }
        let host = parsed.host_str().ok_or_else(|| anyhow!("url has no host"))?;
        if !is_allowed_host(host) {
            bail!("host {host} is not on the allowlist");
        }
        Ok(parsed)
    }

    pub async fn get(&self, url: &str) -> Result<Response> {
        let url = Self::check_url(url)?;
        send(self.client.get(url).header("Accept", "application/json")).await
    }

    /// GET без заголовка `Accept: application/json` (для PNG-текстур).
    pub async fn get_any(&self, url: &str) -> Result<Response> {
        let url = Self::check_url(url)?;
        send(self.client.get(url).header("Accept", "*/*")).await
    }

    pub async fn get_bearer(&self, url: &str, token: &str) -> Result<Response> {
        let url = Self::check_url(url)?;
        send(
            self.client
                .get(url)
                .header("Accept", "application/json")
                .bearer_auth(token),
        )
        .await
    }

    pub async fn post_json<B: Serialize + ?Sized>(&self, url: &str, body: &B) -> Result<Response> {
        let url = Self::check_url(url)?;
        send(self.client.post(url).header("Accept", "application/json").json(body)).await
    }

    pub async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<Response> {
        let url = Self::check_url(url)?;
        send(self.client.post(url).header("Accept", "application/json").form(form)).await
    }
}

/// Отправляет запрос. Из текста ошибки убирается URL: в нём может быть ник игрока,
/// а бот не должен оставлять такие данные даже в логах.
async fn send(req: reqwest::RequestBuilder) -> Result<Response> {
    req.send().await.map_err(|e| anyhow::Error::new(e.without_url()))
}

/// Читает тело ответа, не допуская превышения `limit` байт.
pub async fn read_limited(mut resp: Response, limit: usize) -> Result<Vec<u8>> {
    if let Some(len) = resp.content_length() {
        if len > limit as u64 {
            bail!("response is too large");
        }
    }
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| anyhow::Error::new(e.without_url()))? {
        if buf.len() + chunk.len() > limit {
            bail!("response is too large");
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Проверяет успешный статус и разбирает JSON.
pub async fn json_ok<T: DeserializeOwned>(resp: Response) -> Result<T> {
    let status = resp.status();
    if !status.is_success() {
        bail!("unexpected HTTP status {status}");
    }
    let body = read_limited(resp, MAX_BODY).await?;
    serde_json::from_slice(&body).context("failed to parse JSON response")
}
