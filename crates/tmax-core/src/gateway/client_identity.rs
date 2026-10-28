use crate::{Error, Result, config::Auth};
use axum::http::{HeaderMap, HeaderValue};
use chrono::Utc;
use std::sync::{LazyLock, Mutex};

use super::{errors::safe_message, native_context, opencode_session};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NativeClient {
    Unknown,
    Pi,
    Hermes,
    OpenCode,
}
pub struct NativeRequirements {
    pub client: NativeClient,
    pub go_session: bool,
    /// Whether the client named a session at all, qualified or not. A client
    /// that named one, even unusably, must not be attributed: the gateway never
    /// overrides a session identity a request tried to supply.
    pub unattributed: bool,
}
impl NativeRequirements {
    pub(super) fn capture(headers: &HeaderMap, context: Option<&native_context::Context>) -> Self {
        Self {
            client: match context.map(|c| c.family) {
                Some("pi") => NativeClient::Pi,
                Some("hermes") => NativeClient::Hermes,
                Some("opencode") => NativeClient::OpenCode,
                _ => NativeClient::Unknown,
            },
            go_session: context.is_some_and(|c| c.go_session_header.is_some()),
            // Attribution is only for a client that supplied no session header
            // at all. A client whose headers are present but unusable — an
            // unrecognized agent, or two headers disagreeing — is a rejected
            // request, not an unattributed one.
            unattributed: !native_context::names_session(headers),
        }
    }
    /// Whether the gateway can attribute `provider`'s requests itself. Only a
    /// Go destination is eligible: the minted id is OpenCode's shape and is
    /// meaningless to any other upstream.
    fn gateway_attribution(&self, provider: &crate::config::Provider) -> bool {
        self.unattributed
            && crate::provider_integration::resolve(provider)
                .is_ok_and(|bases| bases.values().any(|b| opencode_gateway_url(b)))
    }
}
pub fn native_mismatch(
    provider: &crate::config::Provider,
    native: &NativeRequirements,
) -> Option<&'static str> {
    match crate::provider_integration::integration_id(provider) {
        Ok("codex-subscription") => Some("gateway_identity_prerequisite"),
        Ok("kimi-coding-plan") if matches!(provider.auth, Some(Auth::Login { .. })) => {
            Some("native_authorization_unqualified")
        }
        // Go answers a request naming no session with HTTP 400. A client that
        // supplied no qualified session header would otherwise be unable to
        // reach Go at all, so the gateway's own per-conversation attribution
        // covers it. The user agent is still left truthful, and a client that
        // did supply a session keeps exclusive ownership of that value.
        Ok("opencode-go") if !native.go_session && !native.gateway_attribution(provider) => {
            Some("native_session_required")
        }
        Ok("kimi-coding-plan") if native.client == NativeClient::Unknown => {
            Some("native_client_required")
        }
        Ok(_) => None,
        Err(_) => Some("integration_unavailable"),
    }
}
pub(super) fn client_contract(
    provider: &crate::config::Provider,
    native: &NativeRequirements,
) -> Result<()> {
    if let Some(reason) = native_mismatch(provider, native) {
        Err(Error::new(reason, safe_message(reason)))
    } else {
        Ok(())
    }
}
pub(super) fn hop_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
/// A bounded map of conversation seed to minted OpenCode session id, so a
/// conversation's turns keep one id. Bounded by the module's own limit.
fn session_seeds() -> &'static Mutex<opencode_session::Sessions> {
    static SEEDS: LazyLock<Mutex<opencode_session::Sessions>> =
        LazyLock::new(|| Mutex::new(opencode_session::Sessions::default()));
    &SEEDS
}

/// Whether `url` names OpenCode's own gateway, whatever port or path it gives.
/// Judged by the destination, not by the provider's configured name, so a
/// renamed or re-based entry is still recognized.
fn opencode_gateway_url(url: &str) -> bool {
    let authority = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    let host = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority)
        .split(':')
        .next()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    host == "opencode.ai" || host.ends_with(".opencode.ai")
}

pub(super) fn upstream_headers(
    headers: &HeaderMap,
    go_session_header: Option<&str>,
    upstream_url: &str,
) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in headers {
        if !hop_header(name.as_str())
            && !matches!(
                name.as_str(),
                "host"
                    | "authorization"
                    | "x-api-key"
                    | "x-gateway-key"
                    | "content-length"
                    | "origin"
                    | "referer"
                    | "cookie"
                    | "sec-fetch-site"
                    | "sec-fetch-mode"
                    | "sec-fetch-dest"
            )
        {
            out.append(name.clone(), value.clone());
        }
    }
    if let Some(session) = go_session_header.and_then(|name| headers.get(name)) {
        // This is the actual qualified native session, not a generated or
        // provider-wide identity. Preserve the native header and truthful UA.
        out.insert("x-opencode-session", session.clone());
    }
    // Go answers a request naming no session with HTTP 400. When the client
    // supplied no session header at all, tmax attributes the request itself: an
    // id in OpenCode's own shape, kept stable per conversation. This is routing
    // attribution, not an invented client identity, and the truthful user agent
    // is left untouched.
    //
    // Any value the client did send is authoritative and is never replaced,
    // whatever its shape: a qualified native session is not OpenCode's format,
    // and its exact value is what Go is meant to route by.
    let client_supplied = out.contains_key("x-opencode-session");
    if !client_supplied && opencode_gateway_url(upstream_url) {
        let seed = conversation_seed(headers);
        let id = session_seeds()
            .lock()
            .map(|mut sessions| sessions.session_id(&seed, Utc::now().into()))
            .unwrap_or_else(|_| opencode_session::new_session_id(Utc::now().into()));
        if let Ok(value) = HeaderValue::from_str(&id) {
            out.insert("x-opencode-session", value.clone());
            out.insert("x-opencode-session-id", value);
        }
    }
    out
}

/// What identifies one conversation, so its turns keep one minted id.
///
/// Only headers that name a conversation belong here. `x-opencode-request` is
/// deliberately excluded: the real client sends it as the per-message id, so
/// seeding on it would give every turn its own session and defeat the purpose.
/// Message content is never read.
fn conversation_seed(headers: &HeaderMap) -> String {
    for name in ["x-opencode-session", "x-session-id"] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok())
            && !value.is_empty()
        {
            return format!("header:{name}={value}");
        }
    }
    "anonymous".to_owned()
}

#[cfg(test)]
#[path = "client_identity_tests.rs"]
mod tests;
