use super::types::LlmError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// 按字节增量解析 SSE；保留不完整行，因此 UTF-8 字符可跨网络分片。
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    buffer: Vec<u8>,
    data_lines: Vec<String>,
    event: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, LlmError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if let Some(event) = self.process_line(&line)? {
                events.push(event);
            }
        }

        Ok(events)
    }

    /// 处理没有结尾换行的最后一个事件。
    pub fn finish(&mut self) -> Result<Option<SseEvent>, LlmError> {
        if !self.buffer.is_empty() {
            let mut line = std::mem::take(&mut self.buffer);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if let Some(event) = self.process_line(&line)? {
                return Ok(Some(event));
            }
        }
        Ok(self.dispatch())
    }

    fn process_line(&mut self, line: &[u8]) -> Result<Option<SseEvent>, LlmError> {
        if line.is_empty() {
            return Ok(self.dispatch());
        }
        if line[0] == b':' {
            return Ok(None);
        }

        let (field, value) = match line.iter().position(|byte| *byte == b':') {
            Some(colon) => {
                let value = &line[colon + 1..];
                let value = value.strip_prefix(b" ").unwrap_or(value);
                (&line[..colon], value)
            }
            None => (line, &[][..]),
        };
        let value = std::str::from_utf8(value)
            .map_err(|_| LlmError::InvalidStream("SSE 字段不是有效 UTF-8".into()))?;

        match field {
            b"data" => self.data_lines.push(value.to_owned()),
            b"event" => self.event = Some(value.to_owned()),
            _ => {}
        }
        Ok(None)
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        if self.data_lines.is_empty() {
            self.event = None;
            return None;
        }
        Some(SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data_lines).join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_recorded_multiline_crlf_and_done() {
        let recorded = b": keep-alive\r\n\r\nevent: message\r\ndata: {\"choices\":[\r\ndata:  {\"delta\":{}}]}\r\n\r\ndata: [DONE]\r\n\r\n";
        let mut parser = SseParser::new();
        let mut events = Vec::new();
        for byte in recorded {
            events.extend(parser.feed(&[*byte]).unwrap());
        }

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.as_deref(), Some("message"));
        assert_eq!(events[0].data, "{\"choices\":[\n {\"delta\":{}}]}");
        assert_eq!(events[1].data, "[DONE]");
    }

    #[test]
    fn preserves_utf8_split_across_network_chunks() {
        let recorded = "data: {\"delta\":\"你好\"}\n\n".as_bytes();
        let split = recorded.iter().position(|byte| *byte == 0xBD).unwrap();
        let mut parser = SseParser::new();
        assert!(parser.feed(&recorded[..split]).unwrap().is_empty());
        let events = parser.feed(&recorded[split..]).unwrap();
        assert_eq!(events[0].data, "{\"delta\":\"你好\"}");
    }

    #[test]
    fn finish_flushes_unterminated_data() {
        let mut parser = SseParser::new();
        assert!(parser.feed(b"data: final").unwrap().is_empty());
        assert_eq!(parser.finish().unwrap().unwrap().data, "final");
    }
}
