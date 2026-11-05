//! Минимальное хранилище в одном JSON-файле.
//!
//! Хранится только то, без чего бот не работает: Telegram ID, язык, подписки,
//! UUID привязанного профиля Minecraft и «последняя виденная версия» по каналам.
//! Токены Microsoft/Xbox здесь не появляются никогда.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use tokio::sync::Mutex;

pub const CHANNELS: [&str; 4] = ["java_release", "java_snapshot", "bedrock_release", "bedrock_preview"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Subs {
    pub java_release: bool,
    pub java_snapshot: bool,
    pub bedrock_release: bool,
    pub bedrock_preview: bool,
}

impl Default for Subs {
    fn default() -> Self {
        Subs {
            java_release: true,
            java_snapshot: true,
            bedrock_release: true,
            bedrock_preview: true,
        }
    }
}

impl Subs {
    pub fn get(&self, key: &str) -> bool {
        match key {
            "java_release" => self.java_release,
            "java_snapshot" => self.java_snapshot,
            "bedrock_release" => self.bedrock_release,
            "bedrock_preview" => self.bedrock_preview,
            _ => false,
        }
    }

    /// Переключает канал и возвращает новое значение. Неизвестный ключ ничего не меняет.
    pub fn toggle(&mut self, key: &str) -> bool {
        let slot = match key {
            "java_release" => &mut self.java_release,
            "java_snapshot" => &mut self.java_snapshot,
            "bedrock_release" => &mut self.bedrock_release,
            "bedrock_preview" => &mut self.bedrock_preview,
            _ => return false,
        };
        *slot = !*slot;
        *slot
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserRec {
    pub lang: String,
    pub subs: Subs,
    #[serde(default)]
    pub mc_uuid: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Data {
    #[serde(default)]
    users: HashMap<i64, UserRec>,
    #[serde(default)]
    seen: HashMap<String, String>,
}

pub struct Store {
    path: PathBuf,
    data: Mutex<Data>,
}

impl Store {
    pub async fn open(path: PathBuf) -> Result<Store> {
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .with_context(|| format!("cannot create data dir {}", dir.display()))?;
        }
        let data = match tokio::fs::read(&path).await {
            Ok(bytes) => match serde_json::from_slice::<Data>(&bytes) {
                Ok(d) => d,
                // Не затираем повреждённый файл молча: пусть владелец разберётся.
                Err(e) => bail!("state file {} is corrupted: {e}", path.display()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        Ok(Store {
            path,
            data: Mutex::new(data),
        })
    }

    /// Атомарная запись: во временный файл, затем rename.
    async fn persist(&self, data: &Data) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(data)?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, bytes).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).await;
        }
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(())
    }

    pub async fn user(&self, id: i64) -> Option<UserRec> {
        self.data.lock().await.users.get(&id).cloned()
    }

    /// Возвращает запись пользователя, создавая её (все подписки включены) при первом обращении.
    pub async fn ensure_user(&self, id: i64, default_lang: &str) -> Result<UserRec> {
        let mut data = self.data.lock().await;
        if let Some(u) = data.users.get(&id) {
            return Ok(u.clone());
        }
        let rec = UserRec {
            lang: default_lang.to_string(),
            subs: Subs::default(),
            mc_uuid: None,
        };
        data.users.insert(id, rec.clone());
        self.persist(&data).await?;
        Ok(rec)
    }

    /// Применяет изменение к существующей записи. `None`: пользователя нет.
    pub async fn update_user<R>(&self, id: i64, f: impl FnOnce(&mut UserRec) -> R) -> Result<Option<R>> {
        let mut data = self.data.lock().await;
        let Some(user) = data.users.get_mut(&id) else {
            return Ok(None);
        };
        let out = f(user);
        self.persist(&data).await?;
        Ok(Some(out))
    }

    pub async fn delete_user(&self, id: i64) -> Result<bool> {
        let mut data = self.data.lock().await;
        let existed = data.users.remove(&id).is_some();
        if existed {
            self.persist(&data).await?;
        }
        Ok(existed)
    }

    /// Подписчики канала: (chat_id, язык).
    pub async fn subscribers(&self, channel: &str) -> Vec<(i64, String)> {
        self.data
            .lock()
            .await
            .users
            .iter()
            .filter(|(_, u)| u.subs.get(channel))
            .map(|(id, u)| (*id, u.lang.clone()))
            .collect()
    }

    pub async fn seen(&self, channel: &str) -> Option<String> {
        self.data.lock().await.seen.get(channel).cloned()
    }

    pub async fn set_seen(&self, channel: &str, id: &str) -> Result<()> {
        let mut data = self.data.lock().await;
        data.seen.insert(channel.to_string(), id.to_string());
        self.persist(&data).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("mcbot-test-{}-{name}", std::process::id()));
        p.push("state.json");
        p
    }

    #[tokio::test]
    async fn users_subscriptions_and_seen_survive_reopen() {
        let path = tmp_path("roundtrip");
        let store = Store::open(path.clone()).await.unwrap();
        store.ensure_user(42, "ru").await.unwrap();
        store.update_user(42, |u| u.subs.toggle("java_snapshot")).await.unwrap();
        store
            .update_user(42, |u| u.mc_uuid = Some("a".repeat(32)))
            .await
            .unwrap();
        store.set_seen("java_release", "1.21.4").await.unwrap();

        let reopened = Store::open(path.clone()).await.unwrap();
        let u = reopened.user(42).await.unwrap();
        assert_eq!(u.lang, "ru");
        assert!(!u.subs.java_snapshot && u.subs.java_release);
        assert_eq!(u.mc_uuid.as_deref(), Some("a".repeat(32).as_str()));
        assert_eq!(reopened.seen("java_release").await.as_deref(), Some("1.21.4"));
        assert_eq!(reopened.subscribers("java_snapshot").await.len(), 0);
        assert_eq!(
            reopened.subscribers("bedrock_preview").await,
            vec![(42, "ru".to_string())]
        );

        assert!(reopened.delete_user(42).await.unwrap());
        assert!(reopened.user(42).await.is_none());
        let _ = tokio::fs::remove_dir_all(path.parent().unwrap()).await;
    }

    #[tokio::test]
    async fn corrupted_state_is_not_overwritten() {
        let path = tmp_path("corrupt");
        tokio::fs::create_dir_all(path.parent().unwrap()).await.unwrap();
        tokio::fs::write(&path, b"{not json").await.unwrap();
        assert!(Store::open(path.clone()).await.is_err());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"{not json");
        let _ = tokio::fs::remove_dir_all(path.parent().unwrap()).await;
    }

    #[test]
    fn toggle_ignores_unknown_channel() {
        let mut s = Subs::default();
        assert!(!s.toggle("nope"));
        assert!(CHANNELS.iter().all(|k| s.get(k)));
    }
}
