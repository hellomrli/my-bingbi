use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

pub const ODOC_SCHEMA_VERSION: &str = "0.1.0";

fn node_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum DocumentType {
    Request,
    Notice,
    Report,
    Letter,
    Minutes,
    Summary,
    Decision,
    Announcement,
    Regulation,
    Proclamation,
    Reply,
    Instruction,
    Proposal,
    WorkPlan,
    WorkReport,
    Briefing,
    Speech,
    Presentation,
    Memo,
    Invitation,
    Certificate,
    Other,
}

impl DocumentType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Notice => "notice",
            Self::Report => "report",
            Self::Letter => "letter",
            Self::Minutes => "minutes",
            Self::Summary => "summary",
            Self::Decision => "decision",
            Self::Announcement => "announcement",
            Self::Regulation => "regulation",
            Self::Proclamation => "proclamation",
            Self::Reply => "reply",
            Self::Instruction => "instruction",
            Self::Proposal => "proposal",
            Self::WorkPlan => "work_plan",
            Self::WorkReport => "work_report",
            Self::Briefing => "briefing",
            Self::Speech => "speech",
            Self::Presentation => "presentation",
            Self::Memo => "memo",
            Self::Invitation => "invitation",
            Self::Certificate => "certificate",
            Self::Other => "other",
        }
    }

    pub const fn label_zh(self) -> &'static str {
        match self {
            Self::Request => "请示",
            Self::Notice => "通知",
            Self::Report => "报告",
            Self::Letter => "函",
            Self::Minutes => "纪要",
            Self::Summary => "总结",
            Self::Decision => "决定",
            Self::Announcement => "公告",
            Self::Regulation => "公报",
            Self::Proclamation => "通告",
            Self::Reply => "批复",
            Self::Instruction => "命令",
            Self::Proposal => "议案",
            Self::WorkPlan => "工作方案",
            Self::WorkReport => "工作汇报",
            Self::Briefing => "简报",
            Self::Speech => "讲话稿",
            Self::Presentation => "发言稿",
            Self::Memo => "备忘录",
            Self::Invitation => "邀请函",
            Self::Certificate => "证明",
            Self::Other => "其他",
        }
    }

    pub const fn requires_main_recipient(self) -> bool {
        matches!(
            self,
            Self::Request
                | Self::Notice
                | Self::Report
                | Self::Letter
                | Self::Decision
                | Self::Reply
                | Self::Instruction
                | Self::Proposal
                | Self::WorkPlan
                | Self::WorkReport
                | Self::Memo
                | Self::Invitation
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum SecurityLevel {
    Public,
    Internal,
    Secret,
    Confidential,
}

impl SecurityLevel {
    pub const fn allows_remote_provider(self) -> bool {
        matches!(self, Self::Public | Self::Internal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum Urgency {
    Normal,
    Urgent,
    ExtraUrgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum DocumentStatus {
    Draft,
    Review,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DocMeta {
    pub doc_type: DocumentType,
    pub custom_doc_type: Option<String>,
    pub security_level: SecurityLevel,
    pub urgency: Urgency,
    pub copy_number: Option<String>,
    pub status: DocumentStatus,
}

impl Default for DocMeta {
    fn default() -> Self {
        Self {
            doc_type: DocumentType::Request,
            custom_doc_type: None,
            security_level: SecurityLevel::Internal,
            urgency: Urgency::Normal,
            copy_number: None,
            status: DocumentStatus::Draft,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Header {
    pub id: String,
    pub issuing_authority_mark: String,
    pub document_number: Option<String>,
    pub signer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Title {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum HeadingLevel {
    L1,
    L2,
    L3,
    L4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingStyle {
    pub prefix_example: &'static str,
    pub font_family: &'static str,
    pub size_pt: u8,
}

impl HeadingLevel {
    pub const fn style(self) -> HeadingStyle {
        match self {
            Self::L1 => HeadingStyle {
                prefix_example: "一、",
                font_family: "黑体",
                size_pt: 16,
            },
            Self::L2 => HeadingStyle {
                prefix_example: "（一）",
                font_family: "楷体_GB2312",
                size_pt: 16,
            },
            Self::L3 => HeadingStyle {
                prefix_example: "1.",
                font_family: "仿宋_GB2312",
                size_pt: 16,
            },
            Self::L4 => HeadingStyle {
                prefix_example: "（1）",
                font_family: "仿宋_GB2312",
                size_pt: 16,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ListStyle {
    Ordered,
    Unordered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ListItem {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct TableCell {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct TableRow {
    pub id: String,
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Heading {
        id: String,
        level: HeadingLevel,
        content: String,
    },
    Paragraph {
        id: String,
        content: String,
    },
    List {
        id: String,
        style: ListStyle,
        items: Vec<ListItem>,
    },
    Table {
        id: String,
        rows: Vec<TableRow>,
    },
    Quote {
        id: String,
        content: String,
    },
}

impl Block {
    pub fn heading(level: HeadingLevel, content: String) -> Self {
        Self::Heading {
            id: node_id("heading"),
            level,
            content,
        }
    }

    pub fn paragraph(content: String) -> Self {
        Self::Paragraph {
            id: node_id("paragraph"),
            content,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Heading { id, .. }
            | Self::Paragraph { id, .. }
            | Self::List { id, .. }
            | Self::Table { id, .. }
            | Self::Quote { id, .. } => id,
        }
    }

    pub fn plain_text(&self) -> String {
        match self {
            Self::Heading { content, .. }
            | Self::Paragraph { content, .. }
            | Self::Quote { content, .. } => content.clone(),
            Self::List { items, .. } => items
                .iter()
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Table { rows, .. } => rows
                .iter()
                .flat_map(|row| row.cells.iter().map(|cell| cell.content.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub order: u32,
    pub title: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Signer {
    pub id: String,
    pub authority: String,
    pub representative: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct OfficialDate {
    pub id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Body {
    pub id: String,
    pub title: Title,
    pub main_recipients: Vec<String>,
    pub blocks: Vec<Block>,
    pub attachments: Vec<Attachment>,
    pub signer: Option<Signer>,
    pub date: Option<OfficialDate>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Footer {
    pub id: String,
    pub copy_recipients: Vec<String>,
    pub printing_authority: Option<String>,
    pub printing_date: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum FactKind {
    Subject,
    Object,
    Action,
    Data,
    Time,
    Place,
    Problem,
    Basis,
    Responsibility,
    Feedback,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum FactSource {
    UserInput,
    KbRetrieved,
    ModelGenerated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Fact {
    pub id: String,
    pub kind: FactKind,
    pub value: String,
    pub unit: Option<String>,
    pub role: Option<String>,
    pub source: FactSource,
    pub source_ref: Option<String>,
    pub verified: bool,
}

impl Fact {
    pub fn from_user_input(
        kind: FactKind,
        value: String,
        unit: Option<String>,
        role: Option<String>,
        source_ref: Option<String>,
    ) -> Self {
        Self {
            id: node_id("fact"),
            kind,
            value,
            unit,
            role,
            source: FactSource::UserInput,
            source_ref,
            verified: true,
        }
    }

    pub fn from_model_generated(
        kind: FactKind,
        value: String,
        unit: Option<String>,
        role: Option<String>,
    ) -> Self {
        Self {
            id: node_id("fact"),
            kind,
            value,
            unit,
            role,
            source: FactSource::ModelGenerated,
            source_ref: None,
            verified: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FactLedger {
    pub facts: Vec<Fact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ProvenanceSource {
    User,
    KnowledgeBase,
    Model,
    Imported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Citation {
    pub id: String,
    pub node_id: String,
    pub source: ProvenanceSource,
    pub source_ref: Option<String>,
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ODoc {
    pub schema_version: String,
    pub id: String,
    pub revision: u32,
    pub meta: DocMeta,
    pub header: Option<Header>,
    pub body: Body,
    pub footer: Option<Footer>,
    pub facts: FactLedger,
    pub provenance: Vec<Citation>,
}

impl ODoc {
    pub fn empty() -> Self {
        Self {
            schema_version: ODOC_SCHEMA_VERSION.to_owned(),
            id: node_id("doc"),
            revision: 0,
            meta: DocMeta::default(),
            header: Some(Header {
                id: node_id("header"),
                issuing_authority_mark: String::new(),
                document_number: None,
                signer: None,
            }),
            body: Body {
                id: node_id("body"),
                title: Title {
                    id: node_id("title"),
                    text: String::new(),
                },
                main_recipients: Vec::new(),
                blocks: vec![Block::Paragraph {
                    id: node_id("block"),
                    content: String::new(),
                }],
                attachments: Vec::new(),
                signer: None,
                date: None,
                note: None,
            },
            footer: None,
            facts: FactLedger::default(),
            provenance: Vec::new(),
        }
    }

    pub fn to_plain_text(&self) -> String {
        let mut sections = Vec::new();
        if let Some(header) = &self.header {
            sections.push(header.issuing_authority_mark.clone());
            if let Some(document_number) = &header.document_number {
                sections.push(document_number.clone());
            }
        }
        sections.push(self.body.title.text.clone());
        sections.extend(self.body.main_recipients.iter().cloned());
        sections.extend(self.body.blocks.iter().map(Block::plain_text));
        if let Some(signer) = &self.body.signer {
            sections.push(signer.authority.clone());
        }
        if let Some(date) = &self.body.date {
            sections.push(date.value.clone());
        }
        sections
            .into_iter()
            .filter(|section| !section.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn typescript_bindings() -> String {
    let declarations = [
        DocumentType::decl(),
        SecurityLevel::decl(),
        Urgency::decl(),
        DocumentStatus::decl(),
        DocMeta::decl(),
        Header::decl(),
        Title::decl(),
        HeadingLevel::decl(),
        ListStyle::decl(),
        ListItem::decl(),
        TableCell::decl(),
        TableRow::decl(),
        Block::decl(),
        Attachment::decl(),
        Signer::decl(),
        OfficialDate::decl(),
        Body::decl(),
        Footer::decl(),
        FactKind::decl(),
        FactSource::decl(),
        Fact::decl(),
        FactLedger::decl(),
        ProvenanceSource::decl(),
        Citation::decl(),
        ODoc::decl(),
    ];
    format!(
        "// This file is generated from src-tauri/crates/odoc-model. Do not edit by hand.\n\nexport {}\n",
        declarations.join("\n\nexport ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_document_serialization_round_trips() {
        let original = ODoc::empty();
        let json = serde_json::to_string_pretty(&original).expect("ODoc should serialize");
        let decoded: ODoc = serde_json::from_str(&json).expect("ODoc should deserialize");
        assert_eq!(decoded, original);
        assert!(json.contains("\"schemaVersion\": \"0.1.0\""));
        assert!(json.contains("\"type\": \"paragraph\""));
    }

    #[test]
    fn empty_document_has_unique_stable_node_ids() {
        let first = ODoc::empty();
        let second = ODoc::empty();
        assert_ne!(first.id, second.id);
        assert_ne!(first.body.id, first.body.title.id);
        assert_ne!(first.body.title.id, first.body.blocks[0].id());
    }

    #[test]
    fn heading_levels_have_gb9704_styles() {
        assert_eq!(HeadingLevel::L1.style().font_family, "黑体");
        assert_eq!(HeadingLevel::L2.style().font_family, "楷体_GB2312");
        assert_eq!(HeadingLevel::L3.style().size_pt, 16);
        assert_eq!(HeadingLevel::L4.style().prefix_example, "（1）");
    }

    #[test]
    fn secret_documents_disallow_remote_providers() {
        assert!(SecurityLevel::Internal.allows_remote_provider());
        assert!(!SecurityLevel::Secret.allows_remote_provider());
        assert!(!SecurityLevel::Confidential.allows_remote_provider());
    }

    #[test]
    fn plain_text_preserves_block_order() {
        let mut document = ODoc::empty();
        document.body.title.text = "测试请示".to_owned();
        document.body.blocks = vec![
            Block::Heading {
                id: node_id("heading"),
                level: HeadingLevel::L1,
                content: "基本情况".to_owned(),
            },
            Block::Paragraph {
                id: node_id("paragraph"),
                content: "正文内容".to_owned(),
            },
        ];
        assert_eq!(document.to_plain_text(), "测试请示\n基本情况\n正文内容");
    }
}
