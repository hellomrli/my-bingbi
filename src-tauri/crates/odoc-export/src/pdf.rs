use super::{ExportError, Gb9704Style};
use odoc_model::{Block, ODoc};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

/// Finds a local LibreOffice/soffice executable without invoking a shell.
pub fn find_libreoffice() -> Option<PathBuf> {
    let candidates = if cfg!(target_os = "windows") {
        vec!["soffice.exe", "libreoffice.exe"]
    } else {
        vec!["soffice", "libreoffice"]
    };
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            for candidate in &candidates {
                let executable = directory.join(candidate);
                if executable.is_file() {
                    return Some(executable);
                }
            }
        }
    }
    let common = if cfg!(target_os = "windows") {
        vec![
            PathBuf::from(r"C:\Program Files\LibreOffice\program\soffice.exe"),
            PathBuf::from(r"C:\Program Files (x86)\LibreOffice\program\soffice.exe"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from(
            "/Applications/LibreOffice.app/Contents/MacOS/soffice",
        )]
    } else {
        vec![
            PathBuf::from("/usr/bin/libreoffice"),
            PathBuf::from("/usr/bin/soffice"),
        ]
    };
    common.into_iter().find(|path| path.is_file())
}

/// Converts DOCX bytes through a local `LibreOffice` headless process.
///
/// # Errors
///
/// Returns an error when `LibreOffice` is not installed, the input is empty or oversized, the
/// temporary workspace cannot be prepared, or the conversion produces no readable PDF.
pub fn write_pdf_with_libreoffice(docx: &[u8]) -> Result<Vec<u8>, ExportError> {
    let executable = find_libreoffice().ok_or(ExportError::LibreOfficeUnavailable)?;
    if docx.is_empty() || docx.len() > 64 * 1024 * 1024 {
        return Err(ExportError::PackageTooLarge);
    }
    let directory = tempdir().map_err(ExportError::Io)?;
    let input = directory.path().join("document.docx");
    let profile = directory.path().join("profile");
    fs::write(&input, docx).map_err(ExportError::Io)?;
    let status = Command::new(&executable)
        .arg("--headless")
        .arg("--nologo")
        .arg(format!(
            "-env:UserInstallation=file://{}",
            profile.display()
        ))
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(directory.path())
        .arg(&input)
        .status()
        .map_err(|_| ExportError::PdfConversionFailed)?;
    if !status.success() {
        return Err(ExportError::PdfConversionFailed);
    }
    let output = directory.path().join("document.pdf");
    let bytes = fs::read(output).map_err(|_| ExportError::PdfConversionFailed)?;
    if bytes.is_empty() || bytes.len() > 64 * 1024 * 1024 {
        return Err(ExportError::PdfConversionFailed);
    }
    Ok(bytes)
}

/// Produces a print-preview HTML representation used when `LibreOffice` is unavailable.
pub fn print_html(document: &ODoc, style: &Gb9704Style) -> String {
    let mut html = String::from("<!doctype html><meta charset=\"utf-8\"><style>");
    let right = style.page.width_mm - style.page.margin_left_mm - style.page.body_width_mm;
    let bottom = style.page.height_mm - style.page.margin_top_mm - style.page.body_height_mm;
    let _ = write!(
        html,
        "@page{{size:A4;margin:{}mm {}mm {}mm {}mm}}body{{font-family:'{}',serif;font-size:{}pt;line-height:{}pt;color:#111}}h1{{text-align:center;font-family:'{}';font-size:{}pt}}h2,h3,h4{{font-size:{}pt}}p{{margin:0 0 0.4em;text-indent:2em}}header{{color:#",
        style.page.margin_top_mm,
        right,
        bottom,
        style.page.margin_left_mm,
        css_string(&style.fonts.body.font),
        style.fonts.body.size_pt,
        style.page.line_spacing_pt,
        css_string(&style.fonts.title.font),
        style.fonts.title.size_pt,
        style.fonts.body.size_pt,
    );
    html.push_str(style.mark_color());
    html.push_str(";text-align:center;font-family:'");
    html.push_str(&css_string(&style.fonts.title.font));
    html.push_str("';font-size:");
    html.push_str(&style.fonts.title.size_pt.to_string());
    html.push_str("pt;border-bottom:1px solid currentColor;padding-bottom:4pt}</style>");
    if let Some(header) = &document.header {
        if !header.issuing_authority_mark.trim().is_empty() {
            html.push_str("<header>");
            html.push_str(&html_escape(&header.issuing_authority_mark));
            html.push_str("</header>");
        }
        if let Some(number) = &header.document_number {
            html.push_str("<p style=\"text-align:center;text-indent:0\">");
            html.push_str(&html_escape(number));
            html.push_str("</p>");
        }
    }
    html.push_str("<h1>");
    html.push_str(&html_escape(&document.body.title.text));
    html.push_str("</h1>");
    for recipient in &document.body.main_recipients {
        html.push_str("<p style=\"text-indent:0\">");
        html.push_str(&html_escape(recipient));
        html.push_str("</p>");
    }
    for block in &document.body.blocks {
        if let Block::Heading { content, level, .. } = block {
            let tag = match level {
                odoc_model::HeadingLevel::L1 => "h2",
                odoc_model::HeadingLevel::L2 => "h3",
                odoc_model::HeadingLevel::L3 | odoc_model::HeadingLevel::L4 => "h4",
            };
            html.push('<');
            html.push_str(tag);
            html.push('>');
            html.push_str(&html_escape(content));
            html.push_str("</");
            html.push_str(tag);
            html.push('>');
        } else {
            html.push_str("<p>");
            html.push_str(&html_escape(&block.plain_text()));
            html.push_str("</p>");
        }
    }
    if let Some(signer) = &document.body.signer {
        html.push_str("<p style=\"text-align:right;text-indent:0\">");
        html.push_str(&html_escape(&signer.authority));
        html.push_str("</p>");
    }
    if let Some(date) = &document.body.date {
        html.push_str("<p style=\"text-align:right;text-indent:0\">");
        html.push_str(&html_escape(&date.value));
        html.push_str("</p>");
    }
    html
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn css_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace(['\n', '\r'], " ")
        .replace(['<', '>'], "")
}
