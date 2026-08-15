use crate::{Frontmatter, KbNote};

const DOCUMENT_TYPES: [(&str, &str); 22] = [
    ("工作汇报", "work_report"),
    ("工作方案", "work_plan"),
    ("邀请函", "invitation"),
    ("讲话稿", "speech"),
    ("发言稿", "presentation"),
    ("备忘录", "memo"),
    ("请示", "request"),
    ("通知", "notice"),
    ("报告", "report"),
    ("纪要", "minutes"),
    ("总结", "summary"),
    ("决定", "decision"),
    ("公告", "announcement"),
    ("公报", "regulation"),
    ("通告", "proclamation"),
    ("批复", "reply"),
    ("命令", "instruction"),
    ("议案", "proposal"),
    ("简报", "briefing"),
    ("证明", "certificate"),
    ("函", "letter"),
    ("其他", "other"),
];

/// Suggests canonical scalar frontmatter fields without overwriting user metadata.
///
/// The inference is intentionally conservative. In particular, an unmarked note is never
/// labelled `public`; security metadata is added only when the source contains an explicit mark.
#[must_use]
pub fn suggest_missing_frontmatter(note: &KbNote) -> Frontmatter {
    let mut fields = Frontmatter::new();
    copy_alias(
        &note.frontmatter,
        &mut fields,
        "doc_type",
        &["文种", "document_type"],
    );
    copy_alias(&note.frontmatter, &mut fields, "year", &["年份"]);
    copy_alias(
        &note.frontmatter,
        &mut fields,
        "issuer",
        &["发文机关", "单位", "authority"],
    );
    copy_alias(
        &note.frontmatter,
        &mut fields,
        "topic",
        &["主题", "subject"],
    );
    copy_alias(
        &note.frontmatter,
        &mut fields,
        "security_level",
        &["密级", "security"],
    );

    if missing(&note.frontmatter, &fields, "doc_type")
        && let Some((_, value)) = document_type(&note.title)
    {
        fields.insert("doc_type".to_owned(), value.to_owned());
    }
    if missing(&note.frontmatter, &fields, "year")
        && let Some(year) = infer_year(&format!("{}\n{}\n{}", note.path, note.title, note.content))
    {
        fields.insert("year".to_owned(), year);
    }
    if missing(&note.frontmatter, &fields, "issuer")
        && let Some(issuer) = infer_issuer(&note.title)
    {
        fields.insert("issuer".to_owned(), issuer);
    }
    if missing(&note.frontmatter, &fields, "topic")
        && let Some(topic) = infer_topic(&note.title)
    {
        fields.insert("topic".to_owned(), topic);
    }
    if missing(&note.frontmatter, &fields, "security_level")
        && let Some(level) = infer_security_level(&note.title, &note.content)
    {
        fields.insert("security_level".to_owned(), level.to_owned());
    }
    fields
}

fn missing(existing: &Frontmatter, proposed: &Frontmatter, key: &str) -> bool {
    existing
        .get(key)
        .is_none_or(|value| value.trim().is_empty())
        && !proposed.contains_key(key)
}

fn copy_alias(existing: &Frontmatter, proposed: &mut Frontmatter, key: &str, aliases: &[&str]) {
    if existing
        .get(key)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return;
    }
    if let Some(value) = aliases
        .iter()
        .filter_map(|alias| existing.get(*alias))
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
    {
        proposed.insert(key.to_owned(), value.to_owned());
    }
}

fn document_type(title: &str) -> Option<(&'static str, &'static str)> {
    DOCUMENT_TYPES
        .iter()
        .copied()
        .find(|(label, _)| title.trim().contains(label))
}

fn infer_year(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    for index in 0..bytes.len().saturating_sub(3) {
        let candidate = &bytes[index..index + 4];
        if !candidate.iter().all(u8::is_ascii_digit)
            || index > 0 && bytes[index - 1].is_ascii_digit()
            || index + 4 < bytes.len() && bytes[index + 4].is_ascii_digit()
        {
            continue;
        }
        let year = std::str::from_utf8(candidate).ok()?.parse::<u16>().ok()?;
        if (1900..=2099).contains(&year) {
            return Some(year.to_string());
        }
    }
    None
}

fn infer_issuer(title: &str) -> Option<String> {
    let prefix = title.trim().split_once("关于")?.0.trim();
    let looks_like_authority = [
        "人民政府",
        "委员会",
        "办公室",
        "领导小组",
        "工作专班",
        "党委",
        "党组",
        "政府",
        "厅",
        "局",
        "部",
        "委",
        "办",
        "院",
        "所",
        "公司",
    ]
    .iter()
    .any(|suffix| prefix.ends_with(suffix));
    let count = prefix.chars().count();
    (looks_like_authority && (2..=64).contains(&count)).then(|| prefix.to_owned())
}

fn infer_topic(title: &str) -> Option<String> {
    let mut topic = title.trim();
    if let Some((issuer, rest)) = topic.split_once("关于")
        && infer_issuer(&format!("{issuer}关于")).is_some()
    {
        topic = rest;
    }
    topic = topic.strip_prefix("关于").unwrap_or(topic).trim();
    if let Some((label, _)) = document_type(topic) {
        topic = topic
            .strip_suffix(label)
            .unwrap_or(topic)
            .trim_end_matches('的')
            .trim();
    }
    let topic = topic.chars().take(120).collect::<String>();
    (!topic.is_empty()).then_some(topic)
}

fn infer_security_level(title: &str, content: &str) -> Option<&'static str> {
    let sample = format!(
        "{}\n{}",
        title,
        content.chars().take(512).collect::<String>()
    );
    if sample.contains("绝密") || sample.contains("机密") {
        Some("confidential")
    } else if sample.contains("秘密") {
        Some("secret")
    } else if sample.contains("内部") {
        Some("internal")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(title: &str) -> KbNote {
        KbNote {
            vault: "政务库".to_owned(),
            path: "公文/2026/测试.md".to_owned(),
            title: title.to_owned(),
            content: "内部资料".to_owned(),
            frontmatter: Frontmatter::new(),
            tags: Vec::new(),
            links: Vec::new(),
            modified_at: None,
        }
    }

    #[test]
    fn suggests_canonical_fields_without_defaulting_to_public() {
        let fields = suggest_missing_frontmatter(&note("市人民政府关于防汛工作的通知"));
        assert_eq!(fields.get("doc_type").map(String::as_str), Some("notice"));
        assert_eq!(fields.get("year").map(String::as_str), Some("2026"));
        assert_eq!(fields.get("issuer").map(String::as_str), Some("市人民政府"));
        assert_eq!(fields.get("topic").map(String::as_str), Some("防汛工作"));
        assert_eq!(
            fields.get("security_level").map(String::as_str),
            Some("internal")
        );
    }

    #[test]
    fn existing_canonical_values_are_never_overwritten() {
        let mut value = note("关于某项工作的报告");
        value
            .frontmatter
            .insert("doc_type".to_owned(), "request".to_owned());
        let fields = suggest_missing_frontmatter(&value);
        assert!(!fields.contains_key("doc_type"));
    }
}
