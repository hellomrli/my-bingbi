use crate::error::{IpcError, map_lint_error};
use odoc_lint::{FactCheckReport, LintReport};
use odoc_model::ODoc;
use odoc_pack::PackRegistry;
use tauri::State;

/// Runs the pack-driven rule engine against the in-memory document.
///
/// The pipeline already reports lint results, but those only describe the revision the kernel
/// snapshotted. The workbench calls this while the user edits so custom pack rules stay visible
/// instead of falling back to the browser's built-in rule subset.
#[tauri::command]
pub(crate) fn lint_document_now(
    document: ODoc,
    packs: State<'_, PackRegistry>,
) -> Result<LintReport, IpcError> {
    odoc_lint::lint_document(&document, &packs).map_err(map_lint_error)
}

/// Rechecks the current ledger without a model call or a persisted draft session.
#[tauri::command]
pub(crate) fn fact_check_document_now(document: ODoc) -> Result<FactCheckReport, IpcError> {
    odoc_lint::fact_check_document(&document).map_err(map_lint_error)
}
