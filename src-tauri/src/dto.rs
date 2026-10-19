//! Request and response payloads serialized across the IPC boundary.

use odoc_model::{DocumentType, ODoc};
use odoc_store::SaveDocumentResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DraftStartRequest {
    pub(crate) document_id: String,
    pub(crate) material: String,
    pub(crate) provider_id: String,
    pub(crate) confirmed_doc_type: Option<DocumentType>,
    /// Reference text for imitate mode; its presence selects the imitate pipeline.
    #[serde(default)]
    pub(crate) model_text: Option<String>,
    /// Knowledge-pack template variant, e.g. `notice-meeting`; absent means the per-type default.
    #[serde(default)]
    pub(crate) template_id: Option<String>,
    /// Polish mode rewrites the supplied material in place.
    #[serde(default)]
    pub(crate) polish: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReviewStartRequest {
    pub(crate) document_id: String,
    pub(crate) provider_id: String,
    /// Optional reviewer focus, e.g. 只看格式.
    #[serde(default)]
    pub(crate) focus: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RerunStageRequest {
    pub(crate) session_id: String,
    pub(crate) stage_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KbIndexProgress {
    pub(crate) status: String,
    pub(crate) processed: u32,
    pub(crate) total: u32,
    pub(crate) path: Option<String>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArchiveResult {
    pub(crate) path: String,
    pub(crate) vault: String,
    pub(crate) overwritten: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionDiff {
    pub(crate) document_id: String,
    pub(crate) left_seq: u32,
    pub(crate) right_seq: u32,
    pub(crate) left_text: String,
    pub(crate) right_text: String,
    pub(crate) added_lines: u32,
    pub(crate) removed_lines: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestoreVersionResult {
    pub(crate) document: ODoc,
    pub(crate) saved: SaveDocumentResult,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DocxExportResult {
    pub(crate) bytes: Vec<u8>,
    pub(crate) used_template: bool,
    pub(crate) fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PdfExportResult {
    Pdf { bytes: Vec<u8> },
    PrintPreview { html: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportCapabilities {
    pub(crate) custom_template_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateCheckResult {
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) current_version: String,
    pub(crate) installed: bool,
}

/// Extracted text and original file name for a file dropped onto the native window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DroppedReference {
    pub(crate) file_name: String,
    pub(crate) text: String,
}
