//! Тексты бота на русском и английском. Без внешних зависимостей.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    /// Язык по коду из Telegram (`language_code`): русский для `ru*`, иначе английский.
    pub fn from_telegram(code: Option<&str>) -> Lang {
        match code {
            Some(c) if c.to_ascii_lowercase().starts_with("ru") => Lang::Ru,
            _ => Lang::En,
        }
    }

    /// Язык из сохранённого значения (`"ru"` / `"en"`).
    pub fn parse(code: &str) -> Lang {
        if code == "ru" {
            Lang::Ru
        } else {
            Lang::En
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
        }
    }
}

/// Ссылка на профиль игрока на NameMC (для русского языка русская версия сайта).
/// Бот эту страницу не загружает: ссылку открывает сам пользователь.
/// Принимает только корректный ник Java (3–16 символов: латиница, цифры, `_`).
pub fn namemc_url(l: Lang, name: &str) -> Option<String> {
    let ok = (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if !ok {
        return None;
    }
    let host = match l {
        Lang::Ru => "ru.namemc.com",
        Lang::En => "namemc.com",
    };
    Some(format!("https://{host}/profile/{name}"))
}

macro_rules! tr {
    ($l:expr, $ru:literal, $en:literal $(, $a:expr)* $(,)?) => {
        match $l {
            Lang::Ru => format!($ru $(, $a)*),
            Lang::En => format!($en $(, $a)*),
        }
    };
}

/// Подпись кнопки подписки по ключу канала.
pub fn channel_label(l: Lang, key: &str) -> &'static str {
    match (l, key) {
        (Lang::Ru, "java_release") => "Java: релизы",
        (Lang::En, "java_release") => "Java: releases",
        (Lang::Ru, "java_snapshot") => "Java: снапшоты",
        (Lang::En, "java_snapshot") => "Java: snapshots",
        (Lang::Ru, "bedrock_release") => "Bedrock: релизы",
        (Lang::En, "bedrock_release") => "Bedrock: releases",
        (Lang::Ru, "bedrock_preview") => "Bedrock: Beta/Preview",
        (Lang::En, "bedrock_preview") => "Bedrock: Beta/Preview",
        _ => "?",
    }
}

pub enum M<'a> {
    Welcome,
    Help,
    Privacy,
    SettingsTitle,
    LanguageTitle,
    LanguageSet,
    VersionsTitle,
    VersionLine {
        label: &'a str,
        value: &'a str,
        url: &'a str,
    },
    VersionUnavailable {
        label: &'a str,
    },
    SearchUsage,
    SearchTitle {
        query: &'a str,
    },
    SearchNone {
        query: &'a str,
    },
    LinkNotConfigured,
    LinkBusy,
    LinkInstructions {
        url: &'a str,
        code: &'a str,
        minutes: u64,
    },
    LinkSuccess {
        name: &'a str,
        capes: usize,
    },
    LinkDeclined,
    LinkExpired,
    LinkNoXbox,
    LinkChild,
    LinkRegionBlocked,
    LinkNoProfile,
    LinkNotApproved,
    LinkFailed,
    Unlinked,
    NotLinked,
    DeletedAll,
    Profile {
        name: &'a str,
        uuid: &'a str,
        slim: bool,
        has_cape: bool,
    },
    NickInvalid,
    NickNotFound,
    NetworkError,
    NoSkin {
        name: &'a str,
    },
    NoCape {
        name: &'a str,
    },
    SkinCaption {
        name: &'a str,
        slim: bool,
    },
    CapeCaption {
        name: &'a str,
    },
    NameMcButton,
    NewJava {
        id: &'a str,
        snapshot: bool,
        url: &'a str,
    },
    NewBedrock {
        title: &'a str,
        preview: bool,
        url: &'a str,
    },
}

pub fn t(l: Lang, m: M) -> String {
    match m {
        M::Welcome => tr!(
            l,
            "Привет! Я присылаю в личные сообщения ссылку на список изменений, как только выходит новая версия Minecraft (Java и Bedrock).\n\nПодписки уже включены, настроить их можно в /settings. Все команды: /help.\n\nВсе данные я беру только с официальных серверов Minecraft, Mojang и Microsoft. Что хранится: /privacy.",
            "Hi! I send you a direct message with the changelog link as soon as a new Minecraft version (Java or Bedrock) is released.\n\nSubscriptions are already on; tune them in /settings. All commands: /help.\n\nI only use official Minecraft, Mojang and Microsoft servers. What is stored: /privacy."
        ),
        M::Help => tr!(
            l,
            "Команды:\n/version: актуальные версии Java и Bedrock\n/search <запрос>: найти версию и ссылку на список изменений (например, /search 1.21.4)\n/link: привязать аккаунт Minecraft (Microsoft/Xbox)\n/unlink: отвязать аккаунт\n/profile: данные вашего профиля\n/skin [ник]: скин (свой или по нику)\n/cape [ник]: плащ (свой или по нику)\n/settings: на какие версии подписаться\n/language: язык бота\n/privacy: какие данные хранятся\n/delete: удалить все мои данные",
            "Commands:\n/version: current Java and Bedrock versions\n/search <query>: find a version and its changelog link (e.g. /search 1.21.4)\n/link: link your Minecraft account (Microsoft/Xbox)\n/unlink: unlink the account\n/profile: your profile data\n/skin [name]: skin (yours or by nickname)\n/cape [name]: cape (yours or by nickname)\n/settings: choose which versions to follow\n/language: bot language\n/privacy: what data is stored\n/delete: delete all my data"
        ),
        M::Privacy => tr!(
            l,
            "Что хранится: ваш Telegram ID (чтобы писать вам в личку), язык, выбранные подписки и, если вы привязали аккаунт, UUID вашего профиля Minecraft. Больше ничего.\n\nЧего нет: токены Microsoft/Xbox не сохраняются (они используются один раз при привязке и сразу отбрасываются), ваши сообщения и запрошенные ники не записываются.\n\nОткуда данные: только официальные серверы Minecraft, Mojang и Microsoft, прямыми запросами. Профили кэшируются в памяти на 1 минуту.\n\n/delete удаляет всё, что о вас хранится.",
            "What is stored: your Telegram ID (to message you), language, chosen subscriptions and, if you linked an account, your Minecraft profile UUID. Nothing else.\n\nWhat is not: Microsoft/Xbox tokens are not saved (they are used once while linking and discarded), your messages and looked-up nicknames are not recorded.\n\nSources: official Minecraft, Mojang and Microsoft servers only, via direct requests. Profiles are cached in memory for 1 minute.\n\n/delete removes everything stored about you."
        ),
        M::SettingsTitle => tr!(
            l,
            "О каких версиях сообщать? Нажмите, чтобы включить или выключить:",
            "Which versions should I notify you about? Tap to toggle:"
        ),
        M::LanguageTitle => tr!(l, "Выберите язык:", "Choose a language:"),
        M::LanguageSet => tr!(l, "Язык переключён на русский.", "Language switched to English."),
        M::VersionsTitle => tr!(l, "Актуальные версии:", "Latest versions:"),
        M::VersionLine { label, value, url } => format!("{label}: {value}\n{url}"),
        M::VersionUnavailable { label } => tr!(
            l,
            "{}: сейчас недоступно",
            "{}: currently unavailable",
            label
        ),
        M::SearchUsage => tr!(
            l,
            "Напишите, что искать: /search 1.21.4 или /search 25w02a",
            "Tell me what to look for: /search 1.21.4 or /search 25w02a"
        ),
        M::SearchTitle { query } => tr!(l, "Результаты по запросу «{}»:", "Results for \"{}\":", query),
        M::SearchNone { query } => tr!(
            l,
            "По запросу «{}» ничего не найдено.",
            "Nothing found for \"{}\".",
            query
        ),
        M::LinkNotConfigured => tr!(
            l,
            "Привязка аккаунта не настроена владельцем бота (нет MS_CLIENT_ID).",
            "Account linking is not configured by the bot owner (MS_CLIENT_ID is missing)."
        ),
        M::LinkBusy => tr!(
            l,
            "Привязка уже идёт. Завершите её или подождите, пока истечёт код.",
            "Linking is already in progress. Finish it or wait for the code to expire."
        ),
        M::LinkInstructions { url, code, minutes } => tr!(
            l,
            "Откройте {} и введите код: {}\n\nВойдите в тот Microsoft-аккаунт, с которым вы играете. Код действует {} мин. Вход выполняется на сайте Microsoft, бот не видит ваш пароль.",
            "Open {} and enter the code: {}\n\nSign in with the Microsoft account you play with. The code is valid for {} min. Sign-in happens on Microsoft's site; the bot never sees your password.",
            url,
            code,
            minutes
        ),
        M::LinkSuccess { name, capes } => tr!(
            l,
            "Готово! Аккаунт привязан: {}.\nПлащей на аккаунте: {}.\nТокены Microsoft не сохранены. Попробуйте /profile, /skin и /cape.",
            "Done! Account linked: {}.\nCapes on the account: {}.\nMicrosoft tokens were not saved. Try /profile, /skin and /cape.",
            name,
            capes
        ),
        M::LinkDeclined => tr!(l, "Вход отменён.", "Sign-in was declined."),
        M::LinkExpired => tr!(
            l,
            "Код истёк. Запустите /link ещё раз.",
            "The code expired. Run /link again."
        ),
        M::LinkNoXbox => tr!(
            l,
            "У этого Microsoft-аккаунта нет профиля Xbox. Создайте его на xbox.com и повторите /link.",
            "This Microsoft account has no Xbox profile. Create one at xbox.com and run /link again."
        ),
        M::LinkChild => tr!(
            l,
            "Это детский аккаунт: Xbox не разрешает вход, пока взрослый не добавит его в семейную группу.",
            "This is a child account: Xbox blocks sign-in until an adult adds it to a Microsoft family group."
        ),
        M::LinkRegionBlocked => tr!(
            l,
            "Xbox Live недоступен для аккаунта в этой стране или регионе.",
            "Xbox Live is not available for this account's country or region."
        ),
        M::LinkNoProfile => tr!(
            l,
            "На этом аккаунте нет профиля Minecraft: Java Edition.",
            "This account has no Minecraft: Java Edition profile."
        ),
        M::LinkNotApproved => tr!(
            l,
            "Mojang отклонил запрос: приложение бота не одобрено для Minecraft API. Это настройка владельца бота.",
            "Mojang rejected the request: the bot's app is not approved for the Minecraft API. This is a bot-owner setting."
        ),
        M::LinkFailed => tr!(
            l,
            "Не удалось привязать аккаунт. Попробуйте позже.",
            "Could not link the account. Please try again later."
        ),
        M::Unlinked => tr!(l, "Аккаунт отвязан.", "Account unlinked."),
        M::NotLinked => tr!(
            l,
            "Аккаунт не привязан. Используйте /link или укажите ник: /skin Notch",
            "No account linked. Use /link or pass a nickname: /skin Notch"
        ),
        M::DeletedAll => tr!(
            l,
            "Все данные о вас удалены. Чтобы снова получать уведомления, отправьте /start.",
            "All your data was deleted. Send /start to receive notifications again."
        ),
        M::Profile { name, uuid, slim, has_cape } => {
            let model = if slim { "Alex (slim)" } else { "Steve (classic)" };
            tr!(
                l,
                "Профиль Minecraft\nНик: {}\nUUID: {}\nМодель скина: {}\nПлащ: {}\n\nДанные получены с серверов Mojang. Игровой статистики (убийства, время игры и т. п.) у Mojang нет в открытом API, поэтому её бот показать не может.",
                "Minecraft profile\nName: {}\nUUID: {}\nSkin model: {}\nCape: {}\n\nData comes from Mojang servers. Mojang's public API has no gameplay statistics (kills, playtime, etc.), so the bot cannot show them.",
                name,
                uuid,
                model,
                if has_cape { tr!(l, "есть", "yes") } else { tr!(l, "нет", "no") }
            )
        }
        M::NickInvalid => tr!(
            l,
            "Ник Minecraft Java: 3–16 символов, только латинские буквы, цифры и «_».",
            "A Minecraft Java name is 3–16 characters: Latin letters, digits and \"_\" only."
        ),
        M::NickNotFound => tr!(l, "Игрок с таким ником не найден.", "No player with that name was found."),
        M::NetworkError => tr!(
            l,
            "Не получилось связаться с серверами Mojang/Minecraft. Попробуйте чуть позже.",
            "Could not reach Mojang/Minecraft servers. Please try again shortly."
        ),
        M::NoSkin { name } => tr!(
            l,
            "У {} нет пользовательского скина (используется стандартный).",
            "{} has no custom skin (a default one is used).",
            name
        ),
        M::NoCape { name } => tr!(
            l,
            "У {} нет плаща Mojang. Плащи OptiFine, LabyMod и другие можно посмотреть на NameMC.",
            "{} has no Mojang cape. OptiFine, LabyMod and other capes can be viewed on NameMC.",
            name
        ),
        M::NameMcButton => tr!(l, "Открыть на NameMC", "Open on NameMC"),
        M::SkinCaption { name, slim } => {
            let model = if slim { "Alex (slim)" } else { "Steve (classic)" };
            tr!(l, "Скин {} ({})", "Skin of {} ({})", name, model)
        }
        M::CapeCaption { name } => tr!(l, "Активный плащ {}", "Active cape of {}", name),
        M::NewJava { id, snapshot, url } => {
            let kind = match (l, snapshot) {
                (Lang::Ru, true) => "снапшот",
                (Lang::Ru, false) => "релиз",
                (Lang::En, true) => "snapshot",
                (Lang::En, false) => "release",
            };
            tr!(
                l,
                "Вышла новая версия Minecraft Java Edition ({}): {}\n{}",
                "A new Minecraft Java Edition {} is out: {}\n{}",
                kind,
                id,
                url
            )
        }
        M::NewBedrock { title, preview, url } => {
            let kind = match (l, preview) {
                (_, true) => "Beta/Preview",
                (Lang::Ru, false) => "релиз",
                (Lang::En, false) => "release",
            };
            tr!(
                l,
                "Новая версия Minecraft Bedrock Edition ({}): {}\n{}",
                "New Minecraft Bedrock Edition {}: {}\n{}",
                kind,
                title,
                url
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telegram_language_detection() {
        assert_eq!(Lang::from_telegram(Some("ru")), Lang::Ru);
        assert_eq!(Lang::from_telegram(Some("RU-RU")), Lang::Ru);
        assert_eq!(Lang::from_telegram(Some("en")), Lang::En);
        assert_eq!(Lang::from_telegram(Some("de")), Lang::En);
        assert_eq!(Lang::from_telegram(None), Lang::En);
    }

    #[test]
    fn namemc_links_only_for_valid_names() {
        assert_eq!(
            namemc_url(Lang::Ru, "Notch").as_deref(),
            Some("https://ru.namemc.com/profile/Notch")
        );
        assert_eq!(
            namemc_url(Lang::En, "a_b").as_deref(),
            Some("https://namemc.com/profile/a_b")
        );
        for bad in [
            "",
            "ab",
            "../x",
            "a b",
            "name?x=1",
            "a/b/c",
            "ник",
            "seventeen_chars_xx",
        ] {
            assert_eq!(namemc_url(Lang::En, bad), None, "{bad:?}");
        }
    }

    #[test]
    fn stored_language_roundtrip() {
        for l in [Lang::Ru, Lang::En] {
            assert_eq!(Lang::parse(l.code()), l);
        }
        assert_eq!(Lang::parse("garbage"), Lang::En);
    }

    #[test]
    fn messages_contain_their_arguments() {
        for l in [Lang::Ru, Lang::En] {
            let s = t(
                l,
                M::LinkInstructions {
                    url: "https://www.microsoft.com/link",
                    code: "ABCD1234",
                    minutes: 15,
                },
            );
            assert!(s.contains("https://www.microsoft.com/link") && s.contains("ABCD1234") && s.contains("15"));
            let s = t(
                l,
                M::NewJava {
                    id: "1.21.4",
                    snapshot: false,
                    url: "https://minecraft.wiki/w/Java_Edition_1.21.4",
                },
            );
            assert!(s.contains("1.21.4") && s.contains("minecraft.wiki"));
            if l == Lang::En {
                assert!(
                    s.contains("release") && !s.contains("релиз"),
                    "английский текст без русских слов"
                );
            }
            let s = t(
                l,
                M::NewBedrock {
                    title: "26.60",
                    preview: false,
                    url: "https://feedback.minecraft.net/hc/en-us/articles/1",
                },
            );
            assert!(s.contains("26.60"));
            if l == Lang::En {
                assert!(!s.contains("релиз"));
            }
            let s = t(
                l,
                M::Profile {
                    name: "Steve",
                    uuid: "u",
                    slim: true,
                    has_cape: false,
                },
            );
            assert!(s.contains("Steve") && s.contains("Alex"));
        }
    }
}
