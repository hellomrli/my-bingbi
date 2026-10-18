use crate::LlmError;

const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_JSON_SUBSTRING_CANDIDATES: usize = 64;

#[derive(Default)]
pub(crate) struct SseDecoder {
    pending: Vec<u8>,
    data_lines: Vec<Vec<u8>>,
}

impl SseDecoder {
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, LlmError> {
        self.pending.extend_from_slice(chunk);
        self.ensure_bounded()?;
        let mut events = Vec::new();
        while let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line = self.pending.drain(..=newline).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.consume_line(&line, &mut events)?;
        }
        Ok(events)
    }

    pub(crate) fn finish(mut self) -> Result<Vec<String>, LlmError> {
        let mut events = Vec::new();
        if !self.pending.is_empty() {
            let mut line = std::mem::take(&mut self.pending);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.consume_line(&line, &mut events)?;
        }
        self.dispatch(&mut events)?;
        Ok(events)
    }

    fn consume_line(&mut self, line: &[u8], events: &mut Vec<String>) -> Result<(), LlmError> {
        if line.is_empty() {
            return self.dispatch(events);
        }
        if line.first() == Some(&b':') {
            return Ok(());
        }
        if let Some(data) = line.strip_prefix(b"data:") {
            self.data_lines
                .push(data.strip_prefix(b" ").unwrap_or(data).to_vec());
            self.ensure_bounded()?;
        }
        Ok(())
    }

    fn dispatch(&mut self, events: &mut Vec<String>) -> Result<(), LlmError> {
        if self.data_lines.is_empty() {
            return Ok(());
        }
        let joined = self
            .data_lines
            .drain(..)
            .reduce(|mut combined, line| {
                combined.push(b'\n');
                combined.extend(line);
                combined
            })
            .unwrap_or_default();
        let event = String::from_utf8(joined).map_err(|_| LlmError::InvalidResponse)?;
        events.push(event);
        Ok(())
    }

    fn ensure_bounded(&self) -> Result<(), LlmError> {
        let data_size = self.data_lines.iter().map(Vec::len).sum::<usize>();
        if self.pending.len() + data_size > MAX_FRAME_BYTES {
            Err(LlmError::InvalidResponse)
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
pub(crate) struct LineDecoder {
    pending: Vec<u8>,
}

impl LineDecoder {
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, LlmError> {
        self.pending.extend_from_slice(chunk);
        if self.pending.len() > MAX_FRAME_BYTES {
            return Err(LlmError::InvalidResponse);
        }
        let mut lines = Vec::new();
        while let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line = self.pending.drain(..=newline).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !line.is_empty() {
                lines.push(String::from_utf8(line).map_err(|_| LlmError::InvalidResponse)?);
            }
        }
        Ok(lines)
    }

    pub(crate) fn finish(self) -> Result<Vec<String>, LlmError> {
        if self.pending.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![
                String::from_utf8(self.pending).map_err(|_| LlmError::InvalidResponse)?,
            ])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_decoder_handles_crlf_comments_and_fragmented_utf8() {
        let payload = "data: {\"text\":\"你好\"}\r\n\r\n: keepalive\n\ndata: [DONE]\n\n";
        let bytes = payload.as_bytes();
        let split = payload.find('你').expect("character should exist") + 1;
        let mut decoder = SseDecoder::default();
        assert!(
            decoder.push(&bytes[..split]).unwrap().is_empty(),
            "a half UTF-8 sequence should not emit an SSE event"
        );
        let events = decoder.push(&bytes[split..]).unwrap();
        assert_eq!(events, ["{\"text\":\"你好\"}", "[DONE]"]);
    }

    #[test]
    fn line_decoder_keeps_partial_json_until_newline() {
        let mut decoder = LineDecoder::default();
        assert!(
            decoder.push(b"{\"a\":").unwrap().is_empty(),
            "an incomplete JSON line should not emit an event"
        );
        assert_eq!(decoder.push(b"1}\n{\"b\":2").unwrap(), ["{\"a\":1}"]);
        assert_eq!(decoder.finish().unwrap(), ["{\"b\":2"]);
    }
}

/// Parses model JSON output with tolerant fallbacks for common LLM formatting slips.
///
/// Attempts in order: the raw text, the text inside a code fence, the text with unescaped
/// newlines inside string values repaired, and finally balanced object/array values extracted
/// from surrounding prose. Any successful attempt returns the parsed value.
///
/// # Errors
///
/// Returns the parse error of the raw text when every fallback fails.
pub fn parse_model_json<T: serde::de::DeserializeOwned>(
    content: &str,
) -> Result<T, serde_json::Error> {
    let trimmed = content.trim();
    let fenced = strip_code_fence(trimmed);
    let candidates = [
        trimmed.to_owned(),
        fenced.to_owned(),
        escape_string_newlines(trimmed),
        escape_string_newlines(fenced),
    ];
    for candidate in &candidates {
        if let Ok(value) = serde_json::from_str(candidate) {
            return Ok(value);
        }
    }

    for source in [trimmed, fenced] {
        if let Some(value) = parse_json_substring(source) {
            return Ok(value);
        }
    }

    serde_json::from_str(content)
}

fn strip_code_fence(content: &str) -> &str {
    let body = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .unwrap_or(content);
    body.strip_suffix("```").unwrap_or(body).trim()
}

/// Replaces unescaped newlines inside JSON string values with the `\n` escape.
fn escape_string_newlines(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut in_string = false;
    let mut escaped = false;
    for character in content.chars() {
        if in_string {
            if escaped {
                output.push(character);
                escaped = false;
                continue;
            }
            match character {
                '\\' => {
                    output.push(character);
                    escaped = true;
                }
                '"' => {
                    in_string = false;
                    output.push(character);
                }
                '\n' | '\r' => output.push_str("\\n"),
                _ => output.push(character),
            }
        } else {
            if character == '"' {
                in_string = true;
            }
            output.push(character);
        }
    }
    output
}

/// Parses balanced object/array slices while preserving nested values and braces in strings.
fn parse_json_substring<T: serde::de::DeserializeOwned>(content: &str) -> Option<T> {
    let bytes = content.as_bytes();
    let mut candidate_count = 0;
    for (start, byte) in bytes.iter().copied().enumerate() {
        if !matches!(byte, b'{' | b'[') {
            continue;
        }
        candidate_count += 1;
        if candidate_count > MAX_JSON_SUBSTRING_CANDIDATES {
            break;
        }
        let mut stack = vec![byte];
        let mut in_string = false;
        let mut escaped = false;
        for (index, current) in bytes.iter().copied().enumerate().skip(start + 1) {
            if in_string {
                if escaped {
                    escaped = false;
                } else if current == b'\\' {
                    escaped = true;
                } else if current == b'"' {
                    in_string = false;
                }
                continue;
            }
            match current {
                b'"' => in_string = true,
                b'{' | b'[' => stack.push(current),
                b'}' | b']' => {
                    let matches = stack.last().is_some_and(|opening| {
                        (*opening == b'{' && current == b'}')
                            || (*opening == b'[' && current == b']')
                    });
                    if !matches {
                        break;
                    }
                    stack.pop();
                    if stack.is_empty() {
                        let candidate = &content[start..=index];
                        if let Ok(value) = serde_json::from_str(candidate) {
                            return Some(value);
                        }
                        let repaired = escape_string_newlines(candidate);
                        if let Ok(value) = serde_json::from_str(&repaired) {
                            return Some(value);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod json_tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize)]
    struct Sample {
        value: String,
    }

    #[test]
    fn parses_plain_json() {
        assert_eq!(
            parse_model_json::<Sample>(r#"{"value":"ok"}"#).expect("plain json should parse"),
            Sample {
                value: "ok".to_owned()
            }
        );
    }

    #[test]
    fn strips_code_fences() {
        let fenced = "```json\n{\"value\":\"ok\"}\n```";
        assert_eq!(
            parse_model_json::<Sample>(fenced).expect("fenced json should parse"),
            Sample {
                value: "ok".to_owned()
            }
        );
    }

    #[test]
    fn repairs_unescaped_newlines_inside_strings() {
        let broken = "{\"value\":\"第一行\n第二行\"}";
        assert_eq!(
            parse_model_json::<Sample>(broken).expect("newlines should be repaired"),
            Sample {
                value: "第一行\n第二行".to_owned()
            }
        );
    }

    #[test]
    fn extracts_the_brace_substring_from_trailing_prose() {
        let noisy = "好的，结果如下：{\"value\":\"ok\"} 以上。";
        assert_eq!(
            parse_model_json::<Sample>(noisy).expect("substring should parse"),
            Sample {
                value: "ok".to_owned()
            }
        );
    }

    #[test]
    fn extracts_arrays_from_surrounding_prose() {
        let noisy = "审核结果如下：[{\"value\":\"first\"},{\"value\":\"second\"}]。";
        let values = parse_model_json::<Vec<Sample>>(noisy).expect("array substring should parse");
        assert_eq!(values.len(), 2);
        assert_eq!(values[1].value, "second");
    }

    #[test]
    fn preserves_braces_inside_json_strings() {
        let noisy = "结果：{\"value\":\"文本中的 {花括号} 保留\"}。";
        let value = parse_model_json::<Sample>(noisy).expect("braces in strings should parse");
        assert_eq!(value.value, "文本中的 {花括号} 保留");
    }
}
