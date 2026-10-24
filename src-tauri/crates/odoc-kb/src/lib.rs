//! Safe knowledge-base providers for local Obsidian vaults and FNS.
//!
//! The provider boundary deliberately contains no `ODoc` or UI state.  Search results carry their
//! source path and frontmatter so the engine can attach provenance without copying credentials
//! or entire notes into frontend state.

mod fns;
mod fns_watch;
mod hybrid;
mod local;
mod mcp;
mod security;
mod tagging;
mod types;

pub use fns::FnsProvider;
pub use fns_watch::{FnsPendingChange, FnsWatchConfig, FnsWatchEvent, watch_fns_notes};
pub use hybrid::{hybrid_search, hybrid_search_with_semantic};
pub use local::{LocalVaultProvider, parse_markdown_note};
pub use mcp::McpProvider;
pub use security::source_security_level;
pub use tagging::suggest_missing_frontmatter;
pub use types::{
    ChangeEvent, ChangeKind, Frontmatter, KbConfig, KbError, KbHit, KbNote, KbProviderKind,
    KbQuery, SearchMode,
};

use async_trait::async_trait;
use std::collections::BTreeMap;
use tokio::sync::broadcast;

/// Common provider contract used by the drafting engine and future UI adapters.
#[async_trait]
pub trait KbProvider: Send + Sync {
    /// Searches notes using provider-local full-text/path capabilities.
    async fn search(&self, query: &KbQuery) -> Result<Vec<KbHit>, KbError>;

    /// Loads one note by its vault-relative path.
    async fn get(&self, path: &str) -> Result<KbNote, KbError>;

    /// Returns paths that link to the requested note.
    async fn backlinks(&self, path: &str) -> Result<Vec<String>, KbError>;

    /// Returns paths linked from the requested note.
    async fn outlinks(&self, path: &str) -> Result<Vec<String>, KbError>;

    /// Writes a note for archive workflows.
    async fn write(&self, path: &str, content: &str) -> Result<(), KbError>;

    /// Merges scalar frontmatter fields without exposing provider credentials.
    async fn patch_frontmatter(
        &self,
        path: &str,
        fields: &BTreeMap<String, String>,
    ) -> Result<(), KbError>;

    /// Lists bounded note metadata/content for local indexing. Providers may implement a more
    /// efficient endpoint; the default uses the provider's search and get contract.
    async fn list_notes(
        &self,
        path_prefix: Option<&str>,
        limit: u32,
    ) -> Result<Vec<KbNote>, KbError> {
        let query = KbQuery {
            keyword: String::new(),
            path_prefix: path_prefix.map(str::to_owned),
            limit,
            ..KbQuery::default()
        };
        let hits = self.search(&query).await?;
        let mut notes = Vec::with_capacity(hits.len());
        for hit in hits {
            match self.get(&hit.path).await {
                Ok(note) => notes.push(note),
                Err(KbError::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(notes)
    }

    /// Subscribes to provider-local change notifications when supported.
    fn watch(&self) -> Option<broadcast::Receiver<ChangeEvent>>;
}

/// Emits frontend declarations owned by this crate.
pub fn typescript_bindings() -> String {
    use ts_rs::TS;

    let declarations = [
        KbProviderKind::decl(),
        KbConfig::decl(),
        SearchMode::decl(),
        KbQuery::decl(),
        KbHit::decl(),
        KbNote::decl(),
        ChangeKind::decl(),
        ChangeEvent::decl(),
    ];
    format!(
        "// This file is generated from src-tauri/crates/odoc-kb. Do not edit by hand.\n\nexport {}\n",
        declarations.join("\n\nexport ")
    )
}
