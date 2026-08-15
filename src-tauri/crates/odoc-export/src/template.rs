use super::{ExportError, Gb9704Style, document_xml, validate_document};
use odoc_model::ODoc;
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const MAX_TEMPLATE_BYTES: usize = 64 * 1024 * 1024;
const BODY_MARKERS: [&str; 3] = ["BINGBI_BODY", "{{BINGBI_BODY}}", "{{ bingbi_body }}"];

/// Validates the package and confirms that its main document has an injectable body marker.
///
/// # Errors
///
/// Returns an error when the package is empty or oversized, is not a readable ZIP, lacks
/// `word/document.xml`, or has no usable `BINGBI_BODY` marker paragraph.
pub fn validate_docx_template(template_bytes: &[u8]) -> Result<(), ExportError> {
    if template_bytes.is_empty() || template_bytes.len() > MAX_TEMPLATE_BYTES {
        return Err(ExportError::PackageTooLarge);
    }
    let mut input =
        ZipArchive::new(Cursor::new(template_bytes)).map_err(|_| ExportError::InvalidXml)?;
    let document = input
        .by_name("word/document.xml")
        .map_err(|_| ExportError::MissingPart)?;
    let mut contents = String::new();
    document
        .take(MAX_TEMPLATE_BYTES as u64 + 1)
        .read_to_string(&mut contents)
        .map_err(ExportError::Io)?;
    if contents.len() > MAX_TEMPLATE_BYTES {
        return Err(ExportError::PackageTooLarge);
    }
    let marker = BODY_MARKERS
        .iter()
        .find(|marker| contents.contains(**marker))
        .copied()
        .ok_or(ExportError::TemplateMarkerMissing)?;
    replace_marker_paragraph(&contents, marker, "")
        .map(|_| ())
        .ok_or(ExportError::TemplateMarkerMissing)
}

/// Injects generated `ODoc` content into a user-authored DOCX template.
///
/// The template must contain a plain-text `BINGBI_BODY` marker in `word/document.xml`. All other
/// package parts, including custom headers, page geometry and organization-specific styles, are
/// preserved byte-for-byte at the XML content level.
///
/// # Errors
///
/// Returns an error when the document or style fails validation, the template is unusable, a
/// package part is oversized or has an unsafe name, or the output package cannot be written.
pub fn write_docx_with_template(
    document: &ODoc,
    template_bytes: &[u8],
    style: &Gb9704Style,
) -> Result<Vec<u8>, ExportError> {
    validate_document(document)?;
    style.validate()?;
    validate_docx_template(template_bytes)?;
    let mut input =
        ZipArchive::new(Cursor::new(template_bytes)).map_err(|_| ExportError::InvalidXml)?;
    let mut parts = Vec::new();
    let mut found_marker = false;
    let mut expanded_bytes = 0_usize;
    for index in 0..input.len() {
        let file = input.by_index(index).map_err(|_| ExportError::InvalidXml)?;
        if file.name().contains("..") || file.name().starts_with('/') {
            return Err(ExportError::InvalidXml);
        }
        let name = file.name().to_owned();
        if file.is_dir() {
            continue;
        }
        let mut contents = Vec::new();
        file.take(MAX_TEMPLATE_BYTES as u64 + 1)
            .read_to_end(&mut contents)
            .map_err(ExportError::Io)?;
        expanded_bytes = expanded_bytes.saturating_add(contents.len());
        if contents.len() > MAX_TEMPLATE_BYTES || expanded_bytes > MAX_TEMPLATE_BYTES {
            return Err(ExportError::PackageTooLarge);
        }
        if name == "word/document.xml" {
            let source = String::from_utf8(contents).map_err(|_| ExportError::InvalidXml)?;
            let marker = BODY_MARKERS
                .iter()
                .find(|marker| source.contains(**marker))
                .copied()
                .ok_or(ExportError::TemplateMarkerMissing)?;
            let generated = document_xml(document, style);
            let fragment = body_fragment(&generated).ok_or(ExportError::InvalidXml)?;
            let replacement = replace_marker_paragraph(&source, marker, &fragment)
                .ok_or(ExportError::TemplateMarkerMissing)?;
            found_marker = true;
            contents = replacement.into_bytes();
        }
        parts.push((name, contents));
    }
    if !found_marker {
        return Err(ExportError::TemplateMarkerMissing);
    }
    let mut output = Cursor::new(Vec::new());
    {
        let mut archive = ZipWriter::new(&mut output);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, contents) in parts {
            archive.start_file(name, options)?;
            archive.write_all(&contents)?;
        }
        archive.finish()?;
    }
    Ok(output.into_inner())
}

fn body_fragment(document_xml: &str) -> Option<String> {
    let body_start = document_xml.find("<w:body>")? + "<w:body>".len();
    let sect_start = document_xml.find("<w:sectPr")?;
    Some(document_xml[body_start..sect_start].trim().to_owned())
}

fn replace_marker_paragraph(source: &str, marker: &str, fragment: &str) -> Option<String> {
    let marker_position = source.find(marker)?;
    // `MatchIndices` for a `&str` pattern is not double-ended, so this walks forward and keeps
    // the last match rather than stepping back from the end.
    let paragraph_start = source[..marker_position]
        .match_indices("<w:p")
        .filter(|(index, _)| {
            source
                .as_bytes()
                .get(index + 4)
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>')
        })
        .map(|(index, _)| index)
        .last()?;
    let paragraph_end =
        source[marker_position..].find("</w:p>")? + marker_position + "</w:p>".len();
    let mut result = String::with_capacity(source.len().saturating_add(fragment.len()));
    result.push_str(&source[..paragraph_start]);
    result.push_str(fragment);
    result.push_str(&source[paragraph_end..]);
    Some(result)
}
