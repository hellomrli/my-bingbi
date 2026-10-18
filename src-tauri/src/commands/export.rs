//! DOCX and PDF export, export capabilities, and template management.

use crate::dto::{DocxExportResult, ExportCapabilities, PdfExportResult};
use crate::error::{
    IpcError, map_export_error, map_pdf_error, map_store_error, map_template_error,
};
use crate::state::AppState;
use crate::util::{ensure_exportable, export_style, lock_documents};
use odoc_export::{
    Gb9704Style, discover_system_fonts, odoc_to_typst, print_html, render_typst_pdf,
    resolve_font_path, validate_docx_template, write_docx_with_style, write_docx_with_template,
};
use odoc_pack::PackRegistry;
use std::path::PathBuf;
use tauri::State;

/// Exports the current stored ODoc after a fresh kernel lint and fact check.
///
/// The frontend sends only an identifier.  Reading the document and its security-sensitive
/// facts here prevents a stale or browser-modified payload from bypassing export checks.
#[tauri::command(async)]
pub(crate) fn export_docx(
    state: State<'_, AppState>,
    packs: State<'_, PackRegistry>,
    document_id: String,
) -> Result<DocxExportResult, IpcError> {
    let document = lock_documents(&state)?
        .get_document(&document_id)
        .map_err(map_store_error)?;
    ensure_exportable(&document, &packs)?;
    let style = export_style(&state, &packs)?;
    let template = lock_documents(&state)?
        .get_setting::<Vec<u8>>("export.docx_template")
        .map_err(map_store_error)?;
    if let Some(template) = template {
        match write_docx_with_template(&document, &template, &style) {
            Ok(bytes) => {
                return Ok(DocxExportResult {
                    bytes,
                    used_template: true,
                    fallback_reason: None,
                });
            }
            Err(_) => {
                let bytes = write_docx_with_style(&document, &style).map_err(map_export_error)?;
                return Ok(DocxExportResult {
                    bytes,
                    used_template: false,
                    fallback_reason: Some(
                        "自定义模板不可用，已回退到 GB/T 9704 严格生成。".to_owned(),
                    ),
                });
            }
        }
    }
    Ok(DocxExportResult {
        bytes: write_docx_with_style(&document, &style).map_err(map_export_error)?,
        used_template: false,
        fallback_reason: None,
    })
}

#[tauri::command(async)]
pub(crate) fn export_pdf(
    state: State<'_, AppState>,
    packs: State<'_, PackRegistry>,
    document_id: String,
) -> Result<PdfExportResult, IpcError> {
    let document = lock_documents(&state)?
        .get_document(&document_id)
        .map_err(map_store_error)?;
    ensure_exportable(&document, &packs)?;
    let style = export_style(&state, &packs)?;

    // Zero-dependency PDF: render the document through the embedded Typst
    // engine using fonts discovered on the host system. When no usable CJK
    // font is available, fall back to the browser print preview (which does not
    // depend on any external binary either).
    let fonts = typst_fonts(&style);
    if fonts.is_empty() {
        return Ok(PdfExportResult::PrintPreview {
            html: print_html(&document, &style),
        });
    }
    let source = odoc_to_typst(&document, &style);
    let bytes = render_typst_pdf(&source, &fonts).map_err(map_pdf_error)?;
    Ok(PdfExportResult::Pdf { bytes })
}

/// Maps every distinct font family referenced by `style` to a discovered font
/// file on the host system. Families that cannot be resolved fall back to the
/// CJK serif font; an empty result means no usable font exists at all.
fn typst_fonts(style: &Gb9704Style) -> Vec<(String, PathBuf)> {
    let system_fonts = discover_system_fonts();
    let families = [
        &style.fonts.title,
        &style.fonts.body,
        &style.fonts.heading_l1,
        &style.fonts.heading_l2,
        &style.fonts.heading_l3,
        &style.fonts.heading_l4,
        &style.fonts.document_number,
    ];
    let mut seen = std::collections::HashSet::new();
    let mut fonts = Vec::new();
    for family in families {
        if !seen.insert(family.font.as_str()) {
            continue;
        }
        if let Some(path) = resolve_font_path(&family.font, &system_fonts) {
            fonts.push((family.font.clone(), path));
        }
    }
    fonts
}

#[tauri::command]
pub(crate) fn export_capabilities(
    state: State<'_, AppState>,
) -> Result<ExportCapabilities, IpcError> {
    let custom_template_configured = lock_documents(&state)?
        .get_setting::<Vec<u8>>("export.docx_template")
        .map_err(map_store_error)?
        .is_some();
    Ok(ExportCapabilities {
        custom_template_configured,
    })
}

#[tauri::command(async)]
pub(crate) fn save_docx_template(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
) -> Result<(), IpcError> {
    validate_docx_template(&bytes).map_err(map_template_error)?;
    lock_documents(&state)?
        .set_setting("export.docx_template", &bytes)
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn delete_docx_template(state: State<'_, AppState>) -> Result<(), IpcError> {
    lock_documents(&state)?
        .delete_setting("export.docx_template")
        .map(|_| ())
        .map_err(map_store_error)
}
