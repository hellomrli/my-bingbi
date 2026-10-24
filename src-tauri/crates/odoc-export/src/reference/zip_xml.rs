use super::super::ExportError;
use quick_xml::Reader;
use crate::xml_text::decode_text;
use quick_xml::events::{BytesStart, Event};
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};
use zip::ZipArchive;

const MAX_ZIP_PART_BYTES: usize = 16 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 4_096;
/// Aggregate budget for every decompressed XML part of one package, so a high-compression
/// input cannot inflate into many times its own size (zip bomb).
const MAX_ZIP_TOTAL_BYTES: usize = 64 * 1024 * 1024;
/// Excel hard limits: column XFD = `16_384`, row `1_048_576`. A cell reference outside these
/// bounds is untrustworthy and must not drive a dense expansion.
const MAX_SHEET_COLUMNS: usize = 16_384;
const MAX_SHEET_ROWS: usize = 1_048_576;
/// Upper bound on the cells a parsed sheet may materialize when rendered to text (row ×
/// column slots). Guards the `(0..=max_col)` dense expansion against sparse hostile sheets.
const MAX_SHEET_EXPANDED_CELLS: usize = 1_000_000;

/// Reads text objects from an OFD package. OFD is a ZIP container whose page content is XML; the
/// semantic text is stored in `TextCode` elements. The package's page/document indirection is
/// intentionally not trusted: all XML parts are inspected in archive order, which also handles
/// vendor packages that omit or localize `OFD.xml` references.
pub(super) fn extract_ofd(bytes: &[u8]) -> Result<String, ExportError> {
    let mut archive = open_archive(bytes)?;
    let mut budget = 0_usize;
    let mut parts = Vec::new();
    for index in 0..archive.len().min(MAX_ZIP_ENTRIES) {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| ExportError::InvalidReference)?;
        let name = entry.name().to_owned();
        if entry.is_dir() || !name.to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        let xml = read_entry_budgeted(&mut entry, &mut budget)?;
        if is_ofd_content_part(&name, &xml) {
            parts.push((name, xml));
        }
    }
    if parts.is_empty() {
        return Err(ExportError::InvalidReference);
    }
    parts.sort_by_key(|part| ofd_part_rank(&part.0));
    let mut output = String::new();
    for (_, xml) in &parts {
        let values = collect_tag_text(xml, &["TextCode", "Text"])?;
        append_lines(&mut output, values);
    }
    if output.trim().is_empty() {
        // A few producers put text directly in Content.xml under a vendor-specific tag. Keep a
        // conservative fallback that ignores attributes and known layout/metadata elements.
        // The parts above were already decompressed and stay within the shared budget, so the
        // fallback reuses them instead of re-opening the archive (which would double memory).
        let mut fallback = String::new();
        for (name, xml) in &parts {
            if is_ofd_content_part(name, xml) {
                append_lines(
                    &mut fallback,
                    collect_leaf_text(xml, &["ofd", "Page", "Document", "OFD"])?,
                );
            }
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
    let mut budget = 0_usize;
    let shared_strings = archive
        .by_name("xl/sharedStrings.xml")
        .ok()
        .map(|mut entry| read_entry_budgeted(&mut entry, &mut budget))
        .transpose()?
        .map(|xml| parse_shared_strings(&xml))
        .transpose()?
        .unwrap_or_default();

    let sheet_names = workbook_sheet_paths(&mut archive, &mut budget)?;
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
        let xml = read_entry_budgeted(&mut entry, &mut budget)?;
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
    let mut budget = 0_usize;
    let mut ranked = collect_xml_entries(bytes, &mut budget)?;
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

fn read_entry_budgeted<R: Read>(entry: &mut R, budget: &mut usize) -> Result<Vec<u8>, ExportError> {
    let mut bytes = Vec::new();
    entry
        .take((MAX_ZIP_PART_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| ExportError::InvalidReference)?;
    if bytes.len() > MAX_ZIP_PART_BYTES {
        return Err(ExportError::PackageTooLarge);
    }
    charge_budget(budget, bytes.len())?;
    Ok(bytes)
}

/// Charges `len` decompressed bytes against the aggregate package budget.
fn charge_budget(budget: &mut usize, len: usize) -> Result<(), ExportError> {
    *budget = budget.saturating_add(len);
    if *budget > MAX_ZIP_TOTAL_BYTES {
        return Err(ExportError::PackageTooLarge);
    }
    Ok(())
}

fn collect_xml_entries(
    bytes: &[u8],
    budget: &mut usize,
) -> Result<Vec<(String, Vec<u8>)>, ExportError> {
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
        entries.push((name, read_entry_budgeted(&mut entry, budget)?));
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
    let mut current = String::new();
    let mut values = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                let local = local_name(&event).to_ascii_lowercase();
                if active.is_none() && wanted.iter().any(|tag| tag == &local) {
                    active = Some(local);
                }
            }
            Ok(event @ (Event::Text(_) | Event::GeneralRef(_) | Event::CData(_))) if active.is_some() => {
                current.push_str(&decode_text(&event).map_err(|_| ExportError::InvalidReference)?);
            }
            Ok(Event::End(event)) => {
                let local = String::from_utf8_lossy(event.local_name().as_ref()).to_ascii_lowercase();
                if active.as_deref() == Some(local.as_str()) {
                    push_text(&mut values, &mut current);
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

fn push_text(values: &mut Vec<String>, current: &mut String) {
    let text = std::mem::take(current);
    if !text.trim().is_empty() {
        values.push(text);
    }
}

fn collect_leaf_text(xml: &[u8], ignored: &[&str]) -> Result<Vec<String>, ExportError> {
    let ignored: Vec<String> = ignored.iter().map(|value| value.to_ascii_lowercase()).collect();
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut values = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                push_text(&mut values, &mut current);
                stack.push(local_name(&event).to_ascii_lowercase());
            }
            Ok(event @ (Event::Text(_) | Event::GeneralRef(_) | Event::CData(_)))
                if !stack.is_empty() && !stack.iter().any(|name| ignored.contains(name)) => {
                current.push_str(&decode_text(&event).map_err(|_| ExportError::InvalidReference)?);
            }
            Ok(Event::End(_)) => {
                push_text(&mut values, &mut current);
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
    budget: &mut usize,
) -> Result<Vec<String>, ExportError> {
    let workbook = match archive.by_name("xl/workbook.xml") {
        Ok(mut entry) => read_entry_budgeted(&mut entry, budget)?,
        Err(_) => return Ok(Vec::new()),
    };
    let relationships = match archive.by_name("xl/_rels/workbook.xml.rels") {
        Ok(mut entry) => read_entry_budgeted(&mut entry, budget)?,
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
            Ok(event @ (Event::Text(_) | Event::GeneralRef(_) | Event::CData(_))) if in_item => {
                current.push_str(&decode_text(&event).map_err(|_| ExportError::InvalidReference)?);
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
            Ok(event @ (Event::Text(_) | Event::GeneralRef(_) | Event::CData(_))) if current_cell.as_ref().is_some_and(|cell| cell.in_value) => {
                if let Some(cell) = current_cell.as_mut() {
                    cell.value.push_str(&decode_text(&event).map_err(|_| ExportError::InvalidReference)?);
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
    let expanded_cells: usize = rows
        .values()
        .map(|row| row.keys().copied().max().unwrap_or(0).saturating_add(1))
        .sum();
    if expanded_cells > MAX_SHEET_EXPANDED_CELLS {
        return Err(ExportError::PackageTooLarge);
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
            .checked_mul(26)?
            .checked_add((character - b'A' + 1) as usize)?;
    }
    let column = column.checked_sub(1)?;
    if row >= MAX_SHEET_ROWS || column >= MAX_SHEET_COLUMNS {
        return None;
    }
    Some((row, column))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn xml_text_fragments_preserve_entities_without_inserting_newlines() {
        let xml = br"<root><t>A&amp;B &lt; 10 &#x4E2D;<![CDATA[ &raw;]]></t></root>";
        let expected = vec!["A&B < 10 中 &raw;".to_owned()];
        assert_eq!(collect_tag_text(xml, &["t"]).expect("tag"), expected);
        assert_eq!(collect_leaf_text(xml, &[]).expect("leaf"), expected);
        assert!(collect_leaf_text(xml, &["t"]).expect("ignored").is_empty());
        assert_eq!(parse_shared_strings(br"<sst><si><t>A&amp;B</t></si></sst>").expect("shared"), vec!["A&B"]);
        assert_eq!(parse_sheet(br#"<worksheet><row r="1"><c r="A1" t="inlineStr"><is><t>A&amp;B &lt; &#65;</t></is></c></row></worksheet>"#, &[]).expect("sheet"), vec![vec!["A&B < A"]]);
    }

    #[test]
    fn cell_reference_bounds_are_enforced() {
        // Normal references still parse.
        assert_eq!(cell_reference("A1"), Some((0, 0)));
        assert_eq!(cell_reference("XFD1048576"), Some((1_048_575, 16_383)));
        assert_eq!(cell_reference("$B$2"), Some((1, 1)));
        // Columns beyond XFD are untrustworthy, not saturated to usize::MAX.
        assert_eq!(cell_reference("XFE1"), None);
        assert_eq!(cell_reference("AAAAAAAAAAAAAAAAAAAA1"), None);
        // Rows beyond the spreadsheet limit are rejected too.
        assert_eq!(cell_reference("A1048577"), None);
        // Malformed references fall back to sequential placement; the loose column/row
        // ordering ("12A") is pre-existing permissive behavior and stays unchanged.
        assert_eq!(cell_reference(""), None);
        assert_eq!(cell_reference("12A"), Some((11, 0)));
    }

    #[test]
    fn read_entry_budget_caps_aggregate_decompressed_bytes() {
        let mut budget = 0_usize;
        let small = vec![b'a'; 1024];
        read_entry_budgeted(&mut Cursor::new(small.as_slice()), &mut budget)
            .expect("small entry should read");
        assert_eq!(budget, 1024);

        // charge_budget enforces the aggregate ceiling without allocating 64 MiB.
        let remaining = MAX_ZIP_TOTAL_BYTES - budget;
        assert!(charge_budget(&mut budget, remaining).is_ok());
        assert_eq!(budget, MAX_ZIP_TOTAL_BYTES);
        assert!(matches!(
            charge_budget(&mut budget, 1),
            Err(ExportError::PackageTooLarge)
        ));
    }

    #[test]
    fn single_entry_above_part_limit_is_rejected() {
        let mut budget = 0_usize;
        let oversized = vec![b'x'; MAX_ZIP_PART_BYTES + 1];
        assert!(matches!(
            read_entry_budgeted(&mut Cursor::new(oversized.as_slice()), &mut budget),
            Err(ExportError::PackageTooLarge)
        ));
    }

    #[test]
    fn sparse_hostile_sheet_is_rejected_before_expansion() {
        // Many rows with one cell parked at column XFD would expand into millions of
        // empty slots; the dense-expansion budget must reject it instead of OOMing.
        let mut xml = String::from("<worksheet><sheetData>");
        for row in 1..=2_000 {
            use std::fmt::Write as _;
            write!(xml, r#"<row r="{row}"><c r="XFD{row}"><v>1</v></c></row>"#)
                .expect("writing to a String cannot fail");
        }
        xml.push_str("</sheetData></worksheet>");
        assert!(matches!(
            parse_sheet(xml.as_bytes(), &[]),
            Err(ExportError::PackageTooLarge)
        ));
    }
}
