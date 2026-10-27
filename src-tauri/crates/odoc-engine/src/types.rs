use odoc_lint::{FactCheckReport, LintReport, Severity};
use odoc_llm::TokenUsage;
use odoc_model::{DocumentType, FactKind, ODoc};
use odoc_pack::StageId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum StageKind {
    Llm,
    Pack,
    Draft,
}

/// Selects which stage set a pipeline definition may declare.
///
/// Existing definitions omit the field, so drafting stays the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum PipelineKind {
    #[default]
    Draft,
    Review,
    /// Imitate mode reuses the drafting stages and adds a `style` stage that distills the
    /// caller-supplied model text into a reusable writing-style profile.
    Imitate,
    /// Polish mode rewrites the supplied material in place: it decides the document type,
    /// applies the humanizer pass, then hard-checks the result. No retrieval or outlining.
    Polish,
}

impl PipelineKind {
    /// The stable session-storage identifier, distinct from the human-facing pipeline name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Review => "review",
            Self::Imitate => "imitate",
            Self::Polish => "polish",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum StageStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum LowConfidenceAction {
    AskUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct StageConfig {
    pub id: StageId,
    pub kind: StageKind,
    pub prompt: Option<String>,
    #[serde(default)]
    pub inject: Vec<String>,
    pub output_schema: Option<String>,
    #[serde(default)]
    pub json_mode: bool,
    #[serde(default)]
    pub stream: bool,
    pub on_low_confidence: Option<LowConfidenceAction>,
    #[serde(default)]
    pub loop_back: Option<LoopBackConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
// Pipeline YAML is snake_case throughout, like `StageConfig`; the TypeScript view stays camelCase.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct LoopBackConfig {
    pub when_below: u32,
    pub to: StageId,
    pub max_iterations: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct TypeDecision {
    pub value: DocumentType,
    pub confidence: f32,
    #[serde(default)]
    pub alternatives: Vec<DocumentType>,
    #[serde(default)]
    pub missing_information: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct FactInput {
    pub kind: FactKind,
    pub value: String,
    pub unit: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct FactExtraction {
    #[serde(default)]
    pub facts: Vec<FactInput>,
    #[serde(default)]
    pub missing_information: Vec<String>,
    #[serde(
        default,
        deserialize_with = "crate::recipient::deserialize_inference",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub recipient: Option<RecipientInference>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct RecipientInference {
    #[serde(default)]
    pub recipients: Vec<String>,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StageFailure {
    pub stage_id: StageId,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct ScoreDimension {
    #[ts(type = "number")]
    pub score: u32,
    #[ts(type = "number")]
    pub max_score: u32,
    #[serde(default)]
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct Score {
    #[ts(type = "number")]
    pub total: u32,
    pub dimensions: BTreeMap<String, ScoreDimension>,
    #[serde(default)]
    pub revision_instructions: Vec<String>,
    #[serde(default)]
    pub missing_information: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StageOutput {
    pub stage_id: StageId,
    pub status: StageStatus,
    pub content: String,
    pub json: Option<String>,
    pub usage: Option<TokenUsage>,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StageSummary {
    pub stage_id: StageId,
    pub status: StageStatus,
    pub usage: Option<TokenUsage>,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

/// The result of re-executing a single pipeline stage from its recorded snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RerunOutcome {
    pub session_id: String,
    pub stage_id: StageId,
    pub document: ODoc,
    pub output: StageOutput,
    #[serde(default)]
    pub lint_report: Option<LintReport>,
    #[serde(default)]
    pub fact_check_report: Option<FactCheckReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DraftInput {
    pub material: String,
    /// The reference text an imitate pipeline copies the writing style from. Must be present
    /// for `PipelineKind::Imitate` and absent for any other kind.
    #[serde(default)]
    pub model_text: Option<String>,
    pub initial_document: Option<ODoc>,
    pub confirmed_doc_type: Option<DocumentType>,
    /// Knowledge-pack template variant (e.g. `notice-meeting`); empty means the per-type default.
    #[serde(default)]
    pub template_id: Option<String>,
}

/// One suggested text correction: replace `original` with `corrected` where it appears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Correction {
    pub original: String,
    pub corrected: String,
    #[serde(default)]
    pub reason: String,
}

/// The reusable writing-style profile an imitate pipeline distills from a model text.
///
/// It describes the reference text's own craft, never its facts: numbers, dates, names and
/// institutions must not appear here, so downstream drafting stages cannot copy them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StyleProfile {
    /// 结构安排：层次序数使用、开篇方式、结尾方式。
    pub structure: String,
    /// 句式节奏：长短句搭配、排比与对仗的使用程度。
    pub sentence_patterns: String,
    /// 用词习惯：术语与称谓的选用。
    pub vocabulary: String,
    /// 可直接套用的惯用表述，逐条列出。
    pub common_phrases: String,
    /// 范文刻意避开的表述与风格。
    pub taboos: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DraftOutcome {
    pub session_id: String,
    pub document: ODoc,
    pub type_decision: TypeDecision,
    pub facts: FactExtraction,
    pub stages: Vec<StageSummary>,
    #[serde(default)]
    pub lint_report: Option<LintReport>,
    #[serde(default)]
    pub fact_check_report: Option<FactCheckReport>,
    #[serde(default)]
    pub score: Option<Score>,
    #[serde(default)]
    #[ts(type = "number")]
    pub revision_iterations: u8,
    #[serde(default)]
    pub warnings: Vec<StageFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum DraftEvent {
    SessionStarted {
        #[serde(rename = "sessionId")]
        #[ts(rename = "sessionId")]
        session_id: String,
    },
    StageStarted {
        #[serde(rename = "stageId")]
        #[ts(rename = "stageId")]
        stage_id: StageId,
    },
    Token {
        #[serde(rename = "stageId")]
        #[ts(rename = "stageId")]
        stage_id: StageId,
        content: String,
    },
    NeedsDocumentType {
        decision: TypeDecision,
    },
    StageCompleted {
        output: StageOutput,
    },
    StageFailed {
        output: StageOutput,
        recoverable: bool,
    },
    Completed {
        outcome: Box<DraftOutcome>,
    },
}

/// Inspection facet a semantic review finding belongs to.
///
/// The five facets mirror the checklist structure of the format pack, so the report can be
/// grouped the same way a human reviewer walks a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ReviewDimension {
    /// 格式：版头、版记、要素齐备与排列。
    Format,
    /// 内容：事项是否说清、论证是否充分。
    Content,
    /// 语言：表述是否具体、有无 AI 腔与空话。
    Language,
    /// 逻辑：层次、因果与文种纪律。
    Logic,
    /// 细节：数字、日期、称谓、标点等易错项。
    Detail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ReviewFinding {
    pub dimension: ReviewDimension,
    pub severity: Severity,
    /// `ODoc` node the finding points at, or `None` when it concerns the document as a whole.
    /// The engine clears identifiers that are absent from the reviewed document.
    #[serde(default)]
    pub node_id: Option<String>,
    pub message: String,
    /// Text quoted from the document, kept so the reader can judge the finding without hunting.
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ReviewReport {
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub missing_information: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ReviewInput {
    pub document: ODoc,
    /// Optional reviewer focus, e.g. 只看格式. Empty means a full inspection.
    #[serde(default)]
    pub focus: Option<String>,
}

/// Outcome of inspecting an existing document.
///
/// Reviewing never rewrites the document, so the outcome carries reports only and records the
/// exact revision they describe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ReviewOutcome {
    pub session_id: String,
    pub document_id: String,
    #[ts(type = "number")]
    pub revision: u32,
    /// Type declared on the document under review.
    pub declared_doc_type: DocumentType,
    /// Type the model considers correct for this content.
    pub type_decision: TypeDecision,
    pub lint_report: LintReport,
    pub fact_check_report: FactCheckReport,
    pub review_report: ReviewReport,
    pub score: Score,
    pub stages: Vec<StageSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum ReviewEvent {
    StageStarted {
        #[serde(rename = "stageId")]
        #[ts(rename = "stageId")]
        stage_id: StageId,
    },
    StageCompleted {
        output: StageOutput,
    },
    Completed {
        outcome: Box<ReviewOutcome>,
    },
}
