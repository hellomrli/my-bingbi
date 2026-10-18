mod error;
mod loader;
mod types;

pub use error::PackError;
pub use loader::{LoadedPack, PackRegistry};
pub use types::{
    DocumentTemplate, PACK_SCHEMA_VERSION, PackCapabilities, PackCapability, PackManifest,
    PackMetadata, PackSummary, RegexMatchMode, ResolvedResource, ResourceRole, RuleDefinition,
    RuleSeverity, RuleType, StageBinding, StageId, StructureDetector, TemplateBlock,
    TemplateRequirements, TemplateSummary,
};

/// Emits the frontend declarations owned by this crate.
pub fn typescript_bindings() -> String {
    use ts_rs::TS;

    let declarations = [
        PackCapability::decl(),
        StageId::decl(),
        PackSummary::decl(),
        TemplateSummary::decl(),
    ];
    format!(
        "// This file is generated from src-tauri/crates/odoc-pack. Do not edit by hand.\n\nimport type {{ DocumentType }} from \"./bindings\";\n\nexport {}\n",
        declarations.join("\n\nexport ")
    )
}
