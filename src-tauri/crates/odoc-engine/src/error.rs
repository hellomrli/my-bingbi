use crate::TypeDecision;
use odoc_pack::StageId;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("pipeline configuration is invalid: {0}")]
    InvalidPipeline(String),
    #[error("pipeline resource is unavailable")]
    PipelineResource,
    #[error("model output is invalid for stage {stage:?}: {reason}")]
    InvalidModelOutput {
        stage: StageId,
        reason: &'static str,
    },
    #[error("required knowledge-pack resource is unavailable")]
    PackResource,
    #[error("draft input is invalid")]
    InvalidInput,
    #[error("document type must be confirmed by the user")]
    LowConfidence(TypeDecision),
    #[error("draft was cancelled")]
    Cancelled,
    #[error("draft journal is unavailable")]
    Journal,
    #[error("draft session or stage run was not found")]
    SessionNotFound,
    #[error("deterministic lint failed: {0}")]
    Lint(#[from] odoc_lint::LintError),
    #[error("knowledge-base retrieval failed: {0}")]
    KnowledgeBase(#[from] odoc_kb::KbError),
    #[error("model request failed: {0}")]
    Llm(#[from] odoc_llm::LlmError),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl EngineError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidPipeline(_) | Self::PipelineResource => "pipeline_unavailable",
            Self::InvalidModelOutput { .. } => "invalid_model_output",
            Self::PackResource => "pack_unavailable",
            Self::InvalidInput => "invalid_input",
            Self::LowConfidence(_) => "document_type_confirmation_required",
            Self::Cancelled => "draft_cancelled",
            Self::Journal => "store_unavailable",
            Self::SessionNotFound => "session_not_found",
            Self::Lint(_) => "lint_unavailable",
            Self::KnowledgeBase(error) => error.code(),
            Self::Llm(error) => error.code(),
            Self::Serialization(_) => "serialization_failed",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::InvalidPipeline(_) | Self::PipelineResource => "起草流程配置不可用。",
            Self::InvalidModelOutput { .. } => "模型返回的数据格式无效，请重试或更换模型。",
            Self::PackResource => "起草所需的知识包资源不可用。",
            Self::InvalidInput => "起草材料格式无效。",
            Self::LowConfidence(_) => "材料不足以确定文种，请先确认文种。",
            Self::Cancelled => "起草已取消。",
            Self::Journal => "本地起草记录暂不可用。",
            Self::SessionNotFound => "未找到对应的起草记录或该阶段尚未运行。",
            Self::Lint(_) => "确定性质检暂不可用。",
            Self::KnowledgeBase(error) => error.user_message(),
            Self::Llm(error) => error.user_message(),
            Self::Serialization(_) => "起草数据无法序列化。",
        }
    }
}
