use super::super::ExportError;
use cfb::CompoundFile;
use encoding_rs::{GBK, WINDOWS_1252};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Seek};
use std::path::PathBuf;

const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;
const OLE_MAGIC: &[u8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";

pub(super) fn extract_xls(bytes: &[u8]) -> Result<String, ExportError> {
    let mut compound = open_compound(bytes)?;
    let path =
        find_stream_path(&compound, &["Workbook", "Book"]).ok_or(ExportError::InvalidReference)?;
    parse_biff_workbook(&read_stream(&mut compound, &path)?)
}

pub(super) fn extract_doc(bytes: &[u8]) -> Result<String, ExportError> {
    let mut compound = open_compound(bytes)?;
    let paths = stream_paths(&compound);
    let word_path = paths
        .iter()
        .find(|path| named(path, "WordDocument"))
        .cloned()
        .ok_or(ExportError::InvalidReference)?;
    let word = read_stream(&mut compound, &word_path)?;
    let table_name = if word
        .get(0x0a..0x0c)
        .is_some_and(|flags| read_u16(flags) & 0x0200 != 0)
    {
        "1Table"
    } else {
        "0Table"
    };
    let range_text = scan_word_text_range(&word);
    if let Some(path) = paths.iter().find(|path| named(path, table_name)) {
        if let Ok(table) = read_stream(&mut compound, path) {
            if let Some(text) =
                extract_word_piece_table(&word, &table).filter(|text| !text.trim().is_empty())
            {
                return Ok(text);
            }
        }
    }
    // WPS and a number of older DOC producers leave an empty/non-standard CLX while keeping the
    // actual text in the FIB's fcMin..fcMac range.
    if let Some(text) = range_text {
        return Ok(text);
    }

    // Do not scan every OLE stream: SummaryInformation, font tables and WPS build metadata often
    // contain much longer printable runs than a short document and would otherwise be returned as
    // if they were the document body.
    let mut candidates = Vec::new();
    for path in paths {
        if named(&path, "WordDocument") || !is_content_stream(&path) {
            continue;
        }
        if let Ok(stream) = read_stream(&mut compound, &path) {
            if let Some(text) = scan_text_stream(&stream) {
                candidates.push((text, 0));
            }
        }
    }
    choose_text(candidates).ok_or(ExportError::InvalidReference)
}

pub(super) fn extract_wps(bytes: &[u8]) -> Result<String, ExportError> {
    if let Ok(text) = extract_doc(bytes) {
        return Ok(text);
    }
    let mut compound = open_compound(bytes)?;
    let paths = stream_paths(&compound);
    let mut candidates = Vec::new();
    for path in paths {
        if named(&path, "WordDocument") || !is_content_stream(&path) {
            continue;
        }
        if let Ok(stream) = read_stream(&mut compound, &path) {
            if let Some(text) = scan_text_stream(&stream) {
                let name = path.to_string_lossy().to_ascii_lowercase();
                let priority =
                    if name.contains("text") || name.contains("content") || name.contains("body") {
                        30
                    } else {
                        0
                    };
                candidates.push((text, priority));
            }
        }
    }
    choose_text(candidates).ok_or(ExportError::InvalidReference)
}

fn open_compound(bytes: &[u8]) -> Result<CompoundFile<Cursor<&[u8]>>, ExportError> {
    if !bytes.starts_with(OLE_MAGIC) {
        return Err(ExportError::InvalidReference);
    }
    CompoundFile::open(Cursor::new(bytes)).map_err(|_| ExportError::InvalidReference)
}

fn named(path: &std::path::Path, name: &str) -> bool {
    path.file_name()
        .is_some_and(|value| value.eq_ignore_ascii_case(name))
}

fn stream_paths<F: Read + Seek>(compound: &CompoundFile<F>) -> Vec<PathBuf> {
    compound
        .walk()
        .filter(cfb::Entry::is_stream)
        .map(|entry| entry.path().to_path_buf())
        .collect()
}

fn find_stream_path<F: Read + Seek>(compound: &CompoundFile<F>, names: &[&str]) -> Option<PathBuf> {
    compound
        .walk()
        .filter(cfb::Entry::is_stream)
        .find_map(|entry| {
            names
                .iter()
                .any(|name| entry.name().eq_ignore_ascii_case(name))
                .then(|| entry.path().to_path_buf())
        })
}

fn read_stream<F: Read + Seek>(
    compound: &mut CompoundFile<F>,
    path: &PathBuf,
) -> Result<Vec<u8>, ExportError> {
    let size = compound
        .entry(path)
        .map_err(|_| ExportError::InvalidReference)?
        .len();
    if size > MAX_STREAM_BYTES as u64 {
        return Err(ExportError::PackageTooLarge);
    }
    let capacity = usize::try_from(size).map_err(|_| ExportError::PackageTooLarge)?;
    let mut bytes = Vec::with_capacity(capacity);
    compound
        .open_stream(path)
        .map_err(|_| ExportError::InvalidReference)?
        .take((MAX_STREAM_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| ExportError::InvalidReference)?;
    if bytes.len() > MAX_STREAM_BYTES {
        Err(ExportError::PackageTooLarge)
    } else {
        Ok(bytes)
    }
}

fn parse_biff_workbook(stream: &[u8]) -> Result<String, ExportError> {
    let mut offset = 0;
    let mut code_page = 1252;
    let mut shared = Vec::new();
    let mut cells: BTreeMap<(usize, usize), String> = BTreeMap::new();
    let mut pending_formula_string = None;
    while offset + 4 <= stream.len() {
        let record = read_u16(&stream[offset..]);
        let length = read_u16(&stream[offset + 2..]) as usize;
        offset += 4;
        let end = offset.saturating_add(length);
        if end > stream.len() {
            return Err(ExportError::InvalidReference);
        }
        let data = &stream[offset..end];
        match record {
            0x0042 if data.len() >= 2 => code_page = read_u16(data),
            0x00fc => parse_sst(data, code_page, &mut shared)?,
            0x00fd if data.len() >= 10 => {
                let position = (read_u16(data) as usize, read_u16(&data[2..]) as usize);
                if let Some(value) = shared.get(read_u32(&data[6..]) as usize) {
                    cells.insert(position, value.clone());
                }
            }
            0x0204 if data.len() >= 8 => {
                let position = (read_u16(data) as usize, read_u16(&data[2..]) as usize);
                if let Some((value, _)) = decode_biff_string(&data[6..], code_page) {
                    cells.insert(position, value);
                }
            }
            0x0203 if data.len() >= 14 => {
                let position = (read_u16(data) as usize, read_u16(&data[2..]) as usize);
                cells.insert(position, format_number(read_f64(&data[6..])));
            }
            0x027e if data.len() >= 10 => {
                let position = (read_u16(data) as usize, read_u16(&data[2..]) as usize);
                cells.insert(position, decode_rk(&data[6..]));
            }
            0x00bd => parse_mulrk(data, &mut cells),
            0x0006 if data.len() >= 14 => {
                let position = (read_u16(data) as usize, read_u16(&data[2..]) as usize);
                let result = &data[6..14];
                if result[6] == 0xff && result[7] == 0xff {
                    pending_formula_string = Some(position);
                } else {
                    cells.insert(position, format_number(read_f64(result)));
                }
            }
            0x0207 if pending_formula_string.is_some() => {
                if let (Some(position), Some((value, _))) = (
                    pending_formula_string.take(),
                    decode_biff_string(data, code_page),
                ) {
                    cells.insert(position, value);
                }
            }
            0x0205 if data.len() >= 8 => {
                let position = (read_u16(data) as usize, read_u16(&data[2..]) as usize);
                cells.insert(
                    position,
                    if data[6] == 0 { "FALSE" } else { "TRUE" }.to_owned(),
                );
            }
            _ => {}
        }
        offset = end;
        if cells.len() > 100_000 {
            return Err(ExportError::PackageTooLarge);
        }
    }
    render_cells(&cells)
}

fn render_cells(cells: &BTreeMap<(usize, usize), String>) -> Result<String, ExportError> {
    if cells.is_empty() {
        return Err(ExportError::InvalidReference);
    }
    let mut output = String::new();
    for row in 0..=cells.keys().map(|(row, _)| *row).max().unwrap_or(0) {
        let Some(last) = cells
            .keys()
            .filter(|(candidate, _)| *candidate == row)
            .map(|(_, column)| *column)
            .max()
        else {
            continue;
        };
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(
            &(0..=last)
                .map(|column| cells.get(&(row, column)).map_or("", String::as_str))
                .collect::<Vec<_>>()
                .join("\t"),
        );
    }
    Ok(output)
}

fn parse_sst(data: &[u8], code_page: u16, output: &mut Vec<String>) -> Result<(), ExportError> {
    if data.len() < 8 {
        return Err(ExportError::InvalidReference);
    }
    let count = read_u32(&data[4..]) as usize;
    let mut offset = 8;
    for _ in 0..count.min(100_000) {
        let Some((value, consumed)) = decode_biff_string(&data[offset..], code_page) else {
            return Err(ExportError::InvalidReference);
        };
        output.push(value);
        offset = offset.saturating_add(consumed);
        if offset > data.len() {
            return Err(ExportError::InvalidReference);
        }
    }
    Ok(())
}

fn parse_mulrk(data: &[u8], cells: &mut BTreeMap<(usize, usize), String>) {
    if data.len() < 6 {
        return;
    }
    let row = read_u16(data) as usize;
    let first = read_u16(&data[2..]) as usize;
    let last = read_u16(&data[data.len() - 2..]) as usize;
    for index in 0..=last.saturating_sub(first) {
        let start = 4 + index * 6;
        if start + 6 > data.len().saturating_sub(2) {
            break;
        }
        cells.insert((row, first + index), decode_rk(&data[start + 2..start + 6]));
    }
}

fn decode_rk(bytes: &[u8]) -> String {
    let value = read_u32(bytes);
    let number = if value & 0x02 != 0 {
        f64::from(i32::from_le_bytes(value.to_le_bytes()) >> 2)
    } else {
        f64::from_bits(u64::from(value & !0x03) << 32)
    };
    format_number(if value & 0x01 != 0 {
        number / 100.0
    } else {
        number
    })
}

fn format_number(value: f64) -> String {
    if value.is_finite() && value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn decode_biff_string(bytes: &[u8], code_page: u16) -> Option<(String, usize)> {
    if bytes.len() < 3 {
        return None;
    }
    let length = read_u16(bytes) as usize;
    let flags = bytes[2];
    let utf16 = flags & 0x01 != 0;
    let rich = if flags & 0x08 != 0 {
        if bytes.len() < 5 {
            return None;
        }
        read_u16(&bytes[3..]) as usize
    } else {
        0
    };
    let ext_offset = 3 + usize::from(flags & 0x08 != 0) * 2;
    let extended = if flags & 0x04 != 0 {
        if bytes.len() < ext_offset + 4 {
            return None;
        }
        read_u32(&bytes[ext_offset..]) as usize
    } else {
        0
    };
    let header = ext_offset + usize::from(flags & 0x04 != 0) * 4;
    let data_length = if utf16 {
        length.checked_mul(2)?
    } else {
        length
    };
    let end = header.checked_add(data_length)?;
    if end > bytes.len() {
        return None;
    }
    let raw = &bytes[header..end];
    let value = if utf16 {
        String::from_utf16_lossy(
            &raw.chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        decode_legacy(raw, code_page)
    };
    Some((value, end + rich * 4 + extended))
}

fn decode_legacy(bytes: &[u8], code_page: u16) -> String {
    let encoding = if code_page == 936 || code_page == 0 {
        GBK
    } else {
        WINDOWS_1252
    };
    encoding.decode(bytes).0.into_owned()
}

fn extract_word_piece_table(word: &[u8], table: &[u8]) -> Option<String> {
    if word.len() < 0x40 || read_u16(word) != 0xa5ec {
        return None;
    }
    // Parse the variable-sized FIB instead of relying on a Word-version-specific absolute offset.
    let word_count = read_u16(&word[0x20..]) as usize;
    let after_words = 0x22usize.checked_add(word_count.checked_mul(2)?)?;
    if after_words + 2 > word.len() {
        return None;
    }
    let long_count = read_u16(&word[after_words..]) as usize;
    let after_longs = after_words.checked_add(2 + long_count.checked_mul(4)?)?;
    if after_longs + 2 > word.len() {
        return None;
    }
    let pair_count = read_u16(&word[after_longs..]) as usize;
    let pairs = after_longs + 2;
    if pairs.checked_add(pair_count.checked_mul(8)?)? > word.len() {
        return None;
    }
    // fcClx is pair 33 in FibRgFcLcb97. It is authoritative when present.
    if let Some(text) = parse_piece_pair(word, table, pairs, pair_count, 33) {
        return Some(text);
    }

    // Older/vendor FibRgFcLcb variants can move the CLX. Only accept a strong text candidate from
    // those fallback pairs so arbitrary formatting tables are not mistaken for the body.
    choose_text(
        (0..pair_count)
            .filter(|index| *index != 33)
            .filter_map(|index| parse_piece_pair(word, table, pairs, pair_count, index))
            .filter(|text| is_high_confidence_text(text))
            .map(|text| (text, 0)),
    )
}

fn parse_piece_pair(
    word: &[u8],
    table: &[u8],
    pairs: usize,
    pair_count: usize,
    index: usize,
) -> Option<String> {
    if index >= pair_count {
        return None;
    }
    let pair = pairs.checked_add(index.checked_mul(8)?)?;
    let start = read_u32(word.get(pair..pair + 4)?) as usize;
    let size = read_u32(word.get(pair + 4..pair + 8)?) as usize;
    let end = start.checked_add(size)?;
    if size < 5 || end > table.len() {
        return None;
    }
    parse_clx(word, &table[start..end]).filter(|text| text.chars().any(char::is_alphanumeric))
}

fn parse_clx(word: &[u8], clx: &[u8]) -> Option<String> {
    let legacy_code_page = word_legacy_code_page(word);
    let mut offset = 0;
    while offset < clx.len() {
        match clx[offset] {
            0x01 => {
                if offset + 3 > clx.len() {
                    return None;
                }
                offset = offset.checked_add(3 + read_u16(&clx[offset + 1..]) as usize)?;
            }
            0x02 => {
                if offset + 5 > clx.len() {
                    return None;
                }
                let size = read_u32(&clx[offset + 1..]) as usize;
                let start = offset + 5;
                let end = start.checked_add(size)?;
                // PlcPcd is `(piece_count + 1) * 4` CP bytes followed by
                // `piece_count * 8` PCD bytes, so its total size is `12n + 4`.
                if end > clx.len() || size < 4 || (size - 4) % 12 != 0 {
                    return None;
                }
                let count = (size - 4) / 12;
                if count == 0 {
                    return None;
                }
                let cp_start = start;
                let pcd_start = cp_start + (count + 1) * 4;
                let mut output = String::new();
                for piece in 0..count {
                    let cp0 = read_u32(&clx[cp_start + piece * 4..]);
                    let cp1 = read_u32(&clx[cp_start + (piece + 1) * 4..]);
                    if cp1 < cp0 {
                        return None;
                    }
                    let pcd = pcd_start + piece * 8;
                    let encoded_offset = read_u32(&clx[pcd + 2..]);
                    let compressed = encoded_offset & 0x4000_0000 != 0;
                    let file_offset = if compressed {
                        (encoded_offset & 0x3fff_ffff) as usize / 2
                    } else {
                        encoded_offset as usize
                    };
                    let characters = cp1.saturating_sub(cp0) as usize;
                    if compressed {
                        let end = file_offset.checked_add(characters)?;
                        let bytes = word.get(file_offset..end)?;
                        output.push_str(&decode_legacy(bytes, legacy_code_page));
                    } else {
                        let end = file_offset.checked_add(characters.checked_mul(2)?)?;
                        let bytes = word.get(file_offset..end)?;
                        output.push_str(&String::from_utf16_lossy(
                            &bytes
                                .chunks_exact(2)
                                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                                .collect::<Vec<_>>(),
                        ));
                    }
                }
                return Some(clean_word_text(&output));
            }
            _ => return None,
        }
    }
    None
}

fn word_legacy_code_page(word: &[u8]) -> u16 {
    let language = word.get(0x06..0x08).map_or(0, read_u16);
    if language & 0x03ff == 0x0004 {
        936
    } else {
        1252
    }
}

fn scan_word_text_range(bytes: &[u8]) -> Option<String> {
    let (start, end) = word_text_bounds(bytes)?;
    let range = &bytes[start..end];
    if range.is_empty() {
        return None;
    }

    let mut candidates = Vec::new();
    if range.len() >= 2 {
        let even_length = range.len() & !1;
        let utf16 = String::from_utf16_lossy(
            &range[..even_length]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        );
        candidates.push((clean_word_text(&utf16), 300));
    }
    // Compressed Word pieces use the document/code-page bytes directly. Trying both common
    // encodings keeps Western DOC files readable while GBK handles the mainland WPS/DOC files.
    candidates.push((clean_word_text(&decode_legacy(range, 936)), 10));
    candidates.push((clean_word_text(&decode_legacy(range, 1252)), 0));
    candidates.extend(
        scan_text_stream_candidates(range)
            .into_iter()
            .map(|text| (text, 20)),
    );
    choose_text(candidates)
}

fn word_text_bounds(bytes: &[u8]) -> Option<(usize, usize)> {
    if bytes.len() < 0x20 {
        return None;
    }
    let start = read_u32(&bytes[0x18..]) as usize;
    let end = read_u32(&bytes[0x1c..]) as usize;
    if start >= end || end > bytes.len() {
        return None;
    }
    Some((start, end))
}

fn scan_text_stream(bytes: &[u8]) -> Option<String> {
    choose_text(
        scan_text_stream_candidates(bytes)
            .into_iter()
            .map(|text| (text, 0)),
    )
}

fn scan_text_stream_candidates(bytes: &[u8]) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut index = 0;
    while index + 4 < bytes.len() {
        if bytes[index + 1] == 0 && is_text_byte(bytes[index]) {
            let start = index;
            index += 2;
            while index + 1 < bytes.len() && bytes[index + 1] == 0 && is_text_byte(bytes[index]) {
                index += 2;
            }
            if index - start >= 6 {
                candidates.push(clean_word_text(&String::from_utf16_lossy(
                    &bytes[start..index]
                        .chunks_exact(2)
                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                        .collect::<Vec<_>>(),
                )));
            }
        } else {
            index += 1;
        }
    }
    let mut run = None;
    for (index, value) in bytes.iter().copied().chain(std::iter::once(0)).enumerate() {
        if index < bytes.len() && is_text_byte(value) {
            run.get_or_insert(index);
        } else if let Some(start) = run.take() {
            if index - start >= 6 {
                candidates.push(clean_word_text(&decode_legacy(&bytes[start..index], 936)));
            }
        }
    }
    candidates
}

fn choose_text<I>(candidates: I) -> Option<String>
where
    I: IntoIterator<Item = (String, i32)>,
{
    candidates
        .into_iter()
        .filter_map(|(text, bonus)| {
            let text = clean_word_text(&text);
            let score = text_quality(&text).saturating_add(i64::from(bonus));
            (score > 0).then_some((score, text))
        })
        .max_by_key(|(score, text)| (*score, text.len()))
        .map(|(_, text)| text)
}

fn text_quality(text: &str) -> i64 {
    let mut visible = 0_i64;
    let mut meaningful = 0_i64;
    let mut cjk = 0_i64;
    let mut replacements = 0_i64;
    for character in text.chars() {
        if character.is_whitespace() {
            continue;
        }
        visible += 1;
        if character == '\u{fffd}' {
            replacements += 1;
        }
        if character.is_alphanumeric() {
            meaningful += 1;
        }
        if ('\u{3400}'..='\u{9fff}').contains(&character)
            || ('\u{f900}'..='\u{faff}').contains(&character)
        {
            cjk += 1;
        }
    }
    if meaningful == 0 || replacements.saturating_mul(4) > visible.saturating_add(1) {
        return -10_000;
    }
    let lower = text.to_ascii_lowercase();
    let metadata_penalty = [
        "ksoproductbuildver",
        "summaryinformation",
        "documentsummaryinformation",
        "normal.dot",
        "normal.wpt",
    ]
    .iter()
    .fold(0_i64, |penalty, marker| {
        penalty + i64::from(lower.contains(*marker)) * 10_000
    });
    meaningful * 8 + cjk * 3 + visible - metadata_penalty
}

fn is_high_confidence_text(text: &str) -> bool {
    let mut visible = 0_usize;
    let mut meaningful = 0_usize;
    let mut replacements = 0_usize;
    let mut private_use = 0_usize;
    let mut single_ascii_tokens = 0_usize;
    for character in text.chars() {
        if character.is_whitespace() {
            continue;
        }
        visible += 1;
        meaningful += usize::from(character.is_alphanumeric());
        replacements += usize::from(character == '\u{fffd}');
        private_use += usize::from(
            ('\u{e000}'..='\u{f8ff}').contains(&character)
                || ('\u{f0000}'..='\u{ffffd}').contains(&character),
        );
    }
    for token in text.split_whitespace() {
        if token.len() == 1 && token.as_bytes()[0].is_ascii_alphanumeric() {
            single_ascii_tokens += 1;
        }
    }
    visible >= 8
        && meaningful >= 4
        && meaningful * 2 >= visible
        && replacements == 0
        && private_use == 0
        && single_ascii_tokens * 4 <= meaningful.saturating_add(8)
}

fn is_content_stream(path: &std::path::Path) -> bool {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.is_empty()
        || name.contains("summaryinformation")
        || name.contains("ksoproduct")
        || name == "data"
        || name.contains("font")
        || name.contains("thumbnail")
        || name.contains("picture")
        || name.contains("image")
    {
        return false;
    }
    name.contains("document")
        || name.contains("content")
        || name.contains("text")
        || name.contains("body")
        || name.contains("word")
        || name.contains("wps")
}

fn clean_word_text(value: &str) -> String {
    let mapped = value
        .chars()
        .map(|character| match character {
            '\r' | '\n' | '\u{000b}' | '\u{000c}' => '\n',
            '\t' | '\u{0007}' => '\t',
            '\u{0000}'..='\u{001f}' | '\u{007f}' => ' ',
            value => value,
        })
        .collect::<String>();
    mapped
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_text_byte(value: u8) -> bool {
    matches!(value, b'\n' | b'\r' | b'\t') || (0x20..=0x7e).contains(&value) || value >= 0x80
}

fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn read_f64(bytes: &[u8]) -> f64 {
    f64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn decodes_biff8_unicode_string() {
        let bytes = [2, 0, 1, 0x2d, 0x4e, 0x87, 0x65];
        assert_eq!(decode_biff_string(&bytes, 936).unwrap().0, "中文");
    }

    #[test]
    fn decodes_rk_integer() {
        assert_eq!(decode_rk(&[0x2a, 0, 0, 0]), "10");
    }

    #[test]
    fn parses_all_entries_in_a_word_piece_table() {
        let mut word = vec![0_u8; 64];
        write_utf16(&mut word, 16, "正文");
        write_utf16(&mut word, 24, "内容");

        let mut clx = vec![0x02];
        clx.extend_from_slice(&28_u32.to_le_bytes());
        for cp in [0_u32, 2, 4] {
            clx.extend_from_slice(&cp.to_le_bytes());
        }
        for offset in [16_u32, 24] {
            clx.extend_from_slice(&0_u16.to_le_bytes());
            clx.extend_from_slice(&offset.to_le_bytes());
            clx.extend_from_slice(&0_u16.to_le_bytes());
        }

        assert_eq!(parse_clx(&word, &clx).as_deref(), Some("正文内容"));
    }

    #[test]
    fn parses_a_compressed_word_piece() {
        let mut word = vec![0_u8; 64];
        word[0x06..0x08].copy_from_slice(&0x0409_u16.to_le_bytes());
        word[32..36].copy_from_slice(b"caf\xe9");
        let mut clx = vec![0x02];
        clx.extend_from_slice(&16_u32.to_le_bytes());
        clx.extend_from_slice(&0_u32.to_le_bytes());
        clx.extend_from_slice(&4_u32.to_le_bytes());
        clx.extend_from_slice(&0_u16.to_le_bytes());
        clx.extend_from_slice(&(0x4000_0000_u32 | 64).to_le_bytes());
        clx.extend_from_slice(&0_u16.to_le_bytes());

        assert_eq!(parse_clx(&word, &clx).as_deref(), Some("café"));
    }

    #[test]
    fn reads_utf16_text_only_from_the_fib_body_range() {
        let mut word = vec![0_u8; 96];
        word[0x18..0x1c].copy_from_slice(&48_u32.to_le_bytes());
        word[0x1c..0x20].copy_from_slice(&72_u32.to_le_bytes());
        write_utf16(&mut word, 48, "[请输入公司名称]");

        assert_eq!(
            scan_word_text_range(&word).as_deref(),
            Some("[请输入公司名称]")
        );
    }

    #[test]
    fn rejects_empty_word_ranges_and_wps_metadata() {
        let mut word = vec![0_u8; 64];
        word[0x18..0x1c].copy_from_slice(&48_u32.to_le_bytes());
        word[0x1c..0x20].copy_from_slice(&50_u32.to_le_bytes());
        word[48..50].copy_from_slice(&[b'\r', 0]);

        assert!(scan_word_text_range(&word).is_none());
        assert!(choose_text([("KSOProductBuildVer 2052-11.1.0.8865".to_owned(), 0)]).is_none());
    }

    #[test]
    fn extracts_doc_body_without_returning_ole_metadata() {
        let mut word = vec![0_u8; 96];
        word[0x18..0x1c].copy_from_slice(&48_u32.to_le_bytes());
        word[0x1c..0x20].copy_from_slice(&72_u32.to_le_bytes());
        write_utf16(&mut word, 48, "会议决定月底整改");

        let mut compound = CompoundFile::create(Cursor::new(Vec::new())).unwrap();
        compound
            .create_stream("/WordDocument")
            .unwrap()
            .write_all(&word)
            .unwrap();
        compound
            .create_stream("/\u{0005}SummaryInformation")
            .unwrap()
            .write_all(b"KSOProductBuildVer 2052-11.1.0.8865")
            .unwrap();
        compound.flush().unwrap();
        let bytes = compound.into_inner().into_inner();

        assert_eq!(extract_doc(&bytes).unwrap(), "会议决定月底整改");
    }

    #[test]
    fn keeps_word_paragraph_boundaries() {
        assert_eq!(
            clean_word_text("标题\r正文第一段\r正文第二段"),
            "标题\n正文第一段\n正文第二段"
        );
    }

    fn write_utf16(output: &mut [u8], offset: usize, value: &str) {
        for (index, character) in value.encode_utf16().enumerate() {
            let start = offset + index * 2;
            output[start..start + 2].copy_from_slice(&character.to_le_bytes());
        }
    }
}
