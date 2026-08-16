//! Deterministic OOXML import/export for the canonical [`ODoc`] model.
//!
//! The exporter intentionally keeps the package small and dependency-light.  It emits the
//! parts required by Word and `LibreOffice`, applies the GB/T 9704 page geometry, and preserves
//! the semantic structure that can be represented by the first `ODoc` schema.

mod import;
mod pdf;
mod reference;
mod style;
mod template;

pub use import::read_docx;
pub use pdf::{find_libreoffice, print_html, write_pdf_with_libreoffice};
pub use reference::extract_reference_text;
pub use style::{FontStyle, FontStyles, Gb9704Style, HeaderStyle, PageStyle, StyleError};
pub use template::{validate_docx_template, write_docx_with_template};

use odoc_model::{Block, HeadingLevel, ListStyle, ODoc};
use std::fmt::Write as _;
use std::io::{Cursor, Write};
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// A4/GB/T 9704 geometry expressed in twips (1/20 point).
pub mod gb9704 {
    /// A4 page width (210 mm).
    pub const PAGE_WIDTH_TWIPS: u32 = 11_906;
    /// A4 page height (297 mm).
    pub const PAGE_HEIGHT_TWIPS: u32 = 16_838;
    /// Top margin / head space (about 37 mm).
    pub const MARGIN_TOP_TWIPS: u32 = 2_098;
    /// Binding-side margin (about 28 mm).
    pub const MARGIN_LEFT_TWIPS: u32 = 1_587;
    /// Right margin, leaving a 156 mm text area with the binding margin.
    pub const MARGIN_RIGHT_TWIPS: u32 = 1_474;
    /// Bottom margin, leaving a 225 mm text area.
    pub const MARGIN_BOTTOM_TWIPS: u32 = 1_984;
    /// Fixed body line spacing (28 pt).
    pub const LINE_SPACING_TWIPS: u32 = 560;
    /// Title size (2 号, 22 pt) in half-points.
    pub const TITLE_SIZE_HALF_POINTS: u32 = 44;
    /// Body size (3 号, 16 pt) in half-points.
    pub const BODY_SIZE_HALF_POINTS: u32 = 32;
}

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("document cannot be exported: {0}")]
    InvalidDocument(&'static str),
    #[error("docx package could not be written")]
    Zip(#[from] zip::result::ZipError),
    #[error("docx package could not be finalized")]
    Io(#[from] std::io::Error),
    #[error("style configuration is invalid")]
    Style(#[from] StyleError),
    #[error("docx XML could not be parsed")]
    InvalidXml,
    #[error("docx package is missing a required part")]
    MissingPart,
    #[error("docx template does not contain the BINGBI_BODY marker")]
    TemplateMarkerMissing,
    #[error("docx package exceeds the configured safety limit")]
    PackageTooLarge,
    #[error("reference file format is not supported")]
    UnsupportedReferenceFormat,
    #[error("reference file structure is invalid or contains no readable text")]
    InvalidReference,
    #[error("LibreOffice is not available")]
    LibreOfficeUnavailable,
    #[error("LibreOffice could not convert the document")]
    PdfConversionFailed,
}

/// Writes an `ODoc` as a structurally valid OOXML DOCX package.
///
/// The function has no filesystem side effects; callers can send the returned bytes to a save
/// dialog or a browser download.  The caller is responsible for running lint and fact checks
/// before making the package available to a user.
///
/// # Errors
///
/// Returns an error when the document contains invalid control characters or the in-memory ZIP
/// package cannot be written.
pub fn write_docx(document: &ODoc) -> Result<Vec<u8>, ExportError> {
    write_docx_with_style(document, &Gb9704Style::default())
}

/// Writes an `ODoc` using a validated knowledge-pack style configuration.
///
/// # Errors
///
/// Returns an error when the document contains invalid control characters, the style fails
/// validation, or the in-memory ZIP package cannot be written.
pub fn write_docx_with_style(document: &ODoc, style: &Gb9704Style) -> Result<Vec<u8>, ExportError> {
    validate_document(document)?;
    style.validate()?;
    let mut output = Cursor::new(Vec::new());
    {
        let mut archive = ZipWriter::new(&mut output);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        add_part(
            &mut archive,
            "[Content_Types].xml",
            &content_types(),
            options,
        )?;
        add_part(&mut archive, "_rels/.rels", &root_relationships(), options)?;
        add_part(
            &mut archive,
            "docProps/core.xml",
            &core_properties(document),
            options,
        )?;
        add_part(
            &mut archive,
            "word/document.xml",
            &document_xml(document, style),
            options,
        )?;
        add_part(&mut archive, "word/styles.xml", &styles_xml(style), options)?;
        add_part(&mut archive, "word/settings.xml", &settings_xml(), options)?;
        add_part(
            &mut archive,
            "word/footer1.xml",
            &footer_xml(document, style),
            options,
        )?;
        add_part(
            &mut archive,
            "word/_rels/document.xml.rels",
            &document_relationships(),
            options,
        )?;
        archive.finish()?;
    }
    Ok(output.into_inner())
}

pub(crate) fn validate_document(document: &ODoc) -> Result<(), ExportError> {
    if document.schema_version.trim().is_empty() {
        return Err(ExportError::InvalidDocument("schema version is empty"));
    }
    if document.id.trim().is_empty() || document.body.id.trim().is_empty() {
        return Err(ExportError::InvalidDocument(
            "document identifiers are empty",
        ));
    }
    let mut texts = vec![
        document.id.as_str(),
        document.body.id.as_str(),
        document.body.title.id.as_str(),
        document.body.title.text.as_str(),
    ];
    if let Some(header) = &document.header {
        texts.extend([
            header.id.as_str(),
            header.issuing_authority_mark.as_str(),
            header.document_number.as_deref().unwrap_or_default(),
            header.signer.as_deref().unwrap_or_default(),
        ]);
    }
    texts.extend(document.body.main_recipients.iter().map(String::as_str));
    for block in &document.body.blocks {
        texts.push(block.id());
        match block {
            Block::Heading { content, .. }
            | Block::Paragraph { content, .. }
            | Block::Quote { content, .. } => texts.push(content),
            Block::List { items, .. } => {
                texts.extend(
                    items
                        .iter()
                        .flat_map(|item| [item.id.as_str(), item.content.as_str()]),
                );
            }
            Block::Table { rows, .. } => {
                texts.extend(rows.iter().flat_map(|row| {
                    std::iter::once(row.id.as_str()).chain(
                        row.cells
                            .iter()
                            .flat_map(|cell| [cell.id.as_str(), cell.content.as_str()]),
                    )
                }));
            }
        }
    }
    texts.extend(document.body.attachments.iter().flat_map(|attachment| {
        [
            attachment.id.as_str(),
            attachment.title.as_str(),
            attachment.note.as_deref().unwrap_or_default(),
        ]
    }));
    if let Some(signer) = &document.body.signer {
        texts.extend([
            signer.id.as_str(),
            signer.authority.as_str(),
            signer.representative.as_deref().unwrap_or_default(),
        ]);
    }
    if let Some(date) = &document.body.date {
        texts.extend([date.id.as_str(), date.value.as_str()]);
    }
    texts.push(document.body.note.as_deref().unwrap_or_default());
    if let Some(footer) = &document.footer {
        texts.extend([
            footer.id.as_str(),
            footer.printing_authority.as_deref().unwrap_or_default(),
            footer.printing_date.as_deref().unwrap_or_default(),
        ]);
        texts.extend(footer.copy_recipients.iter().map(String::as_str));
    }
    if texts.into_iter().any(contains_invalid_xml_character) {
        return Err(ExportError::InvalidDocument(
            "document contains invalid XML control characters",
        ));
    }
    Ok(())
}

fn contains_invalid_xml_character(character: &str) -> bool {
    character
        .chars()
        .any(|value| matches!(value, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}'))
}

fn add_part<W: Write + std::io::Seek>(
    archive: &mut ZipWriter<W>,
    name: &str,
    contents: &str,
    options: SimpleFileOptions,
) -> Result<(), ExportError> {
    archive.start_file(name, options)?;
    archive.write_all(contents.as_bytes())?;
    Ok(())
}

fn content_types() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="{CT_NS}">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
  <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
  <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
</Types>"#
    )
}

fn root_relationships() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{REL_NS}">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>"#
    )
}

fn document_relationships() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{REL_NS}">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
</Relationships>"#
    )
}

fn core_properties(document: &ODoc) -> String {
    let title = xml_escape(document.body.title.text.trim());
    let identifier = xml_escape(&document.id);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <dc:title>{title}</dc:title>
  <dc:creator>秉笔</dc:creator>
  <cp:identifier>{identifier}</cp:identifier>
  <dcterms:created xsi:type="dcterms:W3CDTF">2026-01-01T00:00:00Z</dcterms:created>
</cp:coreProperties>"#
    )
}

/// Renders 版头: the issuing authority mark, document number and signer line.
fn version_head_xml(document: &ODoc, style: &Gb9704Style) -> String {
    let mut head = String::new();
    let Some(header) = &document.header else {
        return head;
    };
    if !header.issuing_authority_mark.trim().is_empty() {
        head.push_str(&paragraph(
            &header.issuing_authority_mark,
            ParagraphKind::HeaderMark,
            style,
        ));
    }
    if let Some(number) = header
        .document_number
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        head.push_str(&paragraph(number, ParagraphKind::DocumentNumber, style));
    }
    if let Some(signer) = header
        .signer
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        head.push_str(&paragraph(
            &format!("签发人：{signer}"),
            ParagraphKind::DocumentNumber,
            style,
        ));
    }
    head
}

/// Renders 落款: the issuing authority signature, date and closing note.
fn closing_xml(document: &ODoc, style: &Gb9704Style) -> String {
    let mut closing = String::new();
    if let Some(signer) = &document.body.signer {
        if !signer.authority.trim().is_empty() {
            closing.push_str(&paragraph(
                &signer.authority,
                ParagraphKind::Signature,
                style,
            ));
        }
        if let Some(representative) = signer
            .representative
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            closing.push_str(&paragraph(representative, ParagraphKind::Signature, style));
        }
    }
    if let Some(date) = &document.body.date {
        if !date.value.trim().is_empty() {
            closing.push_str(&paragraph(&date.value, ParagraphKind::Signature, style));
        }
    }
    if let Some(note) = document
        .body
        .note
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        closing.push_str(&paragraph(note, ParagraphKind::Note, style));
    }
    closing
}

pub(crate) fn document_xml(document: &ODoc, style: &Gb9704Style) -> String {
    let mut body = version_head_xml(document, style);
    if !document.body.title.text.trim().is_empty() {
        body.push_str(&paragraph(
            &document.body.title.text,
            ParagraphKind::Title,
            style,
        ));
    }
    for recipient in document
        .body
        .main_recipients
        .iter()
        .filter(|value| !value.trim().is_empty())
    {
        body.push_str(&paragraph(recipient, ParagraphKind::Recipient, style));
    }
    for block in &document.body.blocks {
        body.push_str(&block_xml(block, style));
    }
    for (index, attachment) in document.body.attachments.iter().enumerate() {
        let mut text = format!("附件{}：{}", index + 1, attachment.title.trim());
        if let Some(note) = attachment
            .note
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            text.push('（');
            text.push_str(note.trim());
            text.push('）');
        }
        body.push_str(&paragraph(&text, ParagraphKind::Attachment, style));
    }
    body.push_str(&closing_xml(document, style));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}">
  <w:body>
    {body}
    <w:sectPr>
      <w:footerReference w:type="default" r:id="rId3"/>
      <w:pgSz w:w="{width}" w:h="{height}"/>
      <w:pgMar w:top="{top}" w:right="{right}" w:bottom="{bottom}" w:left="{left}" w:header="0" w:footer="0" w:gutter="0"/>
      <w:docGrid w:linePitch="{line}"/>
    </w:sectPr>
  </w:body>
</w:document>"#,
        width = style.page_width_twips(),
        height = style.page_height_twips(),
        top = style.margin_top_twips(),
        right = style.margin_right_twips(),
        bottom = style.margin_bottom_twips(),
        left = style.margin_left_twips(),
        line = style.line_spacing_twips(),
    )
}

#[derive(Clone, Copy)]
enum ParagraphKind {
    HeaderMark,
    DocumentNumber,
    Title,
    Recipient,
    Body,
    Attachment,
    Signature,
    Heading(HeadingLevel),
    Quote,
    Note,
}

fn block_xml(block: &Block, style: &Gb9704Style) -> String {
    match block {
        Block::Heading { level, content, .. } => {
            paragraph(content, ParagraphKind::Heading(*level), style)
        }
        Block::Paragraph { content, .. } => paragraph(content, ParagraphKind::Body, style),
        Block::Quote { content, .. } => paragraph(content, ParagraphKind::Quote, style),
        Block::List {
            style: list_style,
            items,
            ..
        } => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let prefix = match list_style {
                    ListStyle::Ordered => format!("{}．", index + 1),
                    ListStyle::Unordered => "• ".to_owned(),
                };
                paragraph(
                    &format!("{prefix}{}", item.content),
                    ParagraphKind::Body,
                    style,
                )
            })
            .collect(),
        Block::Table { rows, .. } => table_xml(rows, style),
    }
}

fn paragraph(text: &str, kind: ParagraphKind, config: &Gb9704Style) -> String {
    let (style_id, alignment, color, bold, left_indent, first_line_chars) = match kind {
        ParagraphKind::HeaderMark => (
            "HeaderMark",
            "center",
            Some(config.mark_color()),
            true,
            0,
            None,
        ),
        ParagraphKind::DocumentNumber => ("DocumentNumber", "center", None, false, 0, None),
        ParagraphKind::Title => ("Title", "center", None, true, 0, None),
        ParagraphKind::Recipient => ("Recipient", "left", None, false, 0, None),
        ParagraphKind::Body => ("Normal", "left", None, false, 0, Some(200)),
        ParagraphKind::Attachment => ("Attachment", "left", None, false, 0, Some(200)),
        ParagraphKind::Signature => ("Signature", "right", None, false, 0, None),
        ParagraphKind::Heading(level) => (heading_style(level), "left", None, false, 0, None),
        ParagraphKind::Quote => ("Quote", "left", None, false, 720, None),
        ParagraphKind::Note => ("Note", "left", None, false, 0, Some(200)),
    };
    let mut ppr = format!(
        r#"<w:pPr><w:pStyle w:val="{style_id}"/><w:jc w:val="{alignment}"/><w:spacing w:line="{line}" w:lineRule="exact"/><w:snapToGrid/>"#,
        line = config.line_spacing_twips(),
    );
    if left_indent > 0 {
        let _ = write!(ppr, r#"<w:ind w:left="{left_indent}"/>"#);
    } else if let Some(first_line_chars) = first_line_chars {
        let _ = write!(ppr, r#"<w:ind w:firstLineChars="{first_line_chars}"/>"#);
    }
    if matches!(kind, ParagraphKind::HeaderMark) && config.header.separator_line {
        let _ = write!(
            ppr,
            r#"<w:pBdr><w:bottom w:val="single" w:sz="12" w:space="4" w:color="{}"/></w:pBdr>"#,
            config.mark_color(),
        );
    }
    ppr.push_str("</w:pPr>");
    let run_properties = format!(
        r"<w:rPr>{}{}</w:rPr>",
        color.map_or(String::new(), |value| format!(
            r#"<w:color w:val="{value}"/>"#
        )),
        if bold { "<w:b/>" } else { "" },
    );
    format!(
        r#"<w:p>{ppr}<w:r>{run_properties}<w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#,
        text = xml_escape(text),
    )
}

fn heading_style(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::L1 => "Heading1",
        HeadingLevel::L2 => "Heading2",
        HeadingLevel::L3 => "Heading3",
        HeadingLevel::L4 => "Heading4",
    }
}

fn table_xml(rows: &[odoc_model::TableRow], style: &Gb9704Style) -> String {
    let mut result = String::from(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders></w:tblPr>"#,
    );
    for row in rows {
        result.push_str("<w:tr>");
        for cell in &row.cells {
            let _ = write!(
                result,
                r#"<w:tc><w:tcPr><w:tcW w:w="0" w:type="auto"/></w:tcPr>{}</w:tc>"#,
                paragraph(&cell.content, ParagraphKind::Body, style)
            );
        }
        result.push_str("</w:tr>");
    }
    result.push_str("</w:tbl>");
    result
}

fn styles_xml(style: &Gb9704Style) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="{W_NS}">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{body_font}" w:eastAsia="{body_font}" w:hAnsi="{body_font}" w:cs="{body_font}"/><w:sz w:val="{body_size}"/><w:szCs w:val="{body_size}"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:line="{line}" w:lineRule="exact"/><w:snapToGrid/></w:pPr></w:pPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:rFonts w:ascii="{body_font}" w:eastAsia="{body_font}" w:hAnsi="{body_font}"/><w:sz w:val="{body_size}"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="HeaderMark"><w:name w:val="HeaderMark"/><w:rPr><w:rFonts w:ascii="{title_font}" w:eastAsia="{title_font}" w:hAnsi="{title_font}"/><w:sz w:val="{title_size}"/><w:b/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="DocumentNumber"><w:name w:val="DocumentNumber"/><w:rPr><w:rFonts w:ascii="{number_font}" w:eastAsia="{number_font}" w:hAnsi="{number_font}"/><w:sz w:val="{number_size}"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:rPr><w:rFonts w:ascii="{title_font}" w:eastAsia="{title_font}" w:hAnsi="{title_font}"/><w:sz w:val="{title_size}"/><w:b/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Recipient"><w:name w:val="Recipient"/><w:rPr><w:rFonts w:ascii="{body_font}" w:eastAsia="{body_font}" w:hAnsi="{body_font}"/><w:sz w:val="{body_size}"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Attachment"><w:name w:val="Attachment"/><w:rPr><w:rFonts w:ascii="{body_font}" w:eastAsia="{body_font}" w:hAnsi="{body_font}"/><w:sz w:val="{body_size}"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Signature"><w:name w:val="Signature"/><w:rPr><w:rFonts w:ascii="{heading_l2_font}" w:eastAsia="{heading_l2_font}" w:hAnsi="{heading_l2_font}"/><w:sz w:val="{heading_l2_size}"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:rPr><w:rFonts w:ascii="{body_font}" w:eastAsia="{body_font}" w:hAnsi="{body_font}"/><w:sz w:val="{body_size}"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="Note"><w:name w:val="Note"/><w:rPr><w:rFonts w:ascii="{body_font}" w:eastAsia="{body_font}" w:hAnsi="{body_font}"/><w:sz w:val="{body_size}"/></w:rPr></w:style>
  {headings}
</w:styles>"#,
        body_font = xml_escape(&style.fonts.body.font),
        body_size = style.fonts.body.half_points(),
        title_font = xml_escape(&style.fonts.title.font),
        title_size = style.fonts.title.half_points(),
        number_font = xml_escape(&style.fonts.document_number.font),
        number_size = style.fonts.document_number.half_points(),
        heading_l2_font = xml_escape(&style.fonts.heading_l2.font),
        heading_l2_size = style.fonts.heading_l2.half_points(),
        line = style.line_spacing_twips(),
        headings = (1..=4).fold(String::new(), |mut headings, level| {
                let heading = match level {
                    1 => &style.fonts.heading_l1,
                    2 => &style.fonts.heading_l2,
                    3 => &style.fonts.heading_l3,
                    _ => &style.fonts.heading_l4,
                };
                let font = xml_escape(&heading.font);
                let size = heading.half_points();
                let _ = write!(headings, r#"<w:style w:type="paragraph" w:styleId="Heading{level}"><w:name w:val="heading {level}"/><w:rPr><w:rFonts w:ascii="{font}" w:eastAsia="{font}" w:hAnsi="{font}"/><w:sz w:val="{size}"/></w:rPr></w:style>"#);
                headings
            }),
    )
}

fn settings_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="{W_NS}">
  <w:zoom w:percent="100"/>
  <w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>
  <w:doNotUseHTMLParagraphAutoSpacing/>
</w:settings>"#
    )
}

fn footer_xml(document: &ODoc, style: &Gb9704Style) -> String {
    let mut text = document
        .footer
        .as_ref()
        .map(|footer| footer.copy_recipients.join("、"))
        .unwrap_or_default();
    if let Some(footer) = &document.footer {
        if let Some(authority) = footer
            .printing_authority
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            if !text.is_empty() {
                text.push('；');
            }
            text.push_str(authority.trim());
        }
        if let Some(date) = footer
            .printing_date
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            if !text.is_empty() {
                text.push('；');
            }
            text.push_str(date.trim());
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="{W_NS}" xmlns:r="{R_NS}">
  <w:p><w:pPr><w:jc w:val="left"/><w:spacing w:line="{line}" w:lineRule="exact"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="{font}" w:eastAsia="{font}" w:hAnsi="{font}"/><w:sz w:val="24"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r><w:r><w:t xml:space="preserve">    — </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/><w:instrText xml:space="preserve"> PAGE </w:instrText><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t xml:space="preserve"> —</w:t></w:r></w:p>
</w:ftr>"#,
        line = style.line_spacing_twips(),
        font = xml_escape(&style.fonts.body.font),
        text = xml_escape(&text),
    )
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_model::{Block, Fact, FactKind, HeadingLevel, TableCell, TableRow};
    use std::io::Read;
    use zip::ZipArchive;

    #[test]
    fn emits_a_readable_docx_with_required_parts() {
        let mut document = ODoc::empty();
        document.body.title.text = "关于 <测试> 的请示".to_owned();
        document.body.main_recipients.push("县人民政府".to_owned());
        document.body.blocks = vec![
            Block::Heading {
                id: "h".to_owned(),
                level: HeadingLevel::L1,
                content: "一、情况".to_owned(),
            },
            Block::Paragraph {
                id: "p".to_owned(),
                content: "正文 & 内容".to_owned(),
            },
            Block::Table {
                id: "t".to_owned(),
                rows: vec![TableRow {
                    id: "r".to_owned(),
                    cells: vec![TableCell {
                        id: "c".to_owned(),
                        content: "单元格".to_owned(),
                    }],
                }],
            },
        ];
        document.facts.facts.push(Fact::from_user_input(
            FactKind::Data,
            "1".to_owned(),
            None,
            None,
            None,
        ));
        let bytes = write_docx(&document).expect("docx should be written");
        let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("zip should be readable");
        for name in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/styles.xml",
            "word/settings.xml",
            "word/footer1.xml",
            "word/_rels/document.xml.rels",
            "docProps/core.xml",
        ] {
            assert!(archive.by_name(name).is_ok(), "missing {name}");
        }
        let mut document_xml = String::new();
        archive
            .by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut document_xml)
            .unwrap();
        assert!(document_xml.contains("11906") && document_xml.contains("560"));
        assert!(document_xml.contains("&lt;测试&gt;") && document_xml.contains("&amp; 内容"));
    }

    #[test]
    fn rejects_xml_control_characters() {
        let mut document = ODoc::empty();
        document.body.title.text = "坏\u{1}数据".to_owned();
        assert!(matches!(
            write_docx(&document),
            Err(ExportError::InvalidDocument(_))
        ));
    }
}
