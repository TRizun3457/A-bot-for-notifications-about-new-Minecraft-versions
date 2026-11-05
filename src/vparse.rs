//! Чистая логика без внешних зависимостей: разбор версий из заголовков
//! и определение «что нового с прошлой проверки».

/// Достаёт первую версию из заголовка статьи: `26.60` (релиз по новой схеме),
/// `1.21.50`, `26.70.23` или `1.21.60.20` (Beta/Preview). Нужно от двух до четырёх чисел.
pub fn extract_version(title: &str) -> Option<String> {
    let bytes = title.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let candidate = title[start..i].trim_end_matches('.');
            let parts: Vec<&str> = candidate.split('.').collect();
            if (2..=4).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty()) {
                return Some(candidate.to_string());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Из списка «новые первыми» выбирает элементы, появившиеся после `last_seen`.
///
/// * `last_seen == None`: канал ещё не инициализирован, ничего не отправляем
///   (иначе при первом запуске каждому пришла бы вся история).
/// * `last_seen` найден: берём всё, что новее него, но не больше `cap`
///   самых свежих. Результат упорядочен от старых к новым.
/// * `last_seen` не найден (запись удалили или список сильно сдвинулся):
///   считаем новым только самый свежий элемент.
pub fn new_since<'a, T, F>(newest_first: &'a [T], last_seen: Option<&str>, cap: usize, id_of: F) -> Vec<&'a T>
where
    F: Fn(&T) -> String,
{
    let Some(last) = last_seen else {
        return Vec::new();
    };
    let mut fresh: Vec<&T> = match newest_first.iter().position(|x| id_of(x) == last) {
        Some(pos) => newest_first[..pos].iter().collect(),
        None => newest_first.iter().take(1).collect(),
    };
    fresh.truncate(cap);
    fresh.reverse();
    fresh
}

/// Находит среди разделов feedback.minecraft.net разделы со списками изменений Bedrock.
/// Возвращает `(release, preview)`. Названия сопоставляются по смыслу, так как точные
/// ID и формулировки владелец сайта может поменять.
pub fn pick_sections(sections: &[(u64, String)]) -> (Option<u64>, Option<u64>) {
    let lower: Vec<(u64, String)> = sections.iter().map(|(id, n)| (*id, n.to_lowercase())).collect();
    let is_preview = |n: &str| n.contains("beta") || n.contains("preview");

    let preview = lower
        .iter()
        .find(|(_, n)| n.contains("changelog") && is_preview(n))
        .map(|(id, _)| *id);

    let release = lower
        .iter()
        .find(|(_, n)| n.contains("release") && n.contains("changelog") && !is_preview(n))
        .or_else(|| lower.iter().find(|(_, n)| n.contains("changelog") && !is_preview(n)))
        .map(|(id, _)| *id);

    (release, preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_changelog_sections_by_meaning() {
        let s = vec![
            (1, "Feedback Guidelines".to_string()),
            (2, "Beta and Preview Information and Changelogs".to_string()),
            (3, "Release Changelogs".to_string()),
            (4, "Add-Ons".to_string()),
        ];
        assert_eq!(pick_sections(&s), (Some(3), Some(2)));
        assert_eq!(pick_sections(&[]), (None, None));
        let only_generic = vec![(9, "Bedrock Changelogs".to_string())];
        assert_eq!(pick_sections(&only_generic), (Some(9), None));
    }

    #[test]
    fn extracts_release_and_preview_versions() {
        assert_eq!(
            extract_version("Minecraft - 1.21.50 (Bedrock)").as_deref(),
            Some("1.21.50")
        );
        assert_eq!(
            extract_version("Minecraft Beta & Preview - 1.21.60.20").as_deref(),
            Some("1.21.60.20")
        );
        assert_eq!(extract_version("Bedrock 1.21.50.").as_deref(), Some("1.21.50"));
    }

    #[test]
    fn extracts_titles_as_published_on_feedback_site() {
        // Реальные заголовки с feedback.minecraft.net (ноябрь 2026).
        assert_eq!(
            extract_version("Minecraft: Bedrock Edition 26.60 Changelog").as_deref(),
            Some("26.60")
        );
        assert_eq!(
            extract_version("Minecraft Beta & Preview - 26.70.23").as_deref(),
            Some("26.70.23")
        );
    }

    #[test]
    fn ignores_titles_without_versions() {
        assert_eq!(extract_version("Welcome to the feedback site"), None);
        assert_eq!(extract_version("Update 2"), None);
        assert_eq!(extract_version("1..2"), None);
        assert_eq!(extract_version(""), None);
    }

    #[test]
    fn new_since_returns_oldest_first_and_respects_cap() {
        let list = vec!["e", "d", "c", "b", "a"];
        let id = |s: &&str| s.to_string();
        assert!(new_since(&list, None, 3, id).is_empty());
        assert!(new_since(&list, Some("e"), 3, id).is_empty());
        let r: Vec<&str> = new_since(&list, Some("c"), 3, id).into_iter().copied().collect();
        assert_eq!(r, vec!["d", "e"]);
        let r: Vec<&str> = new_since(&list, Some("a"), 2, id).into_iter().copied().collect();
        assert_eq!(r, vec!["d", "e"], "при превышении лимита остаются самые свежие");
        let r: Vec<&str> = new_since(&list, Some("zzz"), 3, id).into_iter().copied().collect();
        assert_eq!(r, vec!["e"]);
    }
}
