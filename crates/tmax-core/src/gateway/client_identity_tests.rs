use super::native_context::independent as independent_context;
use super::*;
use crate::gateway::test_support::{GO_URL, fixture, native_headers};
use serde_json::json;

#[test]
fn stateless_context_is_protocol_qualified() {
    assert!(independent_context(
        "chat-completions",
        &json!({"messages":[{"role":"user","content":"x"}]})
    ));
    assert!(!independent_context(
        "responses",
        &json!({"input":"x","previous_response_id":"id","store":false})
    ));
    assert!(!independent_context("responses", &json!({"input":"x"})));
    assert!(independent_context(
        "responses",
        &json!({"input":[],"store":false})
    ));
}

/// The shape OpenCode mints. Go's free tier rejects a malformed id outright.
fn is_session_shape(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("ses_") else {
        return false;
    };
    if rest.len() != 26 {
        return false;
    }
    let (clock, random) = rest.split_at(12);
    clock.bytes().all(|b| b.is_ascii_hexdigit())
        && random.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn bare_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "omp/18.12.6".parse().unwrap());
    headers
}

#[test]
fn a_client_without_a_session_gets_one_in_opencodes_own_shape() {
    let headers = bare_headers();
    let forwarded = upstream_headers(&headers, None, GO_URL);
    let session = forwarded["x-opencode-session"].to_str().unwrap();
    assert!(
        is_session_shape(session),
        "minted id {session} is not OpenCode's shape"
    );
    // Both spellings carry the same id, as the real client sends.
    assert_eq!(forwarded["x-opencode-session-id"], session);
    // The client's truthful identity is never replaced.
    assert_eq!(forwarded["user-agent"], "omp/18.12.6");
}

/// The real client sends a per-message id in `x-opencode-request`, so it must
/// not become the conversation seed, or every turn would get its own session.
#[test]
fn one_conversation_keeps_one_session_id_across_turns() {
    let mut first = bare_headers();
    first.insert("x-opencode-request", "msg_turn_1".parse().unwrap());
    let mut second = bare_headers();
    second.insert("x-opencode-request", "msg_turn_2".parse().unwrap());
    let minted =
        |headers: &HeaderMap| upstream_headers(headers, None, GO_URL)["x-opencode-session"].clone();
    assert_eq!(minted(&first), minted(&second));
    assert_eq!(minted(&first), minted(&first));
}

#[test]
fn a_client_supplied_session_is_forwarded_unchanged() {
    let mut headers = bare_headers();
    headers.insert(
        "x-opencode-session",
        "ses_qualified_native_conversation".parse().unwrap(),
    );
    let forwarded = upstream_headers(&headers, Some("x-opencode-session"), GO_URL);
    assert_eq!(
        forwarded["x-opencode-session"],
        "ses_qualified_native_conversation"
    );
}

#[test]
fn a_non_opencode_destination_gets_no_minted_session() {
    for url in [
        "https://api.kimi.com/coding/v1",
        "https://opencode.example.com/v1",
        "not-a-url",
    ] {
        let forwarded = upstream_headers(&bare_headers(), None, url);
        assert!(
            forwarded.get("x-opencode-session").is_none(),
            "{url} must not receive an OpenCode session"
        );
    }
}

#[test]
fn the_gateway_is_recognized_whatever_the_configured_name() {
    for url in [
        "https://opencode.ai/inference/go/openai/v1",
        "https://opencode.ai:8443/zen/go/v1",
        "https://api.opencode.ai/inference/go",
        "https://opencode.ai./inference/go",
    ] {
        assert!(opencode_gateway_url(url), "{url}");
    }
}

/// Go no longer rejects a client that supplied no session header, because the
/// gateway attributes that request itself. A client that did name a session,
/// even an unusable one, is still rejected: attribution must not paper over a
/// client that tried to name a session and got it wrong.
#[test]
fn an_unattributed_client_reaches_go_through_the_gateway_attribution() {
    let (_temporary, _runtime, config) = fixture();
    let mut go = config.clone();
    let provider = go.providers.get_mut("p").unwrap();
    provider.integration = Some("opencode-go".into());
    provider.endpoints.clear();
    let body = json!({"messages":[{"role":"user","content":"x"}]});
    let unattributed = HeaderMap::new();
    let mut unknown = HeaderMap::new();
    unknown.insert("user-agent", "generic-client".parse().unwrap());
    let mut conflicting = native_headers(Utc::now());
    conflicting.insert("x-opencode-session", "a-different-session".parse().unwrap());
    for headers in [&unattributed, &unknown, &conflicting] {
        let context = native_context::identify(headers, &body, "chat-completions");
        let native = NativeRequirements::capture(headers, context.as_ref());
        assert!(
            !native.go_session,
            "no qualified session header is present in this request"
        );
        assert_eq!(
            native_mismatch(provider, &native),
            if native_context::names_session(headers) {
                Some("native_session_required")
            } else {
                None
            },
            "headers {headers:?}"
        );
    }
}
