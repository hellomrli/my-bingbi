//! Document CRUD, versions, diffing, restore, import, and reference extraction.

use crate::dto::{DroppedReference, RestoreVersionResult, VersionDiff};
use crate::error::{IpcError, map_import_error, map_reference_error, map_store_error};
use crate::state::AppState;
use crate::util::{MAX_DRAFT_MATERIAL_BYTES, lock_documents};
use odoc_export::{extract_reference_text as parse_reference_text, read_docx};
use odoc_model::ODoc;
use odoc_store::{DocumentSummary, DocumentVersionSummary, SaveDocumentResult};
use std::path::Path;
use tauri::State;

const MAX_REFERENCE_FILE_BYTES: usize = 64 * 1024 * 1024;
const TEXT_REFERENCE_EXTENSIONS: &[&str] = &["txt", "md", "markdown", "csv", "json", "yaml", "yml"];

#[tauri::command]
pub(crate) fn create_empty_document() -> ODoc {
    ODoc::empty()
}

#[tauri::command]
pub(crate) fn backend_version() -> &'static str {
    concat!("rust-core ", env!("CARGO_PKG_VERSION"))
}

#[tauri::command]
pub(crate) fn save_document(
    state: State<'_, AppState>,
    document: ODoc,
    stage: Option<String>,
) -> Result<SaveDocumentResult, IpcError> {
    let mut store = lock_documents(&state)?;
    store
        .save_document(&document, stage.as_deref())
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn load_document(state: State<'_, AppState>, id: String) -> Result<ODoc, IpcError> {
    lock_documents(&state)?
        .get_document(&id)
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn list_documents(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<DocumentSummary>, IpcError> {
    lock_documents(&state)?
        .list_documents(limit.unwrap_or(30))
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn list_sessions(
    state: State<'_, AppState>,
    document_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<odoc_store::SessionSummary>, IpcError> {
    let limit = limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(IpcError {
            code: "invalid_input",
            message: "起草记录数量范围无效。",
        });
    }
    lock_documents(&state)?
        .list_sessions(document_id.as_deref(), limit)
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn list_stage_runs(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<odoc_store::StageRunRecord>, IpcError> {
    lock_documents(&state)?
        .list_stage_runs(&session_id)
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn delete_document(state: State<'_, AppState>, id: String) -> Result<bool, IpcError> {
    lock_documents(&state)?
        .delete_document(&id)
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn list_document_versions(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<Vec<DocumentVersionSummary>, IpcError> {
    lock_documents(&state)?
        .list_document_versions(&document_id)
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn load_document_version(
    state: State<'_, AppState>,
    document_id: String,
    seq: u32,
) -> Result<ODoc, IpcError> {
    lock_documents(&state)?
        .get_document_version(&document_id, seq)
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn restore_document_version(
    state: State<'_, AppState>,
    document_id: String,
    seq: u32,
) -> Result<RestoreVersionResult, IpcError> {
    let mut store = lock_documents(&state)?;
    let current = store.get_document(&document_id).map_err(map_store_error)?;
    let mut restored = store
        .get_document_version(&document_id, seq)
        .map_err(map_store_error)?;
    restored.id = current.id;
    restored.revision = current.revision.saturating_add(1);
    let saved = store
        .save_document(&restored, Some(&format!("restore:{seq}")))
        .map_err(map_store_error)?;
    Ok(RestoreVersionResult {
        document: restored,
        saved,
    })
}

#[tauri::command(async)]
pub(crate) fn diff_document_versions(
    state: State<'_, AppState>,
    document_id: String,
    left_seq: u32,
    right_seq: u32,
) -> Result<VersionDiff, IpcError> {
    let store = lock_documents(&state)?;
    let left = store
        .get_document_version(&document_id, left_seq)
        .map_err(map_store_error)?;
    let right = store
        .get_document_version(&document_id, right_seq)
        .map_err(map_store_error)?;
    let left_text = left.to_plain_text();
    let right_text = right.to_plain_text();
    let (removed_lines, added_lines) = changed_line_counts(&left_text, &right_text);
    Ok(VersionDiff {
        document_id,
        left_seq,
        right_seq,
        left_text,
        right_text,
        added_lines,
        removed_lines,
    })
}

#[tauri::command(async)]
pub(crate) fn import_docx(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
) -> Result<RestoreVersionResult, IpcError> {
    let document = read_docx(&bytes).map_err(map_import_error)?;
    let saved = lock_documents(&state)?
        .save_document(&document, Some("docx_import"))
        .map_err(map_store_error)?;
    Ok(RestoreVersionResult { document, saved })
}

/// Extracts semantic text from a DOCX reference without saving or changing the current document.
///
/// This is intentionally separate from `import_docx`: an uploaded reference is only input to a
/// later drafting run and must not appear in the document library as an imported draft.
#[tauri::command(async)]
pub(crate) fn extract_docx_text(bytes: Vec<u8>) -> Result<String, IpcError> {
    extract_reference_text("reference.docx".to_owned(), bytes)
}

/// Extracts bounded plain text from a binary drafting reference without importing it.
#[tauri::command(async)]
pub(crate) fn extract_reference_text(
    file_name: String,
    bytes: Vec<u8>,
) -> Result<String, IpcError> {
    if bytes.is_empty() || bytes.len() > MAX_REFERENCE_FILE_BYTES {
        return Err(IpcError {
            code: "reference_too_large",
            message: "参考文件为空或超过 64 MiB 限制。",
        });
    }
    if file_name.trim().is_empty()
        || file_name.len() > 512
        || file_name.contains(['\0', '\r', '\n'])
    {
        return Err(IpcError {
            code: "reference_invalid",
            message: "参考文件名或文件结构无效，无法读取。",
        });
    }
    let text = parse_reference_text(&file_name, &bytes).map_err(map_reference_error)?;
    if text.trim().is_empty() || text.len() > MAX_DRAFT_MATERIAL_BYTES {
        return Err(IpcError {
            code: "reference_too_large",
            message: "参考文件提取后的文字为空或超过起草材料 2 MiB 限制。",
        });
    }
    Ok(text)
}

/// Reads a file path supplied by a native window drop event and extracts bounded text.
///
/// Browsers cannot read arbitrary native paths, so Tauri forwards dropped paths instead of
/// `File` objects. Text formats are decoded in the kernel (UTF-8/UTF-16/GB18030); Office and
/// PDF formats use the same bounded extractor as webview uploads.
#[tauri::command(async)]
pub(crate) fn read_dropped_reference(path: String) -> Result<DroppedReference, IpcError> {
    let path = Path::new(&path);
    let metadata = std::fs::metadata(path).map_err(|_| IpcError {
        code: "reference_unreadable",
        message: "无法读取拖入的参考文件，请确认文件仍然存在。",
    })?;
    if !metadata.is_file() {
        return Err(IpcError {
            code: "reference_invalid",
            message: "拖入的内容不是文件。",
        });
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| {
            !name.trim().is_empty() && name.len() <= 512 && !name.contains(['\0', '\r', '\n'])
        })
        .map(str::to_owned)
        .ok_or(IpcError {
            code: "reference_invalid",
            message: "参考文件名无效。",
        })?;
    let extension = file_name
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension)
        .to_ascii_lowercase();
    let text_extension = TEXT_REFERENCE_EXTENSIONS.contains(&extension.as_str());
    let binary_extension = matches!(
        extension.as_str(),
        "pdf" | "ofd" | "doc" | "docx" | "xlsx" | "xls" | "wps"
    );
    if !text_extension && !binary_extension {
        return Err(IpcError {
            code: "reference_invalid",
            message: "暂不支持这种参考文件格式。",
        });
    }
    if metadata.len() == 0 || metadata.len() > MAX_REFERENCE_FILE_BYTES as u64 {
        return Err(IpcError {
            code: "reference_too_large",
            message: "参考文件为空或超过 64 MiB 限制。",
        });
    }
    let bytes = std::fs::read(path).map_err(|_| IpcError {
        code: "reference_unreadable",
        message: "无法读取拖入的参考文件，请检查访问权限。",
    })?;
    if bytes.is_empty() || bytes.len() > MAX_REFERENCE_FILE_BYTES {
        return Err(IpcError {
            code: "reference_too_large",
            message: "参考文件为空或超过 64 MiB 限制。",
        });
    }
    let text = if text_extension {
        decode_dropped_text(&bytes)
    } else {
        parse_reference_text(&file_name, &bytes).map_err(map_reference_error)?
    };
    if text.trim().is_empty() || text.len() > MAX_DRAFT_MATERIAL_BYTES {
        return Err(IpcError {
            code: "reference_too_large",
            message: "参考文件提取后的文字为空或超过起草材料 2 MiB 限制。",
        });
    }
    Ok(DroppedReference { file_name, text })
}

fn decode_dropped_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] == 0xfe {
        return encoding_rs::UTF_16LE.decode(&bytes[2..]).0.into_owned();
    }
    if bytes.len() >= 2 && bytes[0] == 0xfe && bytes[1] == 0xff {
        return encoding_rs::UTF_16BE.decode(&bytes[2..]).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        // Windows-produced TXT/CSV files in Chinese offices are commonly GBK/GB18030.
        Err(_) => encoding_rs::GB18030.decode(bytes).0.into_owned(),
    }
}

/// Beyond this many lines per side the LCS becomes too slow to run synchronously and the
/// counts degrade to "everything changed" (the text diff itself is unaffected).
const MAX_DIFF_LCS_LINES: usize = 20_000;

fn changed_line_counts(left: &str, right: &str) -> (u32, u32) {
    let left = left.lines().collect::<Vec<_>>();
    let right = right.lines().collect::<Vec<_>>();
    let common = if left.len() > MAX_DIFF_LCS_LINES || right.len() > MAX_DIFF_LCS_LINES {
        // Conservative fallback: no common lines assumed, so no edit is ever under-counted.
        0
    } else {
        longest_common_subsequence_len(&left, &right)
    };
    let removed = left.len().saturating_sub(common);
    let added = right.len().saturating_sub(common);
    (
        u32::try_from(removed).unwrap_or(u32::MAX),
        u32::try_from(added).unwrap_or(u32::MAX),
    )
}

fn longest_common_subsequence_len(left: &[&str], right: &[&str]) -> usize {
    let mut previous = vec![0_usize; right.len().saturating_add(1)];
    let mut current = previous.clone();
    for left_line in left {
        for (index, right_line) in right.iter().enumerate() {
            current[index + 1] = if left_line == right_line {
                previous[index].saturating_add(1)
            } else {
                current[index].max(previous[index + 1])
            };
        }
        std::mem::swap(&mut previous, &mut current);
        current.fill(0);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_export::ExportError;

    #[test]
    fn creates_an_empty_document_for_ipc() {
        let document = create_empty_document();
        assert_eq!(document.schema_version, odoc_model::ODOC_SCHEMA_VERSION);
        assert!(document.body.blocks.len() == 1);
    }

    #[test]
    fn reports_the_backend_version() {
        assert!(backend_version().starts_with("rust-core "));
    }

    #[test]
    fn reads_dropped_text_references_and_rejects_unknown_formats() {
        let directory = tempfile::tempdir().expect("temp directory");
        let text_path = directory.path().join("检查材料.txt");
        std::fs::write(&text_path, "一、值班值守\n二、防汛物资").expect("write text");
        let dropped = read_dropped_reference(text_path.to_string_lossy().into_owned())
            .expect("text reference should read");
        assert_eq!(dropped.file_name, "检查材料.txt");
        assert!(dropped.text.contains("值班值守"));

        let unsupported = directory.path().join("notes.exe");
        std::fs::write(&unsupported, b"not a reference").expect("write unsupported");
        let error = read_dropped_reference(unsupported.to_string_lossy().into_owned())
            .expect_err("unsupported format should fail");
        assert_eq!(error.code, "reference_invalid");
    }

    #[test]
    fn reference_upload_stays_outside_document_import_and_has_stable_errors() {
        let error = extract_docx_text(Vec::new()).expect_err("empty reference should fail");
        assert_eq!(error.code, "reference_too_large");
        assert_eq!(
            map_reference_error(ExportError::InvalidXml).code,
            "reference_invalid"
        );
        assert_eq!(
            map_reference_error(ExportError::PackageTooLarge).code,
            "reference_too_large"
        );
    }
}
