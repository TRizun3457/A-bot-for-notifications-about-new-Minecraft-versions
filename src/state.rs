use std::collections::HashSet;
use tokio::sync::Mutex;

use crate::{http::Http, mojang::Mojang, store::Store, versions::Versions};

/// Общее состояние бота, раздаётся хендлерам через `Arc`.
pub struct AppState {
    pub http: Http,
    pub store: Store,
    pub versions: Versions,
    pub mojang: Mojang,
    /// Client ID приложения Azure для входа через Microsoft. `None`: /link отключена.
    pub ms_client_id: Option<String>,
    /// Чаты, в которых прямо сейчас идёт привязка аккаунта (по одной на чат).
    pub linking: Mutex<HashSet<i64>>,
}
