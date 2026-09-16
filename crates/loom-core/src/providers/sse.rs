//! Minimal, dependency-free SSE line parser.
//!
//! Kept as pure functions so the dialect quirks (multi-line data, comment
//! keep-alives, `[DONE]` sentinels) are unit-testable without a network.

/// Splits a byte chunk into complete lines, carrying the remainder into the
/// next call. Feed with `\n`-terminated content.
pub struct LineBuffer {
    buffer: String,
}

impl Default for LineBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl LineBuffer {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Appends a chunk and returns every complete line it produced.
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(chunk);
        let mut lines = Vec::new();
        while let Some(index) = self.buffer.find('\n') {
            let line = self.buffer[..index].trim_end_matches('\r').to_string();
            self.buffer.drain(..=index);
            lines.push(line);
        }
        lines
    }
}

/// Extracts the payload of an SSE `data:` field. Returns `None` for blank
/// lines, comments, and other fields (`event:`, `id:`, `retry:`).
pub fn data_field(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with(':') {
        return None;
    }
    let rest = trimmed.strip_prefix("data:")?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_buffer_handles_split_chunks() {
        let mut buffer = LineBuffer::new();
        assert!(buffer.push("data: {\"a\":").is_empty());
        let lines = buffer.push("1}\n\ndata: [DONE]\n");
        assert_eq!(lines, vec!["data: {\"a\":1}", "", "data: [DONE]"]);
    }

    #[test]
    fn data_field_ignores_non_data_lines() {
        assert_eq!(data_field("data: hello"), Some("hello"));
        assert_eq!(data_field("data:hello"), Some("hello"));
        assert_eq!(data_field(": keep-alive"), None);
        assert_eq!(data_field("event: message"), None);
        assert_eq!(data_field(""), None);
    }

    #[test]
    fn done_sentinel_is_detectable() {
        assert_eq!(data_field("data: [DONE]"), Some("[DONE]"));
    }
}
