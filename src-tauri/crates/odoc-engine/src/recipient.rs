use crate::types::RecipientInference;
use odoc_model::{DocumentType, ODoc};
use serde::{Deserialize, Deserializer};

// Optional metadata must not invalidate an otherwise usable fact extraction.
pub(crate) fn deserialize_inference<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<RecipientInference>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// Reference recipients belong to the source document, not necessarily to the new one.
pub(crate) fn writing_brief(material: &str) -> String {
    let mut brief = String::new();
    let mut remaining = material;
    while let Some(start) = remaining.find("【参考来源｜") {
        brief.push_str(&remaining[..start]);
        let Some(end) = remaining[start..].find("【参考来源结束】") else {
            return brief;
        };
        remaining = &remaining[start + end + "【参考来源结束】".len()..];
    }
    brief.push_str(remaining);
    brief
}

pub(crate) fn apply_recipient_inference(
    document: &mut ODoc,
    material: &str,
    inference: Option<RecipientInference>,
) -> RecipientInference {
    if document
        .body
        .main_recipients
        .iter()
        .any(|name| !name.trim().is_empty())
    {
        return RecipientInference {
            recipients: document.body.main_recipients.clone(),
            ..RecipientInference::default()
        };
    }
    let inference = inference.unwrap_or_default();
    let brief = writing_brief(material);
    let mut recipients = Vec::new();
    for name in &inference.recipients {
        let name = name.trim().trim_end_matches(['：', ':']);
        if !recipients.iter().any(|existing| existing == name) {
            recipients.push(name.to_owned());
        }
    }
    let question = inference
        .question
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty());
    let grounded = question.is_none()
        && !recipients.is_empty()
        && recipients.len() <= 32
        && !(document.meta.doc_type == DocumentType::Request && recipients.len() > 1)
        && inference.evidence.as_deref().is_some_and(|evidence| {
            !evidence.trim().is_empty()
                && evidence.len() <= 2048
                && brief.contains(evidence)
                && recipients.iter().all(|name| {
                    !name.is_empty()
                        && name.len() <= 256
                        && !name.chars().any(char::is_control)
                        && evidence.contains(name)
                        && has_explicit_recipient_relation(evidence, name)
                })
        });
    if grounded {
        document.body.main_recipients.clone_from(&recipients);
        return RecipientInference {
            recipients,
            evidence: inference.evidence,
            question: None,
        };
    }

    RecipientInference {
        question: document.meta.doc_type.requires_main_recipient().then(|| {
            question
                .filter(|q| q.len() <= 1024 && !q.chars().any(char::is_control))
                .map_or_else(
                    || {
                        format!(
                            "这份{}准备发给哪个机关或哪些单位？不确定可以先留空。",
                            document.meta.doc_type.label_zh()
                        )
                    },
                    str::to_owned,
                )
        }),
        ..RecipientInference::default()
    }
}

fn has_explicit_recipient_relation(evidence: &str, recipient: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "主送机关为",
        "主送机关是",
        "主送对象为",
        "主送单位为",
        "收文对象为",
        "收文对象是",
        "收文单位为",
        "发文对象为",
        "发文对象是",
        "发送给",
        "发送至",
        "报送给",
        "报送至",
        "报送",
        "上报给",
        "上报至",
        "上报",
        "呈报给",
        "呈报至",
        "呈报",
        "发给",
        "送给",
        "送至",
        "送交",
        "寄给",
        "主送：",
        "主送:",
        "主送",
        "通知",
        "函告",
        "报请",
        "向",
        "给",
    ];
    const NEGATIONS: &[&str] = &[
        "不要", "不应", "不得", "不必", "不用", "无需", "无须", "不是", "不",
    ];

    PREFIXES.iter().any(|prefix| {
        evidence.match_indices(prefix).any(|(index, _)| {
            let tail = &evidence[index + prefix.len()..];
            let clause = tail
                .split(['，', '。', '；', ';', '\n'])
                .next()
                .unwrap_or_default();
            if !clause.contains(recipient) {
                return false;
            }
            let preceding = evidence[..index]
                .rsplit(['，', '。', '；', ';', '\n'])
                .next()
                .unwrap_or_default();
            !NEGATIONS
                .iter()
                .any(|negation| preceding.contains(negation))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inference(name: &str, evidence: &str) -> RecipientInference {
        RecipientInference {
            recipients: vec![name.into()],
            evidence: Some(evidence.into()),
            question: None,
        }
    }

    #[test]
    fn fills_only_grounded_recipients_and_preserves_explicit_fields() {
        let mut document = ODoc::empty();
        document.meta.doc_type = DocumentType::Notice;
        let result = apply_recipient_inference(
            &mut document,
            "给各乡镇发一个防汛通知",
            Some(inference("各乡镇", "给各乡镇发一个防汛通知")),
        );
        assert_eq!(document.body.main_recipients, ["各乡镇"]);
        assert_eq!(result.question, None);
        apply_recipient_inference(
            &mut document,
            "给市政府发函",
            Some(inference("市政府", "给市政府发函")),
        );
        assert_eq!(document.body.main_recipients, ["各乡镇"]);
    }

    #[test]
    fn does_not_copy_reference_recipients_or_expand_institution_names() {
        for (material, candidate) in [
            (
                "写一份通知\n【参考来源｜上传文件：去年通知】\n主送机关：市政府\n【参考来源结束】",
                inference("市政府", "主送机关：市政府"),
            ),
            (
                "给各乡镇发一个通知",
                inference("各乡镇人民政府", "给各乡镇发一个通知"),
            ),
            (
                "住建局参加了会议，写一份工作通知",
                inference("住建局", "住建局参加了会议"),
            ),
            (
                "不要给市政府发通知",
                inference("市政府", "不要给市政府发通知"),
            ),
            ("不要发给市政府", inference("市政府", "不要发给市政府")),
        ] {
            let mut document = ODoc::empty();
            let result = apply_recipient_inference(&mut document, material, Some(candidate));
            assert_eq!(document.body.main_recipients.len(), 0);
            assert!(result.question.is_some());
        }
    }

    #[test]
    fn accepts_multiple_recipients_named_in_the_same_explicit_direction() {
        let material = "向市政府和财政局报送材料";
        let mut document = ODoc::empty();
        document.meta.doc_type = DocumentType::Notice;
        let result = apply_recipient_inference(
            &mut document,
            material,
            Some(RecipientInference {
                recipients: vec!["市政府".into(), "财政局".into()],
                evidence: Some(material.into()),
                question: None,
            }),
        );
        assert_eq!(document.body.main_recipients, ["市政府", "财政局"]);
        assert_eq!(result.question, None);
    }

    #[test]
    fn informal_work_reports_do_not_require_a_recipient() {
        let mut document = ODoc::empty();
        document.meta.doc_type = DocumentType::WorkReport;
        let result = apply_recipient_inference(&mut document, "写住建工作汇报", None);
        assert_eq!(result.recipients.len(), 0);
        assert_eq!(result.question, None);
    }

    #[test]
    fn malformed_optional_metadata_does_not_block_fact_extraction() {
        let facts: crate::FactExtraction =
            serde_json::from_str(r#"{"facts":[],"recipient":{"recipients":"未知"}}"#)
                .expect("optional metadata");
        assert_eq!(facts.recipient, None);
        assert!(
            serde_json::to_value(facts)
                .expect("facts JSON")
                .get("recipient")
                .is_none()
        );
    }

    #[test]
    fn uncertainty_and_multiple_request_recipients_become_optional_questions() {
        let mut document = ODoc::empty();
        let result = apply_recipient_inference(
            &mut document,
            "给市政府和财政局写请示",
            Some(RecipientInference {
                recipients: vec!["市政府".into(), "财政局".into()],
                evidence: Some("给市政府和财政局写请示".into()),
                question: None,
            }),
        );
        assert_eq!(document.body.main_recipients.len(), 0);
        assert!(result.question.is_some());
        let result = apply_recipient_inference(
            &mut document,
            "市政府",
            Some(RecipientInference {
                question: Some("是报送市政府吗？".into()),
                ..inference("市政府", "市政府")
            }),
        );
        assert_eq!(document.body.main_recipients.len(), 0);
        assert_eq!(result.question.as_deref(), Some("是报送市政府吗？"));
    }
}
