//! Minimal Server-Sent-Events parser plus an accumulator that rebuilds the final `Message`
//! object from the Messages API stream events (`message_start`, `content_block_start`,
//! `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`, `ping`, `error`).
//!
//! The accumulator keeps every content block as raw JSON so a paused turn (`stop_reason ==
//! "pause_turn"`) can be echoed back unchanged, thinking signatures included.

use serde_json::{json, Map, Value};

/// Splits a byte stream into SSE events. Feed chunks with `push`, drain with `next_event`.
#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    /// Returns the next complete event (terminated by a blank line), if any.
    pub fn next_event(&mut self) -> Option<SseEvent> {
        // Normalise CRLF so we only look for "\n\n".
        let pos = find_blank_line(&self.buf)?;
        let raw: Vec<u8> = self.buf.drain(..pos.end).collect();
        let block = String::from_utf8_lossy(&raw[..pos.start]).into_owned();
        let mut event = None;
        let mut data_lines = Vec::new();
        for line in block.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                event = Some(v.trim().to_string());
            } else if let Some(v) = line.strip_prefix("data:") {
                data_lines.push(v.strip_prefix(' ').unwrap_or(v).to_string());
            } // comments (":...") and other fields are ignored
        }
        Some(SseEvent {
            event,
            data: data_lines.join("\n"),
        })
    }
}

struct Span {
    start: usize,
    end: usize,
}

fn find_blank_line(buf: &[u8]) -> Option<Span> {
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == b'\n' {
            if i + 1 < buf.len() && buf[i + 1] == b'\n' {
                return Some(Span {
                    start: i,
                    end: i + 2,
                });
            }
            if i + 3 <= buf.len() && &buf[i + 1..(i + 3).min(buf.len())] == b"\r\n" {
                return Some(Span {
                    start: i,
                    end: i + 3,
                });
            }
        }
        if buf[i] == b'\r' && i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(Span {
                start: i,
                end: i + 4,
            });
        }
        i += 1;
    }
    None
}

/// Rebuilt message, in the same shape the non-streaming endpoint returns.
#[derive(Debug, Clone, Default)]
pub struct Message {
    pub id: String,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_details: Option<Value>,
    pub content: Vec<Value>,
    pub usage: Map<String, Value>,
}

impl Message {
    /// Text of the last `text` block (structured outputs put the JSON there).
    pub fn last_text(&self) -> Option<&str> {
        self.content
            .iter()
            .rev()
            .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str)
    }
}

#[derive(Default)]
pub struct MessageAccumulator {
    pub message: Message,
    partial_json: Vec<String>, // per block index, for input_json_delta
    done: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("stream error event: {0}")]
    ApiError(Value),
    #[error("malformed event: {0}")]
    Malformed(String),
}

impl MessageAccumulator {
    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn apply(&mut self, ev: &SseEvent) -> Result<(), StreamError> {
        if ev.data.is_empty() {
            return Ok(());
        }
        let v: Value = serde_json::from_str(&ev.data)
            .map_err(|e| StreamError::Malformed(format!("{e}: {}", ev.data)))?;
        let typ = v.get("type").and_then(Value::as_str).unwrap_or("");
        match typ {
            "message_start" => {
                let m = v
                    .get("message")
                    .ok_or_else(|| StreamError::Malformed(ev.data.clone()))?;
                self.message.id = m
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                self.message.model = m
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if let Some(Value::Object(u)) = m.get("usage") {
                    merge_usage(&mut self.message.usage, u);
                }
            }
            "content_block_start" => {
                let idx = index_of(&v)?;
                let block = v.get("content_block").cloned().unwrap_or(json!({}));
                while self.message.content.len() <= idx {
                    self.message.content.push(json!({}));
                    self.partial_json.push(String::new());
                }
                self.message.content[idx] = block;
            }
            "content_block_delta" => {
                let idx = index_of(&v)?;
                let delta = v
                    .get("delta")
                    .ok_or_else(|| StreamError::Malformed(ev.data.clone()))?;
                let block = self.message.content.get_mut(idx).ok_or_else(|| {
                    StreamError::Malformed(format!("delta for unknown block {idx}"))
                })?;
                match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text_delta" => append_str(block, "text", delta.get("text")),
                    "thinking_delta" => append_str(block, "thinking", delta.get("thinking")),
                    "signature_delta" => {
                        if let Some(s) = delta.get("signature").and_then(Value::as_str) {
                            block["signature"] = Value::String(s.to_string());
                        }
                    }
                    "input_json_delta" => {
                        if let Some(s) = delta.get("partial_json").and_then(Value::as_str) {
                            self.partial_json[idx].push_str(s);
                        }
                    }
                    "citations_delta" => {
                        if let Some(c) = delta.get("citation") {
                            let arr = block.as_object_mut().and_then(|o| {
                                o.entry("citations")
                                    .or_insert_with(|| json!([]))
                                    .as_array_mut()
                            });
                            if let Some(arr) = arr {
                                arr.push(c.clone());
                            }
                        }
                    }
                    _ => {} // unknown delta types are ignored, forward compatible
                }
            }
            "content_block_stop" => {
                let idx = index_of(&v)?;
                let partial = std::mem::take(&mut self.partial_json[idx]);
                let block = &mut self.message.content[idx];
                let is_tool_like = matches!(
                    block.get("type").and_then(Value::as_str),
                    Some("tool_use") | Some("server_tool_use") | Some("mcp_tool_use")
                );
                if is_tool_like {
                    let input: Value = if partial.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&partial)
                            .map_err(|e| StreamError::Malformed(format!("tool input json: {e}")))?
                    };
                    block["input"] = input;
                }
            }
            "message_delta" => {
                if let Some(d) = v.get("delta") {
                    if let Some(s) = d.get("stop_reason").and_then(Value::as_str) {
                        self.message.stop_reason = Some(s.to_string());
                    }
                    if let Some(sd) = d.get("stop_details") {
                        if !sd.is_null() {
                            self.message.stop_details = Some(sd.clone());
                        }
                    }
                }
                if let Some(Value::Object(u)) = v.get("usage") {
                    merge_usage(&mut self.message.usage, u);
                }
            }
            "message_stop" => self.done = true,
            "ping" => {}
            "error" => return Err(StreamError::ApiError(v.get("error").cloned().unwrap_or(v))),
            _ => {}
        }
        Ok(())
    }
}

fn index_of(v: &Value) -> Result<usize, StreamError> {
    v.get("index")
        .and_then(Value::as_u64)
        .map(|i| i as usize)
        .ok_or_else(|| StreamError::Malformed("event without index".into()))
}

fn append_str(block: &mut Value, key: &str, piece: Option<&Value>) {
    if let Some(s) = piece.and_then(Value::as_str) {
        match block.get_mut(key) {
            Some(Value::String(existing)) => existing.push_str(s),
            _ => block[key] = Value::String(s.to_string()),
        }
    }
}

/// `message_delta.usage` carries cumulative counters for the turn; later values replace earlier ones.
fn merge_usage(into: &mut Map<String, Value>, from: &Map<String, Value>) {
    for (k, v) in from {
        if !v.is_null() {
            into.insert(k.clone(), v.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-opus-5-5\",\"usage\":{\"input_tokens\":10,\"cache_read_input_tokens\":5}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig==\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"server_tool_use\",\"id\":\"srvtoolu_1\",\"name\":\"web_search\",\"input\":{}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"query\\\": \\\"AS\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"ML 10-K\\\"}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: ping\ndata: {\"type\":\"ping\"}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\"{\\\"a\\\":\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\" 1}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_details\":null},\"usage\":{\"output_tokens\":42,\"server_tool_use\":{\"web_search_requests\":1}}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );

    #[test]
    fn rebuilds_message_from_chunks_of_any_size() {
        for chunk in [1usize, 7, 64, 4096] {
            let mut p = SseParser::default();
            let mut acc = MessageAccumulator::default();
            for c in STREAM.as_bytes().chunks(chunk) {
                p.push(c);
                while let Some(ev) = p.next_event() {
                    acc.apply(&ev).unwrap();
                }
            }
            assert!(acc.is_done());
            let m = &acc.message;
            assert_eq!(m.id, "msg_1");
            assert_eq!(m.stop_reason.as_deref(), Some("end_turn"));
            assert_eq!(m.content.len(), 3);
            assert_eq!(m.content[0]["signature"], "sig==");
            assert_eq!(m.content[1]["input"]["query"], "ASML 10-K");
            assert_eq!(m.last_text(), Some("{\"a\": 1}"));
            assert_eq!(m.usage["input_tokens"], 10);
            assert_eq!(m.usage["output_tokens"], 42);
            assert_eq!(m.usage["server_tool_use"]["web_search_requests"], 1);
        }
    }

    #[test]
    fn error_event_surfaces() {
        let mut p = SseParser::default();
        let mut acc = MessageAccumulator::default();
        p.push(b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n");
        let ev = p.next_event().unwrap();
        assert!(matches!(acc.apply(&ev), Err(StreamError::ApiError(_))));
    }

    #[test]
    fn handles_crlf() {
        let mut p = SseParser::default();
        p.push(b"event: ping\r\ndata: {\"type\":\"ping\"}\r\n\r\n");
        let ev = p.next_event().unwrap();
        assert_eq!(ev.event.as_deref(), Some("ping"));
        assert_eq!(ev.data, "{\"type\":\"ping\"}");
    }
}
