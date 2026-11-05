//! Привязка аккаунта Minecraft через вход Microsoft (OAuth 2.0 Device Code Flow).
//!
//! Пользователь вводит код на сайте Microsoft сам: бот не видит пароль.
//! Цепочка: Microsoft token → Xbox Live → XSTS → Minecraft services → профиль.
//! Запрашивается только scope `XboxLive.signin` (без `offline_access`), поэтому
//! refresh-токена нет, а access-токены живут в памяти считанные секунды
//! и нигде не сохраняются и не логируются.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::http::{json_ok, read_limited, Http, MAX_BODY};

const DEVICE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

fn default_interval() -> u64 {
    5
}

#[derive(Deserialize, Debug)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

pub enum Poll {
    Pending,
    SlowDown,
    Declined,
    Expired,
    Token(String),
}

/// Результат привязки. Хранится только `id`; остальное нужно для ответа пользователю.
#[derive(Debug)]
pub struct McAccount {
    pub id: String,
    pub name: String,
    pub capes: usize,
}

#[derive(Debug)]
pub enum LinkError {
    /// У Microsoft-аккаунта нет профиля Xbox.
    NoXbox,
    /// Детский аккаунт без семейной группы.
    Child,
    /// Xbox Live недоступен в стране аккаунта.
    RegionBlocked,
    /// Mojang не одобрил приложение для Minecraft API.
    NotApproved,
    /// Нет профиля Minecraft: Java Edition.
    NoProfile,
    Other(anyhow::Error),
}

fn other<E: Into<anyhow::Error>>(e: E) -> LinkError {
    LinkError::Other(e.into())
}

pub async fn start(http: &Http, client_id: &str) -> Result<DeviceCode> {
    let resp = http
        .post_form(DEVICE_URL, &[("client_id", client_id), ("scope", "XboxLive.signin")])
        .await?;
    json_ok(resp).await
}

pub async fn poll(http: &Http, client_id: &str, device_code: &str) -> Result<Poll> {
    let resp = http
        .post_form(
            TOKEN_URL,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", device_code),
            ],
        )
        .await?;
    // Пока пользователь не ввёл код, endpoint отвечает 400 с описанием ошибки: статус не смотрим.
    let body = read_limited(resp, MAX_BODY).await?;
    let v: Value = serde_json::from_slice(&body)?;
    if let Some(token) = v.get("access_token").and_then(Value::as_str) {
        return Ok(Poll::Token(token.to_string()));
    }
    match v.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => Ok(Poll::Pending),
        Some("slow_down") => Ok(Poll::SlowDown),
        Some("authorization_declined") => Ok(Poll::Declined),
        Some("expired_token") | Some("bad_verification_code") => Ok(Poll::Expired),
        Some(other) => Err(anyhow!("token endpoint error: {other}")),
        None => Err(anyhow!("unexpected token endpoint response")),
    }
}

fn xui_hash(v: &Value) -> Option<&str> {
    v.get("DisplayClaims")?.get("xui")?.get(0)?.get("uhs")?.as_str()
}

fn map_xsts_error(code: u64) -> LinkError {
    match code {
        2148916233 => LinkError::NoXbox,
        2148916235 => LinkError::RegionBlocked,
        2148916238 => LinkError::Child,
        other => LinkError::Other(anyhow!("XSTS error {other}")),
    }
}

/// По access-токену Microsoft получает профиль Minecraft Java.
pub async fn minecraft_account(http: &Http, ms_token: &str) -> Result<McAccount, LinkError> {
    // 1. Xbox Live user token
    let resp = http
        .post_json(
            XBL_URL,
            &json!({
                "Properties": {
                    "AuthMethod": "RPS",
                    "SiteName": "user.auth.xboxlive.com",
                    "RpsTicket": format!("d={ms_token}")
                },
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT"
            }),
        )
        .await
        .map_err(other)?;
    let xbl: Value = json_ok(resp).await.map_err(other)?;
    let xbl_token = xbl
        .get("Token")
        .and_then(Value::as_str)
        .ok_or_else(|| other(anyhow!("XBL response has no token")))?
        .to_string();

    // 2. XSTS token для Minecraft services
    let resp = http
        .post_json(
            XSTS_URL,
            &json!({
                "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT"
            }),
        )
        .await
        .map_err(other)?;
    if resp.status().as_u16() == 401 {
        let body = read_limited(resp, MAX_BODY).await.map_err(other)?;
        let v: Value = serde_json::from_slice(&body).map_err(other)?;
        return Err(map_xsts_error(v.get("XErr").and_then(Value::as_u64).unwrap_or(0)));
    }
    let xsts: Value = json_ok(resp).await.map_err(other)?;
    let xsts_token = xsts
        .get("Token")
        .and_then(Value::as_str)
        .ok_or_else(|| other(anyhow!("XSTS response has no token")))?;
    let uhs = xui_hash(&xsts).ok_or_else(|| other(anyhow!("XSTS response has no user hash")))?;

    // 3. Токен Minecraft services
    let resp = http
        .post_json(
            MC_LOGIN_URL,
            &json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }),
        )
        .await
        .map_err(other)?;
    if resp.status().as_u16() == 403 {
        return Err(LinkError::NotApproved);
    }
    let login: Value = json_ok(resp).await.map_err(other)?;
    let mc_token = login
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| other(anyhow!("Minecraft login response has no token")))?;

    // 4. Профиль
    let resp = http.get_bearer(MC_PROFILE_URL, mc_token).await.map_err(other)?;
    if resp.status().as_u16() == 404 {
        return Err(LinkError::NoProfile);
    }
    let profile: Value = json_ok(resp).await.map_err(other)?;
    let id = profile
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| other(anyhow!("profile has no id")))?;
    let name = profile
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| other(anyhow!("profile has no name")))?;
    let capes = profile
        .get("capes")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    Ok(McAccount {
        id: id.to_string(),
        name: name.to_string(),
        capes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xsts_errors_map_to_user_facing_variants() {
        assert!(matches!(map_xsts_error(2148916233), LinkError::NoXbox));
        assert!(matches!(map_xsts_error(2148916238), LinkError::Child));
        assert!(matches!(map_xsts_error(2148916235), LinkError::RegionBlocked));
        assert!(matches!(map_xsts_error(1), LinkError::Other(_)));
    }

    #[test]
    fn extracts_user_hash() {
        let v: Value = serde_json::from_str(r#"{"Token":"t","DisplayClaims":{"xui":[{"uhs":"12345"}]}}"#).unwrap();
        assert_eq!(xui_hash(&v), Some("12345"));
        let empty: Value = serde_json::from_str(r#"{"DisplayClaims":{"xui":[]}}"#).unwrap();
        assert_eq!(xui_hash(&empty), None);
    }

    #[test]
    fn device_code_defaults_interval() {
        let d: DeviceCode = serde_json::from_str(
            r#"{"device_code":"d","user_code":"U","verification_uri":"https://www.microsoft.com/link","expires_in":900}"#,
        )
        .unwrap();
        assert_eq!(d.interval, 5);
    }
}
