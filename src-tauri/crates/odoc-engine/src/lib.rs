mod error;
mod pipeline;
mod review;
mod types;

pub use error::EngineError;
pub use pipeline::{DraftEngine, DraftJournal, NoopJournal, PipelineDefinition};
pub use review::ReviewEngine;
pub use types::{
    DraftEvent, DraftInput, DraftOutcome, FactExtraction, FactInput, LoopBackConfig,
    LowConfidenceAction, PipelineKind, RerunOutcome, ReviewDimension, ReviewEvent, ReviewFinding,
    ReviewInput, ReviewOutcome, ReviewReport, Score, ScoreDimension, StageConfig, StageKind,
    StageOutput, StageStatus, StageSummary, TypeDecision,
};

pub fn typescript_bindings() -> String {
    use ts_rs::TS;

    let declarations = [
        StageKind::decl(),
        StageStatus::decl(),
        PipelineKind::decl(),
        LowConfidenceAction::decl(),
        StageConfig::decl(),
        LoopBackConfig::decl(),
        TypeDecision::decl(),
        FactInput::decl(),
        FactExtraction::decl(),
        ScoreDimension::decl(),
        Score::decl(),
        StageOutput::decl(),
        StageSummary::decl(),
        RerunOutcome::decl(),
        DraftInput::decl(),
        DraftOutcome::decl(),
        DraftEvent::decl(),
        ReviewDimension::decl(),
        ReviewFinding::decl(),
        ReviewReport::decl(),
        ReviewInput::decl(),
        ReviewOutcome::decl(),
        ReviewEvent::decl(),
    ];
    format!(
        "// This file is generated from src-tauri/crates/odoc-engine. Do not edit by hand.\n\nimport type {{ DocumentType, FactKind, ODoc }} from \"./bindings\";\nimport type {{ TokenUsage }} from \"./llm\";\nimport type {{ StageId }} from \"./pack\";\nimport type {{ FactCheckReport, LintReport, Severity }} from \"./lint\";\n\nexport {}\n",
        declarations.join("\n\nexport ")
    )
}
