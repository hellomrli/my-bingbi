use super::super::ExportError;
use flate2::read::ZlibDecoder;
use std::io::Read;

const MAX_PDF_STREAM_BYTES: usize = 16 * 1024 * 1024;

/// Extracts text operators from PDF page content streams. This intentionally does not render a
/// page or execute JavaScript: only literal/hex strings associated with `Tj`, `TJ`, `'` and `"`
/// operators are considered. Flate-compressed streams are supported without an external PDF
/// process; unrecognised filters are skipped safely.
pub(super) fn extract(bytes: &[u8]) -> Result<String, ExportError> {
    if let Ok(Ok(text)) = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(bytes)) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    let mut output = String::new();
    let mut cursor = 0;
    while let Some(stream_start) = find_keyword(bytes, b"stream", cursor) {
        let data_start = skip_stream_eol(bytes, stream_start + b"stream".len());
        let Some(end_offset) = find_keyword(bytes, b"endstream", data_start) else {
            break;
        };
        let raw = &bytes[data_start..end_offset];
        let dictionary_start = find_dictionary_start(bytes, cursor, stream_start);
        let dictionary = &bytes[dictionary_start..stream_start];
        let decoded = if dictionary
            .windows(b"FlateDecode".len())
            .any(|part| part == b"FlateDecode")
        {
            let mut decompressed = Vec::new();
            let decoder = ZlibDecoder::new(raw);
            let mut decoder = decoder.take((MAX_PDF_STREAM_BYTES + 1) as u64);
            if decoder.read_to_end(&mut decompressed).is_err() {
                cursor = end_offset + b"endstream".len();
                continue;
            }
            if decompressed.len() > MAX_PDF_STREAM_BYTES {
                return Err(ExportError::PackageTooLarge);
            }
            decompressed
        } else if dictionary
            .windows(b"Filter".len())
            .any(|part| part == b"Filter")
        {
            // ASCII85/LZW/etc. need a full PDF codec. Skip those streams instead of treating
            // compressed bytes as prompt text.
            cursor = end_offset + b"endstream".len();
            continue;
        } else {
            if raw.len() > MAX_PDF_STREAM_BYTES {
                return Err(ExportError::PackageTooLarge);
            }
            raw.to_vec()
        };
        parse_content_stream(&decoded, &mut output);
        cursor = end_offset + b"endstream".len();
    }
    if output.trim().is_empty() {
        // Some producers put a plain text trailer after a minimal PDF header. It is safe to use
        // this only when no stream yielded text and the bytes are valid UTF-8.
        let body = String::from_utf8_lossy(bytes);
        let candidate = body
            .lines()
            .filter(|line| !line.trim_start().starts_with('%'))
            .collect::<Vec<_>>()
            .join("\n");
        if candidate.contains("BT") && candidate.contains("ET") {
            parse_content_stream(candidate.as_bytes(), &mut output);
        }
    }
    if output.trim().is_empty() {
        Err(ExportError::InvalidReference)
    } else {
        Ok(output)
    }
}

fn find_keyword(bytes: &[u8], keyword: &[u8], from: usize) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(keyword.len())
        .position(|window| window == keyword)
        .map(|offset| from + offset)
}

fn find_dictionary_start(bytes: &[u8], from: usize, stream_start: usize) -> usize {
    let mut index = stream_start;
    while index >= 2 {
        if bytes[index - 2..index] == *b"<<" {
            return index - 2;
        }
        if bytes[index - 2..index] == *b">>" {
            break;
        }
        index -= 1;
        if index <= from {
            break;
        }
    }
    from
}

fn skip_stream_eol(bytes: &[u8], mut index: usize) -> usize {
    if bytes.get(index) == Some(&b'\r') {
        index += 1;
        if bytes.get(index) == Some(&b'\n') {
            index += 1;
        }
    } else if bytes.get(index) == Some(&b'\n') {
        index += 1;
    }
    index
}

fn parse_content_stream(bytes: &[u8], output: &mut String) {
    let mut index = 0;
    let mut operands: Vec<Operand> = Vec::new();
    while index < bytes.len() {
        skip_whitespace_and_comments(bytes, &mut index);
        if index >= bytes.len() {
            break;
        }
        match bytes[index] {
            b'(' => {
                if let Some((value, next)) = parse_literal(bytes, index) {
                    operands.push(Operand::Text(value));
                    index = next;
                } else {
                    index += 1;
                }
            }
            b'<' if bytes.get(index + 1) != Some(&b'<') => {
                if let Some((value, next)) = parse_hex(bytes, index) {
                    operands.push(Operand::Text(value));
                    index = next;
                } else {
                    index += 1;
                }
            }
            b'[' => {
                let (items, next) = parse_array(bytes, index);
                operands.push(Operand::Array(items));
                index = next;
            }
            _ => {
                let start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && !matches!(bytes[index], b'[' | b']' | b'(' | b'<' | b'>')
                {
                    index += 1;
                }
                if start == index {
                    index += 1;
                    continue;
                }
                let token = &bytes[start..index];
                if is_text_operator(token) {
                    emit_operator(token, &mut operands, output);
                } else if token == b"Td" || token == b"TD" || token == b"Tm" || token == b"T*" {
                    // Positioning operators generally indicate a new visual line. Do not add a
                    // newline before the first text run, and avoid duplicate blank lines.
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    operands.clear();
                } else if token == b"BT" || token == b"ET" || token == b"Tf" || token == b"Tc" {
                    operands.clear();
                } else {
                    operands.push(Operand::Number);
                    if operands.len() > 64 {
                        operands.drain(..operands.len() - 16);
                    }
                }
            }
        }
    }
}

fn is_text_operator(token: &[u8]) -> bool {
    matches!(token, b"Tj" | b"TJ" | b"'" | b"\"")
}

fn emit_operator(operator: &[u8], operands: &mut Vec<Operand>, output: &mut String) {
    let mut text = String::new();
    match operator {
        b"TJ" => {
            if let Some(Operand::Array(items)) = operands.pop() {
                for item in items {
                    if let Operand::Text(value) = item {
                        text.push_str(&value);
                    }
                }
            }
        }
        _ => {
            if let Some(Operand::Text(value)) = operands.pop() {
                text = value;
            }
        }
    }
    operands.clear();
    if text.is_empty() {
        return;
    }
    if !output.is_empty() && !output.ends_with('\n') && (operator == b"'" || operator == b"\"") {
        output.push('\n');
    }
    output.push_str(&text);
    if operator == b"Tj" || operator == b"TJ" {
        // A separate Tj commonly represents a line in generated PDFs. A following positioning
        // operator can add another newline, which is collapsed during final normalization.
        output.push('\n');
    }
}

#[derive(Debug)]
enum Operand {
    Text(String),
    Array(Vec<Operand>),
    Number,
}

fn parse_array(bytes: &[u8], mut index: usize) -> (Vec<Operand>, usize) {
    let mut items = Vec::new();
    index += 1;
    while index < bytes.len() {
        skip_whitespace_and_comments(bytes, &mut index);
        if index >= bytes.len() {
            break;
        }
        if bytes[index] == b']' {
            return (items, index + 1);
        }
        if bytes[index] == b'(' {
            if let Some((value, next)) = parse_literal(bytes, index) {
                items.push(Operand::Text(value));
                index = next;
                continue;
            }
        } else if bytes[index] == b'<' && bytes.get(index + 1) != Some(&b'<') {
            if let Some((value, next)) = parse_hex(bytes, index) {
                items.push(Operand::Text(value));
                index = next;
                continue;
            }
        }
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b']' {
            index += 1;
        }
        items.push(Operand::Number);
    }
    (items, index)
}

fn parse_literal(bytes: &[u8], mut index: usize) -> Option<(String, usize)> {
    if bytes.get(index) != Some(&b'(') {
        return None;
    }
    index += 1;
    let mut raw = Vec::new();
    let mut depth = 1_u32;
    while index < bytes.len() {
        let value = bytes[index];
        index += 1;
        match value {
            b'(' => {
                depth = depth.saturating_add(1);
                raw.push(value);
            }
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((decode_pdf_bytes(&raw), index));
                }
                raw.push(value);
            }
            b'\\' => {
                let Some(&escaped) = bytes.get(index) else {
                    break;
                };
                index += 1;
                match escaped {
                    b'n' => raw.push(b'\n'),
                    b'r' => raw.push(b'\r'),
                    b't' => raw.push(b'\t'),
                    b'b' => raw.push(0x08),
                    b'f' => raw.push(0x0c),
                    b'\r' => {
                        if bytes.get(index) == Some(&b'\n') {
                            index += 1;
                        }
                    }
                    b'\n' => {}
                    b'0'..=b'7' => {
                        let mut octal = vec![escaped];
                        for _ in 0..2 {
                            if bytes
                                .get(index)
                                .is_some_and(|value| (b'0'..=b'7').contains(value))
                            {
                                octal.push(bytes[index]);
                                index += 1;
                            } else {
                                break;
                            }
                        }
                        if let Ok(value) = u8::from_str_radix(std::str::from_utf8(&octal).ok()?, 8)
                        {
                            raw.push(value);
                        }
                    }
                    _ => raw.push(escaped),
                }
            }
            _ => raw.push(value),
        }
    }
    None
}

fn parse_hex(bytes: &[u8], mut index: usize) -> Option<(String, usize)> {
    if bytes.get(index) != Some(&b'<') || bytes.get(index + 1) == Some(&b'<') {
        return None;
    }
    index += 1;
    let mut nibbles = Vec::new();
    while index < bytes.len() && bytes[index] != b'>' {
        if !bytes[index].is_ascii_whitespace() {
            nibbles.push(bytes[index]);
        }
        index += 1;
    }
    if index >= bytes.len() {
        return None;
    }
    if nibbles.len() % 2 != 0 {
        nibbles.push(b'0');
    }
    let mut raw = Vec::with_capacity(nibbles.len() / 2);
    for pair in nibbles.chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        raw.push((high << 4) | low);
    }
    Some((decode_pdf_bytes(&raw), index + 1))
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn decode_pdf_bytes(raw: &[u8]) -> String {
    if raw.starts_with(&[0xfe, 0xff]) {
        let units = raw[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]));
        return String::from_utf16_lossy(&units.collect::<Vec<_>>());
    }
    if raw.starts_with(&[0xff, 0xfe]) {
        let units = raw[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
        return String::from_utf16_lossy(&units.collect::<Vec<_>>());
    }
    String::from_utf8_lossy(raw).into_owned()
}

fn skip_whitespace_and_comments(bytes: &[u8], index: &mut usize) {
    loop {
        while *index < bytes.len() && bytes[*index].is_ascii_whitespace() {
            *index += 1;
        }
        if bytes.get(*index) != Some(&b'%') {
            return;
        }
        while *index < bytes.len() && !matches!(bytes[*index], b'\r' | b'\n') {
            *index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_literal_and_hex_pdf_text() {
        let pdf = b"%PDF-1.4\n1 0 obj\n<< /Length 31 >>\nstream\nBT (Hello) Tj <576f726c64> Tj ET\nendstream\nendobj\n%%EOF";
        let text = extract(pdf).unwrap();
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn decodes_utf16_hex_strings() {
        assert_eq!(decode_pdf_bytes(&[0xfe, 0xff, 0x4e, 0x2d]), "中");
    }
}
