//! Bounded response inspection for completion and allowlisted usage evidence.
use super::INSPECTION_BYTES;
use crate::diagnostics::{Record, error_log::DETAIL_BYTES};
use chrono::{DateTime, Utc};
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
    http_error: bool,
    event_generic_error: bool,
    event_error: bool,
    error_body: Vec<u8>,
    error: Option<String>,
    error_time: Option<DateTime<Utc>>,
    incomplete: bool,
    error_truncated: bool,
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
            http_error: !success,
            event_error: false,
            event_generic_error: false,
            error_body: Vec::new(),
            error: None,
            error_time: (!success).then(Utc::now),
            incomplete: false,
            error_truncated: false,
        }
    }
    pub(super) fn observe(&mut self, bytes: &[u8]) {
        if self.http_error {
            let keep = bytes.len().min(DETAIL_BYTES - self.error_body.len());
            self.error_body.extend_from_slice(&bytes[..keep]);
            self.error_truncated |= keep < bytes.len();
        }
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
                        if self.event_generic_error
                            && self.error.is_none()
                            && let Some(data) = self.buffer.strip_prefix(b"data:")
                        {
                            let data = data.strip_prefix(b" ").unwrap_or(data);
                            let keep = data.len().min(DETAIL_BYTES);
                            let mut text = String::from_utf8_lossy(&data[..keep]).into_owned();
                            if text.len() > DETAIL_BYTES {
                                let mut end = DETAIL_BYTES;
                                while !text.is_char_boundary(end) {
                                    end -= 1;
                                }
                                text.truncate(end);
                            }
                            self.error = Some(text);
                            self.error_truncated = true;
                        }
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
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            self.event_error = false;
            self.event_generic_error = false;
        }
        if let Some(event) = line.strip_prefix(b"event:") {
            self.event_generic_error = event.trim_ascii() == b"error";
            self.event_error = matches!(
                event.trim_ascii(),
                b"error" | b"response.failed" | b"response.incomplete"
            );
            if self.event_error {
                self.error_time.get_or_insert_with(Utc::now);
                self.success = false;
                self.complete = false;
            }
            return;
        }
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
            if self.event_error {
                self.capture_error(&value);
            }
        } else if self.event_error && self.error.is_none() {
            self.error = Some(String::from_utf8_lossy(data).into_owned());
        }
    }
    fn value(&mut self, value: &Value) {
        let failed = value.get("error").is_some_and(|error| !error.is_null())
            || matches!(
                value.get("type").and_then(Value::as_str),
                Some("response.failed" | "response.incomplete" | "error")
            )
            || matches!(
                value.get("status").and_then(Value::as_str),
                Some("failed" | "incomplete")
            );
        if failed {
            self.error_time.get_or_insert_with(Utc::now);
            self.success = false;
            self.capture_error(value);
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
    fn capture_error(&mut self, value: &Value) {
        if self.http_error || self.error.is_some() {
            return;
        }
        let generic_error =
            self.event_generic_error || value.get("type").and_then(Value::as_str) == Some("error");
        self.error = Some(if generic_error {
            value.to_string()
        } else {
            let detail = value
                .get("error")
                .filter(|error| !error.is_null())
                .or_else(|| {
                    value
                        .pointer("/response/error")
                        .filter(|error| !error.is_null())
                })
                .or_else(|| value.get("incomplete_details"))
                .or_else(|| value.pointer("/response/incomplete_details"));
            match detail {
                Some(detail) => detail.to_string(),
                None => serde_json::json!({"type":value.get("type"),"status":value.get("status")})
                    .to_string(),
            }
        });
    }
    pub(super) fn mark_incomplete(&mut self) {
        self.incomplete = true;
    }
    pub(super) fn take_error(&mut self) -> Option<(DateTime<Utc>, String, bool)> {
        let error = if self.http_error {
            let bytes = std::mem::take(&mut self.error_body);
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            self.error.take()?
        };
        Some((
            self.error_time.take()?,
            error,
            self.error_truncated || self.incomplete,
        ))
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
        if self.sse && !self.discard && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.line(&line);
        }
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
        record.evidence_gap |= self.partial || self.incomplete;
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
    fn review_regression_oversized_sse_error_keeps_available_prefix() {
        let mut observer = Observer::new("responses", true, true);
        observer.observe(b"event: error\ndata: ");
        observer.observe(&vec![b'x'; INSPECTION_BYTES + 1]);
        observer.observe(b"\n\n");
        let mut record = record();
        observer.finish(&mut record);
        let (_, text, truncated) = observer
            .take_error()
            .expect("bounded error prefix retained");
        assert!(!observer.success);
        assert!(truncated && record.evidence_gap);
        assert!(text.starts_with("xxxxxxxx"));
        assert!(text.len() <= DETAIL_BYTES);
    }

    #[test]
    fn review_regression_generic_error_preserves_vendor_event_fields() {
        let mut observer = Observer::new("responses", true, true);
        observer.observe(b"data: {\"type\":\"error\",\"code\":\"vendor_rejection\",\"error\":{\"message\":\"Bad tool arguments\"}}\n\n");
        let (_, text, truncated) = observer.take_error().unwrap();
        let details: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(details["code"], "vendor_rejection");
        assert_eq!(details["error"]["message"], "Bad tool arguments");
        assert!(!truncated);
    }

    #[test]
    fn named_generic_error_keeps_event_but_response_failure_omits_output() {
        let mut generic = Observer::new("responses", true, true);
        generic.observe(b"event: error\ndata: {\"code\":\"vendor_rejection\",\"error\":{\"message\":\"bad\"}}\n\n");
        let (_, text, truncated) = generic.take_error().unwrap();
        let details: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(details["code"], "vendor_rejection");
        assert_eq!(details["error"]["message"], "bad");
        assert!(!truncated);

        let mut failed = Observer::new("responses", true, true);
        failed.observe(b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"bad\"},\"output\":[\"private output\"]}}\n\n");
        let (_, text, _) = failed.take_error().unwrap();
        let details: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(details["message"], "bad");
        assert!(!text.contains("private output"));
    }

    #[test]
    fn error_time_is_the_first_observation_not_finish_time() {
        let mut http = Observer::new("responses", false, false);
        let http_time = http.error_time.unwrap();
        http.observe(br#"{"error":"bad"}"#);
        http.finish(&mut record());
        assert_eq!(http.take_error().unwrap().0, http_time);

        let mut named = Observer::new("responses", true, true);
        named.observe(b"event: error\n");
        let named_time = named.error_time.unwrap();
        named.observe(b"data: {\"type\":\"error\",\"error\":\"bad\"}\n\n");
        named.finish(&mut record());
        assert_eq!(named.take_error().unwrap().0, named_time);

        let mut generic = Observer::new("responses", true, true);
        generic.observe(b"data: {\"type\":\"error\",\"error\":\"bad\"}\n");
        let generic_time = generic.error_time.unwrap();
        generic.finish(&mut record());
        assert_eq!(generic.take_error().unwrap().0, generic_time);
    }

    #[test]
    fn incomplete_capture_marks_valid_json_without_inventing_failure() {
        let mut http = Observer::new("responses", false, false);
        http.observe(br#"{"error":"bad"}"#);
        http.mark_incomplete();
        let mut history = record();
        http.finish(&mut history);
        let (_, text, truncated) = http.take_error().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap()["error"],
            "bad"
        );
        assert!(truncated && history.evidence_gap);

        let mut event = Observer::new("responses", true, true);
        event.observe(b"data: {\"type\":\"error\",\"error\":\"bad\"}");
        event.mark_incomplete();
        let mut history = record();
        event.finish(&mut history);
        assert!(event.take_error().unwrap().2 && history.evidence_gap);

        let mut success = Observer::new("responses", true, true);
        success.observe(b"data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3}}}\n");
        success.mark_incomplete();
        let mut history = record();
        success.finish(&mut history);
        assert!(success.success && success.complete && history.evidence_gap);
        assert_eq!(history.usage_state, "reported");
        assert_eq!(history.execution, "completion_evidenced");
        assert!(success.take_error().is_none());
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
        assert!(observer.take_error().is_none());
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
