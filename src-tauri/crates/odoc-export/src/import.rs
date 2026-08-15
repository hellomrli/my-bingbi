use super::ExportError;
use odoc_model::{Block, HeadingLevel, ODoc};
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use std::io::{Cursor, Read};
use zip::ZipArchive;

const MAX_DOCX_BYTES: usize = 64 * 1024 * 1024;
const MAX_XML_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Default)]
struct ParsedParagraph {
    style: String,
    text: String,
}

/// Imports the readable text and paragraph hierarchy from a DOCX package into an [`ODoc`].
///
/// OOXML has many presentation details that cannot be represented by the canonical model. The
/// importer deliberately preserves the semantic text, heading levels, title, recipients and
/// common signature/date fields while marking all imported content as user-editable.
///
/// # Errors
///
/// Returns an error when the package is empty or oversized, is not a readable ZIP, or has no
/// parsable `word/document.xml`.
pub fn read_docx(bytes: &[u8]) -> Result<ODoc, ExportError> {
    if bytes.is_empty() || bytes.len() > MAX_DOCX_BYTES {
        return Err(ExportError::PackageTooLarge);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| ExportError::InvalidXml)?;
    let mut xml = Vec::new();
    archive
        .by_name("word/document.xml")
        .map_err(|_| ExportError::MissingPart)?
        .take(MAX_XML_BYTES as u64 + 1)
        .read_to_end(&mut xml)
        .map_err(ExportError::Io)?;
    if xml.len() > MAX_XML_BYTES {
        return Err(ExportError::PackageTooLarge);
    }
    let paragraphs = parse_paragraphs(&xml)?;
    if paragraphs.is_empty() {
        return Err(ExportError::InvalidDocument("docx contains no text"));
    }
    let mut document = ODoc::empty();
    document.meta.status = odoc_model::DocumentStatus::Review;
    document.body.blocks.clear();

    let mut index = 0;
    if let Some(first) = paragraphs
        .get(index)
        .filter(|item| item.style == "HeaderMark")
    {
        if let Some(header) = document.header.as_mut() {
            header.issuing_authority_mark.clone_from(&first.text);
        }
        index += 1;
    }
    if let Some(number) = paragraphs
        .get(index)
        .filter(|item| item.style == "DocumentNumber" || looks_like_document_number(&item.text))
    {
        if let Some(header) = document.header.as_mut() {
            header.document_number = Some(number.text.clone());
        }
        index += 1;
    }
    if let Some(title) = paragraphs.get(index).filter(|item| {
        item.style.eq_ignore_ascii_case("title") || (item.style.is_empty() && index == 0)
    }) {
        document.body.title.text.clone_from(&title.text);
        index += 1;
    }

    // A recipient paragraph usually ends with a full-width colon and appears before the body.
    while let Some(item) = paragraphs
        .get(index)
        .filter(|item| is_recipient(&item.text))
    {
        document.body.main_recipients.push(item.text.clone());
        index += 1;
    }

    for item in paragraphs.into_iter().skip(index) {
        let text = item.text.trim().to_owned();
        if text.is_empty() {
            continue;
        }
        if let Some(level) = heading_level(&item.style, &text) {
            document.body.blocks.push(Block::Heading {
                id: format!("import-heading-{index}"),
                level,
                content: text,
            });
        } else {
            document.body.blocks.push(Block::Paragraph {
                id: format!("import-paragraph-{index}"),
                content: text,
            });
        }
        index += 1;
    }
    if document.body.blocks.is_empty() {
        document.body.blocks.push(Block::paragraph(String::new()));
    }
    document.provenance = document
        .body
        .blocks
        .iter()
        .map(|block| odoc_model::Citation {
            id: format!("citation-{}", block.id()),
            node_id: block.id().to_owned(),
            source: odoc_model::ProvenanceSource::Imported,
            source_ref: Some("docx:word/document.xml".to_owned()),
            excerpt: Some(block.plain_text().chars().take(160).collect()),
        })
        .collect();
    Ok(document)
}

fn parse_paragraphs(xml: &[u8]) -> Result<Vec<ParsedParagraph>, ExportError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut paragraphs = Vec::new();
    let mut current: Option<ParsedParagraph> = None;
    let mut in_text = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"p" => {
                current = Some(ParsedParagraph::default());
            }
            Ok(Event::Start(event) | Event::Empty(event))
                if event.local_name().as_ref() == b"pStyle" =>
            {
                if let Some(paragraph) = current.as_mut() {
                    for attribute in event.attributes().flatten() {
                        if attribute.key.local_name().as_ref() == b"val" {
                            let decoded = reader
                                .decoder()
                                .decode(&attribute.value)
                                .map_err(|_| ExportError::InvalidXml)?;
                            paragraph.style = unescape(&decoded)
                                .map_err(|_| ExportError::InvalidXml)?
                                .into_owned();
                        }
                    }
                }
            }
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"t" => in_text = true,
            Ok(Event::Text(event)) if in_text => {
                if let Some(paragraph) = current.as_mut() {
                    let decoded = event.decode().map_err(|_| ExportError::InvalidXml)?;
                    paragraph
                        .text
                        .push_str(&unescape(&decoded).map_err(|_| ExportError::InvalidXml)?);
                }
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"t" => in_text = false,
            Ok(Event::Empty(event)) if event.local_name().as_ref() == b"tab" => {
                if let Some(paragraph) = current.as_mut() {
                    paragraph.text.push('\t');
                }
            }
            Ok(Event::Empty(event)) if event.local_name().as_ref() == b"br" => {
                if let Some(paragraph) = current.as_mut() {
                    paragraph.text.push('\n');
                }
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"p" => {
                if let Some(paragraph) = current.take() {
                    paragraphs.push(paragraph);
                }
                in_text = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(ExportError::InvalidXml),
            _ => {}
        }
        buffer.clear();
    }
    Ok(paragraphs)
}

fn heading_level(style: &str, text: &str) -> Option<HeadingLevel> {
    let normalized = style.to_ascii_lowercase();
    if normalized.contains("heading1") || normalized == "l1" {
        return Some(HeadingLevel::L1);
    }
    if normalized.contains("heading2") || normalized == "l2" {
        return Some(HeadingLevel::L2);
    }
    if normalized.contains("heading3") || normalized == "l3" {
        return Some(HeadingLevel::L3);
    }
    if normalized.contains("heading4") || normalized == "l4" {
        return Some(HeadingLevel::L4);
    }
    let trimmed = text.trim_start();
    if trimmed.starts_with('一') || trimmed.starts_with('二') || trimmed.starts_with('三') {
        return Some(HeadingLevel::L1);
    }
    if trimmed.starts_with('（') {
        return Some(HeadingLevel::L2);
    }
    if trimmed.as_bytes().first().is_some_and(u8::is_ascii_digit) && trimmed.contains('.') {
        return Some(HeadingLevel::L3);
    }
    None
}

fn is_recipient(value: &str) -> bool {
    let value = value.trim();
    value.len() <= 160
        && (value.ends_with('：') || value.ends_with(':'))
        && !value.contains('。')
        && !value.contains('，')
}

fn looks_like_document_number(value: &str) -> bool {
    let value = value.trim();
    value.contains('〔') && value.contains('〕') && value.contains('号')
}
