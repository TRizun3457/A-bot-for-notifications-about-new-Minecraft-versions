//! Список доменов, к которым бот вообще имеет право обращаться.
//!
//! Требование проекта: запросы только к Minecraft, Feedback, Microsoft, Mojang
//! и Minecraft Wiki, никаких сторонних источников. Проверка делается в одном
//! месте (см. `http.rs`) и применяется и к исходным URL, и к редиректам.

/// Разрешённые домены. Поддомены разрешены автоматически
/// (`feedback.minecraft.net`, `launchermeta.mojang.com` и т. д.).
pub const ALLOWED_DOMAINS: &[&str] = &[
    // Minecraft (в том числе feedback.minecraft.net и textures.minecraft.net)
    "minecraft.net",
    "minecraft.com",
    // Minecraft Wiki
    "minecraft.wiki",
    // Mojang: launchermeta, sessionserver, api.mojang.com
    "mojang.com",
    // API профилей Minecraft (принадлежит Mojang)
    "minecraftservices.com",
    // Microsoft: вход через Microsoft-аккаунт и Xbox Live
    "microsoft.com",
    "microsoftonline.com",
    "live.com",
    "xboxlive.com",
];

/// Проверяет, что хост входит в список разрешённых (точное совпадение
/// или поддомен на границе метки: `evilminecraft.net` не проходит).
pub fn is_allowed_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return false;
    }
    ALLOWED_DOMAINS.iter().any(|d| {
        host == *d || (host.len() > d.len() && host.ends_with(d) && host.as_bytes()[host.len() - d.len() - 1] == b'.')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_listed_hosts_and_subdomains() {
        for h in [
            "minecraft.net",
            "feedback.minecraft.net",
            "textures.minecraft.net",
            "launchermeta.mojang.com",
            "sessionserver.mojang.com",
            "api.minecraftservices.com",
            "login.microsoftonline.com",
            "user.auth.xboxlive.com",
            "xsts.auth.xboxlive.com",
            "minecraft.wiki",
            "FEEDBACK.Minecraft.NET",
            "minecraft.net.",
        ] {
            assert!(is_allowed_host(h), "{h} должен быть разрешён");
        }
    }

    #[test]
    fn rejects_foreign_and_lookalike_hosts() {
        for h in [
            "",
            "example.com",
            "evilminecraft.net",
            "minecraft.net.evil.com",
            "mojang.com.evil.org",
            "notmojang.com",
            "minecraft.net:443",
            "mine craft.net",
            "crafatar.com",
            "namemc.com",
        ] {
            assert!(!is_allowed_host(h), "{h} не должен быть разрешён");
        }
    }
}
