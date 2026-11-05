//! Публичное API Mojang: поиск UUID по нику, профиль с текстурами, загрузка скина и плаща.
//! Ничего, кроме серверов Mojang/Minecraft, не используется.

use anyhow::{anyhow, bail, Result};
use base64::Engine;
use serde::Deserialize;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

use crate::http::{read_limited, Http};

const CACHE_TTL: Duration = Duration::from_secs(60);
const CACHE_MAX: usize = 500;
const TEXTURE_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub skin_url: Option<String>,
    pub slim: bool,
    pub cape_url: Option<String>,
}

/// Ник Minecraft Java: 3–16 символов, латиница, цифры, «_».
pub fn valid_name(s: &str) -> bool {
    (3..=16).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// UUID в виде 32 hex-символов; допускаются дефисы. Возвращает нормализованный вид.
pub fn normalize_uuid(s: &str) -> Option<String> {
    let cleaned: String = s.chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
    (cleaned.len() == 32 && cleaned.bytes().all(|b| b.is_ascii_hexdigit())).then_some(cleaned)
}

#[derive(Deserialize)]
struct NameLookup {
    id: String,
}

#[derive(Deserialize)]
struct SessionProfile {
    id: String,
    name: String,
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize, Default)]
struct TexturesPayload {
    #[serde(default)]
    textures: Textures,
}

#[derive(Deserialize, Default)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<Texture>,
    #[serde(rename = "CAPE")]
    cape: Option<Texture>,
}

#[derive(Deserialize)]
struct Texture {
    url: String,
    #[serde(default)]
    metadata: Option<TextureMeta>,
}

#[derive(Deserialize)]
struct TextureMeta {
    model: Option<String>,
}

/// Разбор ответа `sessionserver.mojang.com/session/minecraft/profile/{uuid}`.
pub fn parse_session_profile(body: &[u8]) -> Result<Profile> {
    let raw: SessionProfile = serde_json::from_slice(body)?;
    let mut profile = Profile {
        id: raw.id,
        name: raw.name,
        skin_url: None,
        slim: false,
        cape_url: None,
    };
    if let Some(prop) = raw.properties.iter().find(|p| p.name == "textures") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(prop.value.as_bytes())
            .map_err(|e| anyhow!("bad textures property: {e}"))?;
        let payload: TexturesPayload = serde_json::from_slice(&decoded)?;
        if let Some(skin) = payload.textures.skin {
            profile.slim = skin
                .metadata
                .and_then(|m| m.model)
                .map(|m| m == "slim")
                .unwrap_or(false);
            profile.skin_url = Some(skin.url);
        }
        profile.cape_url = payload.textures.cape.map(|c| c.url);
    }
    Ok(profile)
}

pub struct Mojang {
    http: Http,
    cache: Mutex<HashMap<String, (Instant, Profile)>>,
}

impl Mojang {
    pub fn new(http: Http) -> Mojang {
        Mojang {
            http,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// UUID по нику. `None`: такого игрока нет.
    pub async fn uuid_by_name(&self, name: &str) -> Result<Option<String>> {
        if !valid_name(name) {
            bail!("invalid player name");
        }
        let resp = self
            .http
            .get(&format!("https://api.mojang.com/users/profiles/minecraft/{name}"))
            .await?;
        match resp.status().as_u16() {
            200 => {
                let body = read_limited(resp, 64 * 1024).await?;
                let found: NameLookup = serde_json::from_slice(&body)?;
                Ok(normalize_uuid(&found.id))
            }
            204 | 404 => Ok(None),
            429 => bail!("Mojang rate limit"),
            other => bail!("unexpected Mojang status {other}"),
        }
    }

    /// Профиль с текстурами по UUID. Кэшируется в памяти на минуту.
    pub async fn profile(&self, uuid: &str) -> Result<Option<Profile>> {
        let uuid = normalize_uuid(uuid).ok_or_else(|| anyhow!("invalid uuid"))?;
        {
            let cache = self.cache.lock().await;
            if let Some((at, p)) = cache.get(&uuid) {
                if at.elapsed() < CACHE_TTL {
                    return Ok(Some(p.clone()));
                }
            }
        }
        let resp = self
            .http
            .get(&format!(
                "https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"
            ))
            .await?;
        let profile = match resp.status().as_u16() {
            200 => {
                let body = read_limited(resp, 256 * 1024).await?;
                parse_session_profile(&body)?
            }
            204 | 404 => return Ok(None),
            429 => bail!("Mojang rate limit"),
            other => bail!("unexpected Mojang status {other}"),
        };
        let mut cache = self.cache.lock().await;
        if cache.len() >= CACHE_MAX {
            cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
        }
        cache.insert(uuid, (Instant::now(), profile.clone()));
        Ok(Some(profile))
    }

    /// Скачивает PNG-текстуру. Разрешён только `textures.minecraft.net`.
    pub async fn texture(&self, url: &str) -> Result<Vec<u8>> {
        let mut parsed = reqwest::Url::parse(url).map_err(|e| anyhow!("invalid texture url: {e}"))?;
        if parsed.host_str() != Some("textures.minecraft.net") {
            bail!("texture host is not textures.minecraft.net");
        }
        // Mojang отдаёт ссылки с http://, но сервер поддерживает и https.
        parsed
            .set_scheme("https")
            .map_err(|_| anyhow!("cannot switch texture url to https"))?;
        let resp = self.http.get_any(parsed.as_str()).await?;
        if !resp.status().is_success() {
            bail!("texture request failed with {}", resp.status());
        }
        read_limited(resp, TEXTURE_LIMIT).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nickname_validation() {
        assert!(valid_name("Notch"));
        assert!(valid_name("a_b"));
        assert!(!valid_name("ab"));
        assert!(!valid_name("seventeen_chars_xx"));
        assert!(!valid_name("bad name"));
        assert!(!valid_name("../etc"));
        assert!(!valid_name("ник"));
    }

    #[test]
    fn uuid_normalization() {
        let plain = "069a79f444e94726a5befca90e38aaf5";
        assert_eq!(normalize_uuid(plain).as_deref(), Some(plain));
        assert_eq!(
            normalize_uuid("069A79F4-44E9-4726-A5BE-FCA90E38AAF5").as_deref(),
            Some(plain)
        );
        assert_eq!(normalize_uuid("short"), None);
        assert_eq!(normalize_uuid(&"z".repeat(32)), None);
    }

    #[test]
    fn parses_textures_property() {
        let payload = r#"{"timestamp":1,"profileId":"069a79f444e94726a5befca90e38aaf5","profileName":"Notch","textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc","metadata":{"model":"slim"}},"CAPE":{"url":"http://textures.minecraft.net/texture/def"}}}"#;
        let b64 = base64::engine::general_purpose::STANDARD.encode(payload);
        let body = format!(
            r#"{{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[{{"name":"textures","value":"{b64}"}}]}}"#
        );
        let p = parse_session_profile(body.as_bytes()).unwrap();
        assert_eq!(p.name, "Notch");
        assert!(p.slim);
        assert_eq!(p.skin_url.as_deref(), Some("http://textures.minecraft.net/texture/abc"));
        assert_eq!(p.cape_url.as_deref(), Some("http://textures.minecraft.net/texture/def"));
    }

    #[test]
    fn profile_without_textures_is_fine() {
        let body = br#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#;
        let p = parse_session_profile(body).unwrap();
        assert!(p.skin_url.is_none() && p.cape_url.is_none() && !p.slim);
    }
}
