//! Версии Minecraft.
//!
//! * Java: официальный манифест версий Mojang (`launchermeta.mojang.com`).
//!   Ссылка на список изменений ведёт на Minecraft Wiki.
//! * Bedrock: Help Center API сайта `feedback.minecraft.net` (разделы со списками изменений
//!   релизов и Beta/Preview). Ссылка берётся прямо из ответа сайта.

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

use crate::{
    http::{json_ok, Http},
    vparse::{extract_version, pick_sections},
};

const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const FEEDBACK_API: &str = "https://feedback.minecraft.net/api/v2/help_center";
const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JavaKind {
    Release,
    Snapshot,
}

#[derive(Clone, Debug)]
pub struct JavaVersion {
    pub id: String,
    pub kind: JavaKind,
}

impl JavaVersion {
    /// Страница версии на Minecraft Wiki (`Java_Edition_1.21.4`, `Java_Edition_25w02a`).
    pub fn changelog_url(&self) -> String {
        let safe: String = self
            .id
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("https://minecraft.wiki/w/Java_Edition_{safe}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BedrockKind {
    Release,
    Preview,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Article {
    pub id: u64,
    pub title: String,
    pub html_url: String,
    #[serde(default)]
    pub section_id: Option<u64>,
}

impl Article {
    /// Ссылка на статью. Если сайт вернул что-то неожиданное, строим стандартную.
    pub fn url(&self) -> String {
        if Http::check_url(&self.html_url).is_ok() {
            self.html_url.clone()
        } else {
            format!("https://feedback.minecraft.net/hc/en-us/articles/{}", self.id)
        }
    }

    pub fn version(&self) -> Option<String> {
        extract_version(&self.title)
    }
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<ManifestEntry>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct ArticlesResp {
    #[serde(default)]
    articles: Vec<Article>,
}

#[derive(Deserialize)]
struct SearchResp {
    #[serde(default)]
    results: Vec<Article>,
}

#[derive(Deserialize)]
struct SectionsResp {
    #[serde(default)]
    sections: Vec<Section>,
    #[serde(default)]
    next_page: Option<String>,
}

#[derive(Deserialize)]
struct Section {
    id: u64,
    name: String,
}

pub struct SearchOutput {
    pub java: Vec<JavaVersion>,
    pub bedrock: Vec<Article>,
}

type Timed<T> = Option<(Instant, Arc<T>)>;

pub struct Versions {
    http: Http,
    release_override: Option<u64>,
    preview_override: Option<u64>,
    sections: Mutex<Option<(Option<u64>, Option<u64>)>>,
    java_cache: Mutex<Timed<Vec<JavaVersion>>>,
    bedrock_cache: Mutex<HashMap<BedrockKind, (Instant, Arc<Vec<Article>>)>>,
}

impl Versions {
    pub fn new(http: Http, release_override: Option<u64>, preview_override: Option<u64>) -> Versions {
        Versions {
            http,
            release_override,
            preview_override,
            sections: Mutex::new(None),
            java_cache: Mutex::new(None),
            bedrock_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Все версии Java (релизы и снапшоты), новые первыми, как в манифесте Mojang.
    pub async fn java(&self) -> Result<Arc<Vec<JavaVersion>>> {
        let mut cache = self.java_cache.lock().await;
        if let Some((at, list)) = cache.as_ref() {
            if at.elapsed() < CACHE_TTL {
                return Ok(list.clone());
            }
        }
        let manifest: Manifest = json_ok(self.http.get(MANIFEST_URL).await?).await?;
        let list: Vec<JavaVersion> = manifest
            .versions
            .into_iter()
            .filter_map(|v| {
                let kind = match v.kind.as_str() {
                    "release" => JavaKind::Release,
                    "snapshot" => JavaKind::Snapshot,
                    _ => return None, // old_alpha / old_beta не интересны
                };
                Some(JavaVersion { id: v.id, kind })
            })
            .collect();
        if list.is_empty() {
            bail!("version manifest has no usable versions");
        }
        let list = Arc::new(list);
        *cache = Some((Instant::now(), list.clone()));
        Ok(list)
    }

    /// ID разделов Bedrock: из переменных окружения или автоопределением по названиям.
    async fn section_ids(&self) -> Result<(Option<u64>, Option<u64>)> {
        if self.release_override.is_some() && self.preview_override.is_some() {
            return Ok((self.release_override, self.preview_override));
        }
        let mut cached = self.sections.lock().await;
        if cached.is_none() {
            let mut all: Vec<(u64, String)> = Vec::new();
            let mut next = Some(format!("{FEEDBACK_API}/en-us/sections.json?per_page=100"));
            let mut pages = 0;
            while let Some(url) = next.take() {
                pages += 1;
                let resp: SectionsResp = json_ok(self.http.get(&url).await?).await?;
                all.extend(resp.sections.into_iter().map(|s| (s.id, s.name)));
                if pages < 5 {
                    next = resp.next_page;
                }
            }
            let (release, preview) = pick_sections(&all);
            // Кэшируем только удачное определение, иначе повторим при следующем вызове.
            if release.is_some() && preview.is_some() {
                *cached = Some((release, preview));
            } else {
                return Ok((self.release_override.or(release), self.preview_override.or(preview)));
            }
        }
        let (release, preview) = cached.expect("filled above");
        Ok((self.release_override.or(release), self.preview_override.or(preview)))
    }

    /// Списки изменений Bedrock, новые первыми.
    pub async fn bedrock(&self, kind: BedrockKind) -> Result<Arc<Vec<Article>>> {
        {
            let cache = self.bedrock_cache.lock().await;
            if let Some((at, list)) = cache.get(&kind) {
                if at.elapsed() < CACHE_TTL {
                    return Ok(list.clone());
                }
            }
        }
        let (release, preview) = self.section_ids().await?;
        let section = match kind {
            BedrockKind::Release => release,
            BedrockKind::Preview => preview,
        }
        .ok_or_else(|| anyhow!("Bedrock changelog section not found on feedback.minecraft.net"))?;

        let url = format!(
            "{FEEDBACK_API}/en-us/sections/{section}/articles.json?per_page=30&sort_by=created_at&sort_order=desc"
        );
        let resp: ArticlesResp = json_ok(self.http.get(&url).await?).await?;
        // В разделах бывают и служебные статьи без номера версии: пропускаем их.
        let list: Vec<Article> = resp.articles.into_iter().filter(|a| a.version().is_some()).collect();
        if list.is_empty() {
            bail!("no Bedrock articles with a version in section {section}");
        }
        let list = Arc::new(list);
        self.bedrock_cache
            .lock()
            .await
            .insert(kind, (Instant::now(), list.clone()));
        Ok(list)
    }

    /// Поиск версии по строке. Java ищется в манифесте, Bedrock в поиске Help Center.
    /// Ошибка одного источника не мешает показать результаты другого.
    pub async fn search(&self, query: &str) -> Result<SearchOutput> {
        let query: String = query.chars().filter(|c| !c.is_control()).take(64).collect();
        let query = query.trim();
        if query.is_empty() {
            return Ok(SearchOutput {
                java: Vec::new(),
                bedrock: Vec::new(),
            });
        }

        let java = self.java().await;
        let bedrock = self.search_bedrock(query).await;
        if java.is_err() && bedrock.is_err() {
            return Err(anyhow!("both version sources are unavailable"));
        }

        let needle = query.to_lowercase();
        let mut java_hits: Vec<JavaVersion> = java
            .map(|list| {
                list.iter()
                    .filter(|v| v.id.to_lowercase().contains(&needle))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        // Точное совпадение первым (sort_by_key стабилен, порядок манифеста сохраняется).
        java_hits.sort_by_key(|v| v.id.to_lowercase() != needle);
        java_hits.truncate(6);

        Ok(SearchOutput {
            java: java_hits,
            bedrock: bedrock.unwrap_or_default(),
        })
    }

    async fn search_bedrock(&self, query: &str) -> Result<Vec<Article>> {
        // Разделы нужны для фильтра по спискам изменений; ошибку определения не считаем
        // фатальной: тогда просто не фильтруем.
        let _ = self.section_ids().await;
        let url = reqwest::Url::parse_with_params(
            &format!("{FEEDBACK_API}/articles/search.json"),
            &[("query", query), ("locale", "en-us"), ("per_page", "20")],
        )?;
        let resp: SearchResp = json_ok(self.http.get(url.as_str()).await?).await?;

        // Если разделы известны, оставляем только списки изменений Bedrock.
        let known = {
            let cached = self.sections.lock().await;
            cached
                .map(|(r, p)| [r, p])
                .unwrap_or([self.release_override, self.preview_override])
        };
        let filter_by_section = known.iter().any(Option::is_some);

        let mut hits: Vec<Article> = resp
            .results
            .into_iter()
            .filter(|a| a.version().is_some())
            .filter(|a| !filter_by_section || a.section_id.map(|s| known.contains(&Some(s))).unwrap_or(false))
            .collect();
        hits.truncate(6);
        Ok(hits)
    }
}
