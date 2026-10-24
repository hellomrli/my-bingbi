use crate::Frontmatter;
use odoc_llm::SecurityLevel;

/// Untagged legacy notes are internal; an explicit unknown classification fails closed.
pub fn source_security_level(frontmatter: &Frontmatter) -> SecurityLevel {
    match frontmatter
        .get("security_level")
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        None | Some("internal" | "内部") => SecurityLevel::Internal,
        Some("public" | "公开") => SecurityLevel::Public,
        Some("secret" | "秘密") => SecurityLevel::Secret,
        _ => SecurityLevel::Confidential,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_classifications_fail_closed() {
        for (value, expected) in [
            ("public", SecurityLevel::Public),
            (" 秘密 ", SecurityLevel::Secret),
            ("CONFIDENTIAL", SecurityLevel::Confidential),
            ("绝密", SecurityLevel::Confidential),
            ("", SecurityLevel::Confidential),
        ] {
            assert_eq!(
                source_security_level(&Frontmatter::from([(
                    "security_level".into(),
                    value.into()
                )])),
                expected
            );
        }
        assert_eq!(
            source_security_level(&Frontmatter::new()),
            SecurityLevel::Internal
        );
    }
}
