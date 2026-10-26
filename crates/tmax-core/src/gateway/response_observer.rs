//! Bounded response inspection for completion and allowlisted usage evidence.
use super::INSPECTION_BYTES;
use crate::diagnostics::Record;
use serde_json::Value;
use std::collections::BTreeMap;
pub(super) struct Observer {
    protocol: String,
    sse: bool,
    pub(super) success: bool,
    buffer: Vec<u8>,
    discard: bool,
    pub(super) complete: bool,
    pub(super) usage: BTreeMap<String, u64>,
    pub(super) partial: bool,
}
impl Observer {
    pub(super) fn new(protocol: &str, sse: bool, success: bool) -> Self {
        Self {
            protocol: protocol.into(),
            sse,
            success,
            buffer: Vec::new(),
            discard: false,
            complete: false,
            usage: BTreeMap::new(),
            partial: false,
        }
    }
    pub(super) fn observe(&mut self, bytes: &[u8]) {
        if self.sse {
            for byte in bytes {
                if *byte == b'\n' {
                    if !self.discard {
                        let mut line = std::mem::take(&mut self.buffer);
                        self.line(&line);
                        line.clear();
                        self.buffer = line;
                    }
                    self.buffer.clear();
                    self.discard = false;
                } else if !self.discard {
                    if self.buffer.len() == INSPECTION_BYTES {
                        self.buffer.clear();
                        self.discard = true;
                        self.partial = true;
                    } else {
                        self.buffer.push(*byte);
                    }
                }
            }
        } else if !self.discard {
            if self.buffer.len().saturating_add(bytes.len()) <= INSPECTION_BYTES {
                self.buffer.extend_from_slice(bytes);
            } else {
                self.buffer.clear();
                self.discard = true;
                self.partial = true;
            }
        }
    }
    fn line(&mut self, line: &[u8]) {
        let Some(data) = line.strip_prefix(b"data:") else {
            return;
        };
        let data = data.strip_prefix(b" ").unwrap_or(data);
        if data == b"[DONE]" || data == b"[DONE]\r" {
            if self.protocol == "chat-completions" {
                self.complete = true;
            }
            return;
        }
        if let Ok(value) = serde_json::from_slice::<Value>(data) {
            self.value(&value);
        }
    }
    fn value(&mut self, value: &Value) {
        if value.get("error").is_some() {
            self.success = false;
        }
        match value.get("type").and_then(Value::as_str) {
            Some("response.completed") => {
                self.complete = true;
                if let Some(response) = value.get("response") {
                    self.extract_usage(response.get("usage"));
                }
            }
            Some("message_stop") => self.complete = true,
            Some("response.failed" | "response.incomplete" | "error") => {
                self.success = false;
                self.complete = false;
            }
            _ => {}
        }
        if !self.sse {
            self.complete =
                match self.protocol.as_str() {
                    "chat-completions" => value
                        .get("choices")
                        .and_then(Value::as_array)
                        .is_some_and(|choices| {
                            !choices.is_empty()
                                && choices
                                    .iter()
                                    .all(|c| c.get("finish_reason").is_some_and(|r| !r.is_null()))
                        }),
                    "responses" => value.get("status").and_then(Value::as_str) == Some("completed"),
                    "messages" => value.get("stop_reason").is_some_and(|r| !r.is_null()),
                    _ => false,
                };
        }
        self.extract_usage(value.get("usage"));
        if let Some(message) = value.get("message") {
            self.extract_usage(message.get("usage"));
        }
    }
    fn extract_usage(&mut self, value: Option<&Value>) {
        if let Some(value) = value {
            for name in [
                "input_tokens",
                "output_tokens",
                "prompt_tokens",
                "completion_tokens",
                "total_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
            ] {
                if let Some(count) = value.get(name).and_then(Value::as_u64) {
                    self.usage.insert(name.into(), count);
                }
            }
        }
    }
    pub(super) fn finish_json(&mut self) {
        if !self.sse
            && !self.discard
            && let Ok(value) = serde_json::from_slice::<Value>(&self.buffer)
        {
            self.value(&value);
        }
    }
    pub(super) fn finish(&mut self, record: &mut Record) {
        self.finish_json();
        record.usage = std::mem::take(&mut self.usage);
        record.usage_state = if record.usage.is_empty() {
            "unknown"
        } else if self.partial || !self.complete {
            "partial"
        } else {
            "reported"
        }
        .into();
        if self.complete {
            record.execution = "completion_evidenced".into();
        }
        record.evidence_gap |= self.partial;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::History;

    fn record() -> Record {
        let mut record = History::new("observer-test".into()).begin();
        record.execution = "unknown".into();
        record
    }

    #[test]
    fn usage_observer_preserves_terminal_semantics() {
        let mut observer = Observer::new("responses", true, true);
        observer.observe(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"secret\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":");
        observer.observe(b"3,\"output_tokens\":2,\"unqualified_counter\":99}}}\n\n");
        let mut record = record();
        observer.finish(&mut record);

        assert!(observer.success && observer.complete);
        assert_eq!(
            record.usage,
            BTreeMap::from([("input_tokens".into(), 3), ("output_tokens".into(), 2)])
        );
        assert_eq!(record.usage_state, "reported");
        assert_eq!(record.execution, "completion_evidenced");
        assert!(!record.evidence_gap);
    }

    #[test]
    fn response_observer_is_bounded() {
        let mut observer = Observer::new("responses", true, true);
        observer.observe(&vec![b'a'; INSPECTION_BYTES + 1]);
        assert!(observer.buffer.len() <= INSPECTION_BYTES);
        observer.observe(b"\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"output_tokens\":2}}}\n");
        let mut record = record();
        observer.finish(&mut record);

        assert!(observer.complete);
        assert_eq!(record.usage, BTreeMap::from([("output_tokens".into(), 2)]));
        assert_eq!(record.usage_state, "partial");
        assert_eq!(record.execution, "completion_evidenced");
        assert!(record.evidence_gap);
    }

    #[test]
    fn nonterminal_json_usage_remains_partial_in_history() {
        let mut observer = Observer::new("responses", false, true);
        observer.observe(br#"{"status":"in_progress","usage":{"input_tokens":3}}"#);
        let mut record = record();
        observer.finish(&mut record);

        assert!(observer.success && !observer.complete);
        assert_eq!(record.usage, BTreeMap::from([("input_tokens".into(), 3)]));
        assert_eq!(record.usage_state, "partial");
        assert_eq!(record.execution, "unknown");
        assert!(!record.evidence_gap);
    }
}
