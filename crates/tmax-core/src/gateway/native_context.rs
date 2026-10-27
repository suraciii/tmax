//! Qualified native context mechanisms, not client registrations or permissions.
// Source qualification: installed pi 1.2.0, pi-ai providers/opencode-headers.js,
// openai-completions.js sendSessionAffinityHeaders/openrouter; native
// core/session-manager.js generates UUIDv7 for newSession and preserves it on resume.
// Hermes 0.22.1 agent/opencode_affinity.py preserves a root session across auxiliary work.
use super::request_facts::RequestFacts;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
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
    pub fn can_start_binding(
        &self,
        body: &RequestFacts,
        protocol: &str,
        epoch: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> bool {
        // OpenCode sends complete Chat Completions history and a stable native
        // affinity ID on new, resumed, forked and auxiliary requests. Its first
        // request through this gateway can establish a binding even when the
        // conversation predates tmax. Never adopt upstream-owned references.
        if self.family == "opencode" {
            return protocol == "chat-completions"
                && self.session.is_some()
                && self.independent
                && !has_unbound_resources(body);
        }
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
        body.new_session(protocol)
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
pub fn identify(headers: &HeaderMap, body: &RequestFacts, protocol: &str) -> Option<Context> {
    let ua = headers.get("user-agent")?.to_str().ok()?;
    let family = if ua.starts_with("pi (") && ua.ends_with(')') && ua.contains(';') {
        "pi"
    } else if ua.starts_with("opencode/") {
        "opencode"
    } else if ua == "HermesAgent/0.22.1" {
        "hermes"
    } else if ua == "omp/18.12.6" && protocol == "chat-completions" {
        "omp"
    } else if ua == "deepseek-harness/0.2.1-rc.2 (+https://github.com/deepseek-ai/deepseek-harness)"
    {
        "dsh"
    } else {
        return None;
    };
    let opencode = headers
        .get("x-opencode-session")
        .and_then(|v| v.to_str().ok());
    let affinity = headers.get("x-session-id").and_then(|v| v.to_str().ok());
    // Qualified OMP Chat and DSH requests carry complete independent history.
    // This qualifies stateless context only, not a session-header lifecycle.
    if matches!(family, "omp" | "dsh") && (opencode.is_some() || affinity.is_some()) {
        return None;
    }
    if opencode.zip(affinity).is_some_and(|(a, b)| a != b) {
        return None;
    }
    if family == "opencode" && opencode.is_none() {
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
/// Whether the request names a session at all, whether or not it is usable.
///
/// A request that carries either session header but disagrees with itself, or
/// carries one from an unrecognized client, still named a session: the gateway
/// must reject it rather than silently attribute it. Only a request carrying no
/// session header is unattributed, and only that request may be attributed by
/// the gateway itself.
pub fn names_session(headers: &HeaderMap) -> bool {
    ["x-opencode-session", "x-session-id"]
        .iter()
        .any(|name| headers.get(*name).is_some_and(|v| !v.as_bytes().is_empty()))
}

pub fn has_remote_state(body: &RequestFacts) -> bool {
    body.remote_state
}
pub fn has_unbound_resources(body: &RequestFacts) -> bool {
    body.unbound_resources
}
pub fn independent(protocol: &str, body: &RequestFacts) -> bool {
    body.independent(protocol)
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
        let body = RequestFacts::from_value(
            serde_json::json!({"messages":[{"role":"user","content":"not inspected"}]}),
        )
        .unwrap();
        let context = identify(&headers, &body, "chat-completions").unwrap();
        assert!(context.can_start_binding(
            &body,
            "chat-completions",
            now - chrono::Duration::seconds(1),
            now
        ));
        assert!(!context.can_start_binding(&body, "chat-completions", now, now));
        let resumed = RequestFacts::from_value(serde_json::json!({"messages":[{"role":"user","content":"x"},{"role":"assistant","content":"y"},{"role":"user","content":"z"}]})).unwrap();
        assert!(!context.can_start_binding(
            &resumed,
            "chat-completions",
            now - chrono::Duration::seconds(1),
            now
        ));
    }
    #[test]
    fn dsh_stateless_context_never_becomes_a_session_binding() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "user-agent",
            "deepseek-harness/0.2.1-rc.2 (+https://github.com/deepseek-ai/deepseek-harness)"
                .parse()
                .unwrap(),
        );
        for (protocol, body) in [
            (
                "chat-completions",
                RequestFacts::from_value(
                    serde_json::json!({"messages":[{"role":"user","content":"x"}]}),
                )
                .unwrap(),
            ),
            (
                "messages",
                RequestFacts::from_value(
                    serde_json::json!({"messages":[{"role":"user","content":"x"}]}),
                )
                .unwrap(),
            ),
            (
                "responses",
                RequestFacts::from_value(serde_json::json!({"input":"x","store":false})).unwrap(),
            ),
        ] {
            let context = identify(&headers, &body, protocol).unwrap();
            assert!(context.session.is_none());
            assert!(independent(protocol, &body));
            let mut with_session = headers.clone();
            with_session.insert("x-session-id", "unqualified".parse().unwrap());
            assert!(identify(&with_session, &body, protocol).is_none());
        }
        assert!(!independent(
            "responses",
            &RequestFacts::from_value(json!({"input":"x","store":true})).unwrap()
        ));
        assert!(!independent(
            "responses",
            &RequestFacts::from_value(
                json!({"input":"x","store":false,"previous_response_id":"remote"})
            )
            .unwrap()
        ));
    }
    #[test]
    fn generic_claims_do_not_qualify_stateless() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "generic-client".parse().unwrap());
        assert!(
            identify(
                &headers,
                &RequestFacts::from_value(json!({"messages":[{"role":"user"}]})).unwrap(),
                "chat-completions"
            )
            .is_none()
        );
    }
}
