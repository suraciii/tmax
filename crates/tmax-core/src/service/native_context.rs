//! Qualified native context mechanisms, not client registrations or permissions.
// Source qualification: installed pi 1.2.0, pi-ai providers/opencode-headers.js,
// openai-completions.js sendSessionAffinityHeaders/openrouter; native
// core/session-manager.js generates UUIDv7 for newSession and preserves it on resume.
// Hermes 0.22.1 agent/opencode_affinity.py preserves a root session across auxiliary work.
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub struct Context {
    pub family: &'static str,
    pub session: Option<String>,
    pub go_session_header: Option<&'static str>,
    pub created_at: Option<DateTime<Utc>>,
    pub independent: bool,
}
impl Context {
    pub fn scope_handle(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        let mut hash = Sha256::new();
        for component in [self.family, session] {
            hash.update((component.len() as u64).to_be_bytes());
            hash.update(component.as_bytes());
        }
        Some(format!("native-scope:{:x}", hash.finalize()))
    }
    pub fn binding_key(&self, public_id: &str, protocol: &str) -> Option<String> {
        Some(binding_key(&self.scope_handle()?, public_id, protocol))
    }
    pub fn trusted_new(
        &self,
        body: &Value,
        protocol: &str,
        epoch: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> bool {
        // UUIDv7's native creation instant prevents a lost binding store from
        // reclassifying an old identifier as new. No clock/TTL releases a binding.
        if self.family != "pi"
            || !self
                .created_at
                .is_some_and(|created| created >= epoch && created <= now)
            || !self.independent
        {
            return false;
        }
        let messages = match protocol {
            "chat-completions" | "messages" => body.get("messages"),
            "responses" => body.get("input"),
            _ => None,
        };
        match messages {
            Some(Value::String(_)) => true,
            Some(Value::Array(messages)) => {
                let mut users = 0;
                for message in messages {
                    match message.get("role").and_then(Value::as_str) {
                        Some("system" | "developer") => {}
                        Some("user") => users += 1,
                        _ => return false,
                    }
                }
                users == 1
            }
            _ => false,
        }
    }
}
pub fn binding_key(scope_handle: &str, public_id: &str, protocol: &str) -> String {
    let mut hash = Sha256::new();
    for component in [scope_handle, public_id, protocol] {
        hash.update((component.len() as u64).to_be_bytes());
        hash.update(component.as_bytes());
    }
    format!("native:{:x}", hash.finalize())
}
pub fn identify(headers: &HeaderMap, body: &Value, protocol: &str) -> Option<Context> {
    let ua = headers.get("user-agent")?.to_str().ok()?;
    let family = if ua.starts_with("pi (") && ua.ends_with(')') && ua.contains(';') {
        "pi"
    } else if ua == "HermesAgent/0.22.1" {
        "hermes"
    } else {
        return None;
    };
    let opencode = headers
        .get("x-opencode-session")
        .and_then(|v| v.to_str().ok());
    let affinity = headers.get("x-session-id").and_then(|v| v.to_str().ok());
    if opencode.zip(affinity).is_some_and(|(a, b)| a != b) {
        return None;
    }
    let session = opencode
        .or(affinity)
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .map(str::to_owned);
    // Only the installed Pi Chat Completions compatibility path qualifies
    // x-session-id for translation to Go's recommended header. Other native
    // paths still need their qualified x-opencode-session mechanism.
    let go_session_header = session.as_ref().and_then(|_| {
        if opencode.is_some() {
            Some("x-opencode-session")
        } else if family == "pi" && protocol == "chat-completions" {
            Some("x-session-id")
        } else {
            None
        }
    });
    let created_at = session
        .as_ref()
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .filter(|id| id.get_version_num() == 7)
        .and_then(|id| {
            let b = id.as_bytes();
            let mut millis = 0i64;
            for byte in &b[..6] {
                millis = (millis << 8) | i64::from(*byte);
            }
            DateTime::from_timestamp_millis(millis)
        });
    Some(Context {
        family,
        session,
        go_session_header,
        created_at,
        independent: independent(protocol, body),
    })
}
pub fn has_remote_state(body: &Value) -> bool {
    body.get("previous_response_id")
        .is_some_and(|v| !v.is_null())
        || body.get("conversation").is_some_and(|v| !v.is_null())
        || body.get("background").and_then(Value::as_bool) == Some(true)
        || body
            .get("input")
            .and_then(Value::as_array)
            .is_some_and(|input| {
                input
                    .iter()
                    .any(|item| item.get("type").and_then(Value::as_str) == Some("item_reference"))
            })
}
pub fn has_unbound_resources(body: &Value) -> bool {
    if body
        .pointer("/prompt/id")
        .is_some_and(|value| !value.is_null())
        || body
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| {
                tools.iter().any(|tool| {
                    tool.get("type").and_then(Value::as_str) == Some("file_search")
                        && tool
                            .get("vector_store_ids")
                            .and_then(Value::as_array)
                            .is_some_and(|ids| !ids.is_empty())
                })
            })
    {
        return true;
    }
    let unbound = |part: &Value| {
        matches!(
            part.get("type").and_then(Value::as_str),
            Some("input_file" | "file" | "document")
        ) && [
            part.get("file_id"),
            part.pointer("/file/file_id"),
            part.pointer("/source/file_id"),
        ]
        .into_iter()
        .flatten()
        .any(|value| !value.is_null())
    };
    for item in body
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            body.get("input")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        )
    {
        if unbound(item)
            || item
                .get("content")
                .and_then(Value::as_array)
                .is_some_and(|content| content.iter().any(unbound))
        {
            return true;
        }
    }
    false
}
pub fn independent(protocol: &str, body: &Value) -> bool {
    if has_remote_state(body) {
        return false;
    }
    match protocol {
        "chat-completions" | "messages" => body
            .get("messages")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty()),
        "responses" => {
            body.get("store").and_then(Value::as_bool) == Some(false)
                && body
                    .get("input")
                    .is_some_and(|v| v.is_array() || v.is_string())
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn native(timestamp: i64) -> String {
        let mut bytes = [0u8; 16];
        for (index, byte) in timestamp.to_be_bytes()[2..].iter().enumerate() {
            bytes[index] = *byte;
        }
        bytes[6] = 0x70;
        bytes[8] = 0x80;
        uuid::Uuid::from_bytes(bytes).to_string()
    }
    #[test]
    fn loss_epoch_blocks_existing_native_identifiers() {
        let now = Utc::now();
        let id = native(now.timestamp_millis() - 100);
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "pi (linux 7; x64)".parse().unwrap());
        headers.insert("x-session-id", id.parse().unwrap());
        let body = json!({"messages":[{"role":"user","content":"not inspected"}]});
        let context = identify(&headers, &body, "chat-completions").unwrap();
        assert!(context.trusted_new(
            &body,
            "chat-completions",
            now - chrono::Duration::seconds(1),
            now
        ));
        assert!(!context.trusted_new(&body, "chat-completions", now, now));
        let resumed = json!({"messages":[{"role":"user","content":"x"},{"role":"assistant","content":"y"},{"role":"user","content":"z"}]});
        assert!(!context.trusted_new(
            &resumed,
            "chat-completions",
            now - chrono::Duration::seconds(1),
            now
        ));
    }
    #[test]
    fn generic_claims_do_not_qualify_stateless() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "generic-client".parse().unwrap());
        assert!(
            identify(
                &headers,
                &json!({"messages":[{"role":"user"}]}),
                "chat-completions"
            )
            .is_none()
        );
    }
}
