use super::super::ExportError;
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};
use zip::ZipArchive;

const MAX_ZIP_PART_BYTES: usize = 16 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 4_096;

/// Reads text objects from an OFD package. OFD is a ZIP container whose page content is XML; the
/// semantic text is stored in `TextCode` elements. The package's page/document indirection is
/// intentionally not trusted: all XML parts are inspected in archive order, which also handles
/// vendor packages that omit or localize `OFD.xml` references.
pub(super) fn extract_ofd(bytes: &[u8]) -> Result<String, ExportError> {
    let mut archive = open_archive(bytes)?;
    let mut parts = Vec::new();
    for index in 0..archive.len().min(MAX_ZIP_ENTRIES) {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| ExportError::InvalidReference)?;
        let name = entry.name().to_owned();
        if entry.is_dir() || !name.to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        let xml = read_entry(&mut entry)?;
        if is_ofd_content_part(&name, &xml) {
            parts.push((name, xml));
        }
    }
    if parts.is_empty() {
        return Err(ExportError::InvalidReference);
    }
    parts.sort_by_key(|left| ofd_part_rank(&left.0));
    let mut output = String::new();
    for (_, xml) in parts {
        let values = collect_tag_text(&xml, &["TextCode", "Text"])?;
        append_lines(&mut output, values);
    }
    if output.trim().is_empty() {
        // A few producers put text directly in Content.xml under a vendor-specific tag. Keep a
        // conservative fallback that ignores attributes and known layout/metadata elements.
        let mut fallback = String::new();
        for (_, xml) in collect_xml_entries(bytes)? {
            append_lines(
                &mut fallback,
                collect_leaf_text(&xml, &["ofd", "Page", "Document", "OFD"])?,
            );
        }
        return if fallback.trim().is_empty() {
            Err(ExportError::InvalidReference)
        } else {
            Ok(fallback)
        };
    }
    Ok(output)
}

/// Extracts cell values from an OOXML spreadsheet. Shared strings and inline strings are
/// resolved, while rows/cells retain their tabular order so the drafting model can distinguish
/// adjacent facts.
pub(super) fn extract_xlsx(bytes: &[u8]) -> Result<String, ExportError> {
    let mut archive = open_archive(bytes)?;
    let shared_strings = archive
        .by_name("xl/sharedStrings.xml")
        .ok()
        .map(|mut entry| read_entry(&mut entry))
        .transpose()?
        .map(|xml| parse_shared_strings(&xml))
        .transpose()?
        .unwrap_or_default();

    let sheet_names = workbook_sheet_paths(&mut archive)?;
    let mut paths = if sheet_names.is_empty() {
        let mut discovered = Vec::new();
        for index in 0..archive.len().min(MAX_ZIP_ENTRIES) {
            let entry = archive
                .by_index(index)
                .map_err(|_| ExportError::InvalidReference)?;
            let name = entry.name().to_owned();
            if name.starts_with("xl/worksheets/") && name.to_ascii_lowercase().ends_with(".xml") {
                discovered.push(name);
            }
        }
        discovered.sort();
        discovered
    } else {
        sheet_names
    };
    paths.dedup();
    if paths.is_empty() {
        return Err(ExportError::InvalidReference);
    }

    let mut output = String::new();
    for path in paths {
        let mut entry = archive
            .by_name(&path)
            .map_err(|_| ExportError::InvalidReference)?;
        let xml = read_entry(&mut entry)?;
        let rows = parse_sheet(&xml, &shared_strings)?;
        append_sheet(&mut output, rows);
    }
    if output.trim().is_empty() {
        Err(ExportError::InvalidReference)
    } else {
        Ok(output)
    }
}

/// WPS has both OOXML-like ZIP variants and vendor-specific XML packages. Prefer obvious document
/// parts, then fall back to leaf text in XML parts while excluding package metadata.
pub(super) fn extract_wps(bytes: &[u8]) -> Result<String, ExportError> {
    if let Ok(document) = super::super::read_docx(bytes) {
        let text = document.to_plain_text();
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    let entries = collect_xml_entries(bytes)?;
    let mut ranked = entries;
    ranked.sort_by_key(|(name, _)| {
        let lower = name.to_ascii_lowercase();
        if lower.contains("document") || lower.contains("content") {
            0_u8
        } else if lower.contains("body") || lower.contains("text") {
            1
        } else {
            2
        }
    });
    let mut output = String::new();
    for (_, xml) in ranked {
        let values = collect_leaf_text(
            &xml,
            &[
                "Relationships",
                "Types",
                "Properties",
                "AppVersion",
                "CoreProperties",
            ],
        )?;
        append_lines(&mut output, values);
    }
    if output.trim().is_empty() {
        Err(ExportError::InvalidReference)
    } else {
        Ok(output)
    }
}

fn open_archive(bytes: &[u8]) -> Result<ZipArchive<Cursor<&[u8]>>, ExportError> {
    let archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| ExportError::InvalidReference)?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(ExportError::PackageTooLarge);
    }
    Ok(archive)
}

fn read_entry<R: Read>(entry: &mut R) -> Result<Vec<u8>, ExportError> {
    let mut bytes = Vec::new();
    entry
        .take((MAX_ZIP_PART_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| ExportError::InvalidReference)?;
    if bytes.len() > MAX_ZIP_PART_BYTES {
        Err(ExportError::PackageTooLarge)
    } else {
        Ok(bytes)
    }
}

fn collect_xml_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ExportError> {
    let mut archive = open_archive(bytes)?;
    let mut entries = Vec::new();
    for index in 0..archive.len().min(MAX_ZIP_ENTRIES) {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| ExportError::InvalidReference)?;
        let name = entry.name().to_owned();
        if entry.is_dir() || !name.to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        entries.push((name, read_entry(&mut entry)?));
    }
    Ok(entries)
}

fn is_ofd_content_part(name: &str, xml: &[u8]) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("content")
        || lower.contains("page")
        || lower.ends_with("document.xml")
        || xml
            .windows(b"TextCode".len())
            .any(|window| window == b"TextCode")
}

fn ofd_part_rank(name: &str) -> u8 {
    let lower = name.to_ascii_lowercase();
    if lower.contains("page") || lower.contains("content") {
        0
    } else if lower.ends_with("document.xml") {
        1
    } else {
        2
    }
}

fn collect_tag_text(xml: &[u8], tags: &[&str]) -> Result<Vec<String>, ExportError> {
    let wanted: Vec<String> = tags.iter().map(|tag| tag.to_ascii_lowercase()).collect();
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut active: Option<String> = None;
    let mut values = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                let local = local_name(&event).to_ascii_lowercase();
                if wanted.iter().any(|tag| tag == &local) {
                    active = Some(local);
                }
            }
            Ok(Event::Text(event)) if active.is_some() => {
                let decoded = event.decode().map_err(|_| ExportError::InvalidReference)?;
                let value = unescape(&decoded)
                    .map_err(|_| ExportError::InvalidReference)?
                    .into_owned();
                if !value.trim().is_empty() {
                    values.push(value);
                }
            }
            Ok(Event::CData(event)) if active.is_some() => {
                let value = String::from_utf8_lossy(event.as_ref()).into_owned();
                if !value.trim().is_empty() {
                    values.push(value);
                }
            }
            Ok(Event::End(event)) => {
                let local =
                    String::from_utf8_lossy(event.local_name().as_ref()).to_ascii_lowercase();
                if active.as_deref() == Some(local.as_str()) {
                    active = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(ExportError::InvalidReference),
            _ => {}
        }
        buffer.clear();
    }
    Ok(values)
}

fn collect_leaf_text(xml: &[u8], ignored: &[&str]) -> Result<Vec<String>, ExportError> {
    let ignored: Vec<String> = ignored
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut values = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => stack.push(local_name(&event).to_ascii_lowercase()),
            Ok(Event::Text(event)) if !stack.is_empty() => {
                if stack
                    .iter()
                    .any(|name| ignored.iter().any(|item| item == name))
                {
                    buffer.clear();
                    continue;
                }
                let decoded = event.decode().map_err(|_| ExportError::InvalidReference)?;
                let value = unescape(&decoded)
                    .map_err(|_| ExportError::InvalidReference)?
                    .into_owned();
                if !value.trim().is_empty() {
                    values.push(value);
                }
            }
            Ok(Event::CData(event)) if !stack.is_empty() => {
                let value = String::from_utf8_lossy(event.as_ref()).into_owned();
                if !value.trim().is_empty() {
                    values.push(value);
                }
            }
            Ok(Event::End(_)) => {
                stack.pop();
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(ExportError::InvalidReference),
            _ => {}
        }
        buffer.clear();
    }
    Ok(values)
}

fn local_name(event: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(event.local_name().as_ref()).into_owned()
}

fn append_lines(output: &mut String, values: Vec<String>) {
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(value);
    }
}

fn workbook_sheet_paths(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<Vec<String>, ExportError> {
    let workbook = match archive.by_name("xl/workbook.xml") {
        Ok(mut entry) => read_entry(&mut entry)?,
        Err(_) => return Ok(Vec::new()),
    };
    let relationships = match archive.by_name("xl/_rels/workbook.xml.rels") {
        Ok(mut entry) => read_entry(&mut entry)?,
        Err(_) => return Ok(Vec::new()),
    };
    let mut targets = HashMap::new();
    let mut reader = Reader::from_reader(relationships.as_slice());
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Empty(event) | Event::Start(event))
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let mut id = None;
                let mut target = None;
                for attribute in event.attributes().flatten() {
                    match attribute.key.local_name().as_ref() {
                        b"Id" => id = Some(String::from_utf8_lossy(&attribute.value).into_owned()),
                        b"Target" => {
                            target = Some(String::from_utf8_lossy(&attribute.value).into_owned());
                        }
                        _ => {}
                    }
                }
                if let (Some(id), Some(target)) = (id, target) {
                    targets.insert(id, target);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(ExportError::InvalidReference),
            _ => {}
        }
        buffer.clear();
    }
    let mut result = Vec::new();
    let mut reader = Reader::from_reader(workbook.as_slice());
    buffer.clear();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Empty(event) | Event::Start(event))
                if event.local_name().as_ref() == b"sheet" =>
            {
                let relation = event
                    .attributes()
                    .flatten()
                    .find(|attribute| attribute.key.local_name().as_ref() == b"id")
                    .map(|attribute| String::from_utf8_lossy(&attribute.value).into_owned());
                if let Some(relation) = relation.and_then(|id| targets.get(&id).cloned()) {
                    let target = relation.trim_start_matches('/');
                    let path = if target.starts_with("xl/") {
                        target.to_owned()
                    } else {
                        format!("xl/{target}")
                    };
                    result.push(path);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(ExportError::InvalidReference),
            _ => {}
        }
        buffer.clear();
    }
    Ok(result)
}

fn parse_shared_strings(xml: &[u8]) -> Result<Vec<String>, ExportError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut in_item = false;
    let mut current = String::new();
    let mut values = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"si" => {
                in_item = true;
                current.clear();
            }
            Ok(Event::Start(event)) if in_item && event.local_name().as_ref() == b"t" => {}
            Ok(Event::Text(event)) if in_item => {
                let decoded = event.decode().map_err(|_| ExportError::InvalidReference)?;
                current.push_str(&unescape(&decoded).map_err(|_| ExportError::InvalidReference)?);
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"si" => {
                values.push(std::mem::take(&mut current));
                in_item = false;
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(ExportError::InvalidReference),
            _ => {}
        }
        buffer.clear();
    }
    Ok(values)
}

fn parse_sheet(xml: &[u8], shared_strings: &[String]) -> Result<Vec<Vec<String>>, ExportError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut rows: BTreeMap<usize, BTreeMap<usize, String>> = BTreeMap::new();
    let mut current_cell: Option<CellState> = None;
    let mut current_row: Option<usize> = None;
    let mut next_col = 0_usize;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"row" => {
                current_row = attr_usize(&event, b"r").or_else(|| Some(rows.len() + 1));
                next_col = 0;
            }
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"c" => {
                let reference = attr_string(&event, b"r");
                let (row, col) = reference
                    .as_deref()
                    .and_then(cell_reference)
                    .unwrap_or((current_row.unwrap_or(1), next_col));
                next_col = col.saturating_add(1);
                current_cell = Some(CellState {
                    row,
                    col,
                    kind: attr_string(&event, b"t").unwrap_or_default(),
                    value: String::new(),
                    in_value: false,
                });
            }
            Ok(Event::Start(event))
                if current_cell.is_some() && event.local_name().as_ref() == b"v" =>
            {
                if let Some(cell) = current_cell.as_mut() {
                    cell.in_value = true;
                }
            }
            Ok(Event::Start(event))
                if current_cell.is_some() && event.local_name().as_ref() == b"t" =>
            {
                if let Some(cell) = current_cell.as_mut() {
                    cell.in_value = true;
                }
            }
            Ok(Event::Text(event)) if current_cell.as_ref().is_some_and(|cell| cell.in_value) => {
                let decoded = event.decode().map_err(|_| ExportError::InvalidReference)?;
                if let Some(cell) = current_cell.as_mut() {
                    cell.value
                        .push_str(&unescape(&decoded).map_err(|_| ExportError::InvalidReference)?);
                }
            }
            Ok(Event::End(event))
                if event.local_name().as_ref() == b"v" || event.local_name().as_ref() == b"t" =>
            {
                if let Some(cell) = current_cell.as_mut() {
                    cell.in_value = false;
                }
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"c" => {
                if let Some(mut cell) = current_cell.take() {
                    if cell.kind == "s" {
                        if let Ok(index) = cell.value.trim().parse::<usize>() {
                            cell.value = shared_strings.get(index).cloned().unwrap_or_default();
                        }
                    }
                    rows.entry(cell.row)
                        .or_default()
                        .insert(cell.col, cell.value);
                }
            }
            Ok(Event::End(event)) if event.local_name().as_ref() == b"row" => {
                current_row = None;
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(ExportError::InvalidReference),
            _ => {}
        }
        buffer.clear();
    }
    Ok(rows
        .into_values()
        .map(|row| {
            let max_col = row.keys().copied().max().unwrap_or(0);
            (0..=max_col)
                .map(|column| row.get(&column).cloned().unwrap_or_default())
                .collect()
        })
        .collect())
}

#[derive(Debug)]
struct CellState {
    row: usize,
    col: usize,
    kind: String,
    value: String,
    in_value: bool,
}

fn attr_string(event: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    event
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == name)
        .map(|attribute| String::from_utf8_lossy(&attribute.value).into_owned())
}

fn attr_usize(event: &BytesStart<'_>, name: &[u8]) -> Option<usize> {
    attr_string(event, name)?.parse().ok()
}

fn cell_reference(value: &str) -> Option<(usize, usize)> {
    let mut letters = String::new();
    let mut digits = String::new();
    for character in value.chars() {
        if character.is_ascii_alphabetic() {
            letters.push(character.to_ascii_uppercase());
        } else if character.is_ascii_digit() {
            digits.push(character);
        } else if character != '$' {
            return None;
        }
    }
    let row = digits.parse::<usize>().ok()?.saturating_sub(1);
    let mut column = 0_usize;
    for character in letters.bytes() {
        column = column
            .saturating_mul(26)
            .saturating_add((character - b'A' + 1) as usize);
    }
    Some((row, column.saturating_sub(1)))
}

fn append_sheet(output: &mut String, rows: Vec<Vec<String>>) {
    for row in rows {
        let mut last = row.len();
        while last > 0 && row[last - 1].trim().is_empty() {
            last -= 1;
        }
        if last == 0 {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(
            &row[..last]
                .iter()
                .map(|value| value.trim())
                .collect::<Vec<_>>()
                .join("\t"),
        );
    }
}
