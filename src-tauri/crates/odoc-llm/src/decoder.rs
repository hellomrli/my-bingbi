use crate::LlmError;

const MAX_FRAME_BYTES: usize = 1024 * 1024;

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
        assert!(decoder.push(&bytes[..split]).unwrap().is_empty());
        let events = decoder.push(&bytes[split..]).unwrap();
        assert_eq!(events, ["{\"text\":\"你好\"}", "[DONE]"]);
    }

    #[test]
    fn line_decoder_keeps_partial_json_until_newline() {
        let mut decoder = LineDecoder::default();
        assert!(decoder.push(b"{\"a\":").unwrap().is_empty());
        assert_eq!(decoder.push(b"1}\n{\"b\":2").unwrap(), ["{\"a\":1}"]);
        assert_eq!(decoder.finish().unwrap(), ["{\"b\":2"]);
    }
}
