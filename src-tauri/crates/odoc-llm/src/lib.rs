mod decoder;
pub use decoder::parse_model_json;
mod error;
mod provider;
mod types;

pub use error::LlmError;
pub use odoc_model::SecurityLevel;
pub use provider::{LlmProvider, ProviderClient};
pub use types::{
    Capabilities, ChatDelta, ChatMessage, ChatRequest, ChatResponse, ChatRole,
    ConnectionTestResult, ProviderKind, ProviderProfile, ProviderProfileInput, TokenUsage,
    builtin_provider_profiles,
};

/// Emits the frontend declarations owned by this crate.
pub fn typescript_bindings() -> String {
    use ts_rs::TS;

    let declarations = [
        ProviderKind::decl(),
        ProviderProfileInput::decl(),
        ProviderProfile::decl(),
        ChatRole::decl(),
        ChatMessage::decl(),
        ChatRequest::decl(),
        TokenUsage::decl(),
        ChatResponse::decl(),
        ChatDelta::decl(),
        Capabilities::decl(),
        ConnectionTestResult::decl(),
    ];
    format!(
        "// This file is generated from src-tauri/crates/odoc-llm. Do not edit by hand.\n\nimport type {{ SecurityLevel }} from \"./bindings\";\n\nexport {}\n",
        declarations.join("\n\nexport ")
    )
}
