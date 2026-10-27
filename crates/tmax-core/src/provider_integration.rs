//! Provider integration for upstream model services.
use crate::gateway::RequestFacts;
use crate::{
    Error, Result,
    config::{Config, PROTOCOLS, Provider},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

pub const CONNECT_DEADLINE: Duration = Duration::from_secs(15);
pub const OBSERVATION_DEADLINE: Duration = Duration::from_secs(30);
pub fn integration_id(provider: &Provider) -> Result<&str> {
    if let Some(id) = provider.integration.as_deref() {
        if matches!(
            id,
            "custom" | "deepseek" | "opencode-go" | "codex-subscription" | "kimi-coding-plan"
        ) {
            return Ok(id);
        }
        return Err(
            Error::new("invalid_configuration", "Unknown integration ID").field("integration"),
        );
    }
    match provider.metadata.as_deref() {
        Some("deepseek") => Ok("deepseek"),
        Some("opencode-go") => Ok("opencode-go"),
        Some("openai-codex") => Ok("codex-subscription"),
        Some("kimi-for-coding") | Some("kimi-code") => Ok("kimi-coding-plan"),
        _ => Err(Error::new(
            "invalid_configuration",
            "Explicit supported integration is required for this metadata selection",
        )
        .field("integration")),
    }
}
fn map(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
pub fn regions(id: &str) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
    let entries = match id {
        "deepseek" => vec![(
            "default",
            map(&[
                ("chat-completions", "https://api.deepseek.com"),
                ("responses", "https://api.deepseek.com"),
                ("messages", "https://api.deepseek.com/anthropic"),
            ]),
        )],
        "opencode-go" => vec![(
            "default",
            map(&[
                (
                    "chat-completions",
                    "https://opencode.ai/inference/go/openai/v1",
                ),
                ("responses", "https://opencode.ai/inference/go/openai/v1"),
                ("messages", "https://opencode.ai/inference/go/anthropic"),
            ]),
        )],
        "codex-subscription" => vec![(
            "default",
            map(&[("responses", "https://chatgpt.com/backend-api/codex")]),
        )],
        "kimi-coding-plan" => vec![
            (
                "china",
                map(&[
                    ("chat-completions", "https://api.kimi.com/coding/v1"),
                    ("responses", "https://api.kimi.com/coding/v1"),
                    ("messages", "https://api.kimi.com/coding/"),
                ]),
            ),
            (
                "overseas",
                map(&[
                    ("chat-completions", "https://api.kimi.ai/coding/v1"),
                    ("responses", "https://api.kimi.ai/coding/v1"),
                    ("messages", "https://api.kimi.ai/coding/"),
                ]),
            ),
        ],
        "custom" => vec![],
        _ => return Err(Error::new("invalid_arguments", "Unknown integration ID")),
    };
    Ok(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub fn valid_base(base: &str) -> Result<()> {
    let u = url::Url::parse(base).map_err(|_| {
        Error::new(
            "invalid_configuration",
            "Endpoint must be an absolute permitted URL",
        )
    })?;
    let local = u.host_str().is_some_and(|h| {
        h == "localhost" || h.parse::<std::net::IpAddr>().is_ok_and(|i| i.is_loopback())
    });
    if u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || !(u.scheme() == "https" || (u.scheme() == "http" && local))
    {
        return Err(Error::new(
            "invalid_configuration",
            "Endpoint must use HTTPS or an explicitly local HTTP destination, without credentials, query or fragment",
        ));
    }
    Ok(())
}
pub fn resolve(provider: &Provider) -> Result<BTreeMap<String, String>> {
    let id = integration_id(provider)?;
    for (p, b) in &provider.endpoints {
        if !PROTOCOLS.contains(&p.as_str()) {
            return Err(Error::new(
                "invalid_configuration",
                "Unknown endpoint protocol",
            ));
        }
        valid_base(b)?;
    }
    if id == "custom" {
        if provider.region.is_some() || provider.endpoints.is_empty() {
            return Err(Error::new(
                "invalid_configuration",
                "Custom integration requires explicit protocol bases and forbids a region",
            ));
        }
        return Ok(provider.endpoints.clone());
    }
    let regions = regions(id)?;
    let mut candidates: Vec<_> = regions
        .iter()
        .filter(|(region, bases)| {
            provider.region.as_ref().is_none_or(|r| r == *region)
                && provider
                    .endpoints
                    .iter()
                    .all(|(p, b)| bases.get(p) == Some(b))
        })
        .collect();
    if candidates.len() != 1 {
        return Err(Error::new(
            "invalid_configuration",
            "Select one unambiguous known region or exact published protocol base",
        ));
    }
    Ok(candidates.remove(0).1.clone())
}
pub fn endpoint_view(provider: &Provider) -> Result<Value> {
    let id = integration_id(provider)?;
    let endpoints = resolve(provider)?;
    let region = if id == "custom" {
        None
    } else {
        regions(id)?
            .into_iter()
            .find(|(_, v)| v == &endpoints)
            .map(|(k, _)| k)
    };
    let source = if id == "custom" {
        "explicit_bases"
    } else if provider.region.is_some() && !provider.endpoints.is_empty() {
        "region_and_known_base"
    } else if provider.region.is_some() {
        "region"
    } else if !provider.endpoints.is_empty() {
        "known_base"
    } else {
        "singleton_default"
    };
    Ok(
        json!({"kind":if id=="custom"{"custom"}else{"known"},"region":region,"source":source,"unavailable_protocols":PROTOCOLS.into_iter().filter(|p|!endpoints.contains_key(*p)).collect::<Vec<_>>(),"endpoints":endpoints}),
    )
}
pub fn operation_path(protocol: &str) -> Result<&'static str> {
    match protocol {
        "chat-completions" => Ok("/chat/completions"),
        "responses" => Ok("/responses"),
        "messages" => Ok("/v1/messages"),
        _ => Err(Error::new(
            "invalid_arguments",
            "Unsupported native protocol",
        )),
    }
}
fn operation_queries(protocol: &str) -> &'static [Option<&'static str>] {
    match protocol {
        // Installed Pi 1.2.0 uses @anthropic-ai/sdk 0.129.0's beta Messages
        // creation path. The official SDK sends this selector independently
        // of anthropic-beta feature headers, for streaming and nonstreaming.
        "messages" => &[None, Some("beta=true")],
        _ => &[None],
    }
}
pub fn inference_protocol(method: &str, path: &str, query: Option<&str>) -> Option<&'static str> {
    if method != "POST" {
        return None;
    }
    let protocol = match path {
        "/chat/completions" | "/v1/chat/completions" => "chat-completions",
        "/responses" | "/v1/responses" => "responses",
        "/v1/messages" => "messages",
        _ => return None,
    };
    operation_queries(protocol)
        .contains(&query)
        .then_some(protocol)
}
pub fn protocols(provider: &Provider) -> Result<Vec<String>> {
    Ok(resolve(provider)?.into_keys().collect())
}
pub fn check_request(provider: &Provider, protocol: &str, body: &RequestFacts) -> Result<()> {
    if protocol == "responses" && body.background {
        return Err(Error::new(
            "capability_incompatible",
            "Background execution is not a declared operation for this integration",
        ));
    }
    if !resolve(provider)?.contains_key(protocol) {
        return Err(Error::new(
            "endpoint_unavailable",
            "This provider has no endpoint for the requested native protocol",
        ));
    }
    if integration_id(provider)? == "deepseek" && protocol == "responses" && body.deepseek_state {
        return Err(Error::new(
            "capability_incompatible",
            "Official DeepSeek Responses requires complete independent input history and does not support server-state or background semantics",
        ));
    }
    Ok(())
}
pub fn client(config: &Config) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .connect_timeout(CONNECT_DEADLINE)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("tmax/", env!("CARGO_PKG_VERSION")));
    if let Some(n) = &config.network
        && n.mode == "proxy"
    {
        let raw = n
            .proxy_url
            .as_deref()
            .ok_or_else(|| Error::new("invalid_configuration", "Outbound proxy URL required"))?;
        let mut url = url::Url::parse(raw)
            .map_err(|_| Error::new("invalid_configuration", "Invalid outbound proxy URL"))?;
        if url.scheme() == "socks5" {
            url.set_scheme("socks5h")
                .map_err(|_| Error::new("invalid_configuration", "Invalid SOCKS proxy"))?;
        }
        b = b.proxy(
            reqwest::Proxy::all(url.as_str())
                .map_err(|_| Error::new("invalid_configuration", "Invalid outbound proxy URL"))?,
        );
    }
    b.build().map_err(|_| {
        Error::new(
            "operation_failed",
            "Cannot initialize the configured outbound transport",
        )
    })
}
pub fn registry() -> Value {
    json!(
        [
            "custom",
            "deepseek",
            "opencode-go",
            "codex-subscription",
            "kimi-coding-plan"
        ]
        .into_iter()
        .map(|id| inspect(id).expect("built-in integration"))
        .collect::<Vec<_>>()
    )
}
pub fn inspect(id: &str) -> Result<Value> {
    let r = regions(id)?;
    let setup = if id == "custom" {
        "explicit_bases"
    } else if r.len() == 1 {
        "singleton_default"
    } else {
        "region_or_known_base"
    };
    let protocols: Vec<_> = if id == "custom" {
        PROTOCOLS.iter().map(|p| p.to_string()).collect()
    } else {
        r.values()
            .flat_map(|r| r.keys().cloned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    let mut operations:Vec<_>=protocols.iter().map(|p|json!({"protocol":p,"method":"POST","path_template":operation_path(p).expect("registry protocol"),"query_variants":operation_queries(p),"transport":"http","streaming":"sse","requires_original_binding":false})).collect();
    if id == "codex-subscription" {
        operations.push(json!({"protocol":"responses","method":"GET","path_template":"/models","transport":"http","requires_original_binding":false,"purpose":"native_model_discovery"}));
    }
    if id == "deepseek" {
        operations.push(json!({"protocol":null,"method":"GET","path_template":"/models","url":DEEPSEEK_MODELS_DESTINATION.url,"transport":"http","requires_original_binding":false,"purpose":"model_discovery","source":DEEPSEEK_MODELS_DESTINATION.source,"source_contract":DEEPSEEK_MODELS_DESTINATION.contract}));
    }
    if id == "opencode-go" {
        operations.push(json!({"protocol":null,"method":"GET","path_template":"/zen/go/v1/usage","url":GO_USAGE_DESTINATION.url,"transport":"http","requires_original_binding":false,"purpose":"quota","source":GO_USAGE_DESTINATION.source,"source_contract":GO_USAGE_DESTINATION.contract}));
    }
    let auth = if id == "codex-subscription" {
        vec!["login"]
    } else if id == "kimi-coding-plan" {
        vec!["api-key", "login"]
    } else {
        vec!["api-key"]
    };
    Ok(
        json!({"id":id,"integration":id,"protocols":protocols,"endpoint_setup":setup,"authentication_methods":auth,"endpoint_support":{"kind":if id=="custom"{"custom"}else{"known"},"setup":setup,"regions":r.into_iter().map(|(id,endpoints)|json!({"id":id,"endpoints":endpoints})).collect::<Vec<_>>(),"operations":operations},"login_sources":crate::agent_integration::login_source_agents(id),"provider_input_capabilities":[],"public_reset_input":if id=="codex-subscription"{json!({"source":"codex-resets","kind":"reset","scope":"codex","endpoint":"https://codex-resets.com/index.md"})}else{Value::Null},"support":{"documentation":"documented","runtime_verification":"unknown","version_range":null},"restrictions":match id{"codex-subscription"=>json!(["Native owner retains tokens and refresh","Gateway requires authorized managed policy and established native identity"]),"opencode-go"=>json!(["Truthful coding-client and stable native conversation identity on every inference request","Quota GET provides consumed-only percentages and reset announcements, with unqualified freshness and no priority-rule inputs; renewal remains unknown (Console or explicit manual information)" ]),"kimi-coding-plan"=>json!(["Personal interactive coding only; no identity spoofing","Regional alternatives require explicit selection","Native omp OAuth cannot be copied to other consumers"]),_=>json!([])}}),
    )
}
pub async fn refresh(
    store: &crate::storage::Store,
    config: &Config,
    provider_id: &str,
    scope: &str,
    dry_run: bool,
) -> Result<Value> {
    crate::observations::refresh(store, config, provider_id, scope, dry_run).await
}

/// The integration owns non-inference destinations; never derive these from a model base.
#[derive(Clone, Copy, Debug)]
pub struct ObservationDestination<'a> {
    pub source: &'static str,
    pub url: &'a str,
    pub contract: &'static str,
}

const GO_USAGE_DESTINATION: ObservationDestination<'static> = ObservationDestination {
    source: "go_usage_api",
    url: "https://opencode.ai/zen/go/v1/usage",
    contract: "opencode_go_usage_2026_10_23",
};

const DEEPSEEK_MODELS_DESTINATION: ObservationDestination<'static> = ObservationDestination {
    source: "official_models_api",
    url: "https://api.deepseek.com/models",
    contract: "deepseek_models_v1",
};

pub fn observation_destination(
    provider: &Provider,
    scope: &str,
) -> Result<Option<ObservationDestination<'static>>> {
    resolve(provider)?;
    Ok(match (integration_id(provider)?, scope) {
        ("deepseek", "auth" | "quota") => Some(ObservationDestination {
            source: "official_balance_api",
            url: "https://api.deepseek.com/user/balance",
            contract: "deepseek_balance_v1",
        }),
        ("deepseek", "models") => Some(DEEPSEEK_MODELS_DESTINATION),
        ("opencode-go", "auth" | "models") => Some(ObservationDestination {
            source: "official_models_api",
            url: "https://opencode.ai/inference/go/v1/models",
            contract: "opencode_go_models_v1",
        }),
        ("opencode-go", "quota") => Some(GO_USAGE_DESTINATION),
        _ => None,
    })
}

pub(crate) struct AcquiredObservation {
    pub data: Value,
    pub status: u16,
}

pub(crate) async fn acquire_observation(
    store: &crate::storage::Store,
    config: &Config,
    provider: &Provider,
    destination: ObservationDestination<'_>,
) -> Result<AcquiredObservation> {
    let observation = async {
        let Some(crate::config::Auth::ApiKey { key_ref }) = &provider.auth else {
            return Err(Error::new(
                "credential_unavailable",
                "Selected API key is unavailable",
            ));
        };
        let key = crate::secrets::get(store, key_ref)?;
        let key = std::str::from_utf8(&key).map_err(|_| {
            Error::new(
                "credential_unavailable",
                "Selected key encoding is unusable",
            )
        })?;
        let response = client(config)?
            .get(destination.url)
            .bearer_auth(key)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|_| Error::new("transport_error", "Provider observation transport failed"))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(Error::new(
                "upstream_rejection",
                "Provider observation was rejected",
            ));
        }
        let limit = if destination.source == GO_USAGE_DESTINATION.source {
            64 * 1024
        } else {
            4 * 1024 * 1024
        };
        let bytes = read_bounded(response, limit).await?;
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| {
            Error::new(
                "source_format_error",
                "Provider observation representation is unsupported",
            )
        })?;
        let data = normalize_observation(destination.source, &body)?;
        Ok(AcquiredObservation { data, status })
    };
    tokio::time::timeout(OBSERVATION_DEADLINE, observation)
        .await
        .map_err(|_| {
            Error::new(
                "observation_deadline",
                "Provider observation deadline elapsed",
            )
        })?
}

pub(crate) fn normalize_observation(source: &str, body: &Value) -> Result<Value> {
    if source == "official_balance_api" {
        return normalize_balance(body);
    }
    if source == GO_USAGE_DESTINATION.source {
        return normalize_go_usage(body);
    }
    if source != "official_models_api" {
        return Err(Error::new(
            "source_format_error",
            "Unknown provider observation source",
        ));
    }
    let models = body.get("data").and_then(Value::as_array).ok_or_else(|| {
        Error::new(
            "source_format_error",
            "Model discovery representation is unsupported",
        )
    })?;
    if models.len() > 10000 {
        return Err(Error::new(
            "source_format_error",
            "Model discovery exceeds its bounded record capacity",
        ));
    }
    let mut ids = Vec::with_capacity(models.len());
    for model in models {
        let id = model
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
            .ok_or_else(|| {
                Error::new(
                    "source_format_error",
                    "Discovered model identity is invalid",
                )
            })?;
        ids.push(id);
    }
    Ok(json!({"models":ids,"registered_models":[],"registration":"not_performed"}))
}
async fn read_bounded(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::new("transport_error", "Provider observation body interrupted"))?
    {
        if body.len() + chunk.len() > limit {
            return Err(Error::new(
                "source_format_error",
                "Provider observation exceeds its finite size limit",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
fn normalize_go_usage(body: &Value) -> Result<Value> {
    let invalid = || {
        Error::new(
            "source_format_error",
            "Go usage window representation is unsupported",
        )
    };
    let usage = body
        .get("usage")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let mut windows = serde_json::Map::new();
    for name in ["rolling", "weekly", "monthly"] {
        let window = usage
            .get(name)
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        // Only this current status is qualified; historical enums are not the
        // current wire contract. Percentages are consumed, floored integers,
        // not exact remaining allowance, and no published maximum is assumed.
        let status = window
            .get("status")
            .and_then(Value::as_str)
            .filter(|s| *s == "ok")
            .ok_or_else(invalid)?;
        let percent = window
            .get("percent")
            .and_then(Value::as_u64)
            .ok_or_else(invalid)?;
        let reset = window
            .get("resetsAt")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 64 && s.contains('T') && s.ends_with('Z'))
            .ok_or_else(invalid)?;
        crate::inputs::ExactInstant::parse(reset).map_err(|_| invalid())?;
        windows.insert(
            name.into(),
            json!({"status":status,"percent":percent,"resetsAt":reset}),
        );
    }
    Ok(
        json!({"usage":windows,"percent_basis":"consumed","percent_precision":"floored","reset_basis":"announcement"}),
    )
}
fn normalize_balance(body: &Value) -> Result<Value> {
    let available = body
        .get("is_available")
        .and_then(Value::as_bool)
        .ok_or_else(|| Error::new("observation_failed", "Balance availability field missing"))?;
    let list = body
        .get("balance_infos")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("observation_failed", "Balance entries missing"))?;
    let mut result = Vec::new();
    for item in list {
        let currency = item
            .get("currency")
            .and_then(Value::as_str)
            .filter(|v| v.len() == 3 && v.bytes().all(|c| c.is_ascii_uppercase()))
            .ok_or_else(|| Error::new("observation_failed", "Balance currency unsupported"))?;
        let mut row = json!({"currency":currency});
        for field in ["total_balance", "granted_balance", "topped_up_balance"] {
            let v = item
                .get(field)
                .and_then(Value::as_str)
                .filter(|v| crate::config::decimal(v.strip_prefix('-').unwrap_or(v)))
                .ok_or_else(|| {
                    Error::new("observation_failed", "Balance decimal field unsupported")
                })?;
            row[field] = json!(v);
        }
        result.push(row);
    }
    Ok(json!({"is_available":available,"balance_infos":result}))
}
pub fn dashboard(id: &str) -> &'static str {
    match id {
        "deepseek" => "https://platform.deepseek.com/",
        "opencode-go" => "https://opencode.ai/console",
        "codex-subscription" => "https://chatgpt.com/codex/settings/usage",
        "kimi-coding-plan" => "https://www.kimi.com/code/console",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inference_operations_allow_only_declared_messages_beta_selector() {
        for (path, protocol) in [
            ("/chat/completions", "chat-completions"),
            ("/v1/chat/completions", "chat-completions"),
            ("/responses", "responses"),
            ("/v1/responses", "responses"),
            ("/v1/messages", "messages"),
        ] {
            assert_eq!(inference_protocol("POST", path, None), Some(protocol));
        }
        assert_eq!(
            inference_protocol("POST", "/v1/messages", Some("beta=true")),
            Some("messages")
        );
        for query in [
            "",
            "beta",
            "beta=false",
            "beta=True",
            "beta=true&beta=true",
            "beta=true&beta=false",
            "beta=true&unknown=1",
            "unknown=1",
            "beta=%74rue",
            "b%65ta=true",
        ] {
            assert!(inference_protocol("POST", "/v1/messages", Some(query)).is_none());
        }
        for (method, path, query) in [
            ("GET", "/v1/messages", Some("beta=true")),
            ("POST", "/v1/messages/count_tokens", Some("beta=true")),
            ("POST", "/v1/messages/batches", Some("beta=true")),
            ("POST", "/v1/responses", Some("beta=true")),
            ("POST", "/v1/chat/completions", Some("beta=true")),
            ("GET", "/v1/responses/id", None),
            ("POST", "/v1/responses/id/cancel", None),
        ] {
            assert!(inference_protocol(method, path, query).is_none());
        }
        let declared = inspect("custom").unwrap();
        let messages = declared["endpoint_support"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|operation| operation["protocol"] == "messages")
            .unwrap();
        assert_eq!(messages["query_variants"], json!([null, "beta=true"]));
    }

    #[test]
    fn map_and_custom_bases() {
        let p = Provider {
            metadata: Some("deepseek".into()),
            ..Provider::default()
        };
        assert_eq!(
            resolve(&p).unwrap()["messages"],
            "https://api.deepseek.com/anthropic"
        );
        let mut p = Provider {
            integration: Some("custom".into()),
            ..Provider::default()
        };
        p.endpoints
            .insert("chat-completions".into(), "http://127.0.0.1:9000/v1".into());
        assert!(!resolve(&p).unwrap().contains_key("responses"));
    }
    #[test]
    fn regional_ambiguity_and_no_product_fallback() {
        let mut p = Provider {
            integration: Some("kimi-coding-plan".into()),
            ..Provider::default()
        };
        assert!(resolve(&p).is_err());
        p.region = Some("china".into());
        assert_eq!(
            resolve(&p).unwrap()["messages"],
            "https://api.kimi.com/coding/"
        );
        p.endpoints
            .insert("messages".into(), "https://api.kimi.ai/coding/".into());
        assert!(resolve(&p).is_err());
    }
    fn go_usage_body() -> Value {
        json!({"usage":{
            "rolling":{"status":"ok","percent":0,"resetsAt":"2026-10-23T19:04:44.687Z"},
            "weekly":{"status":"ok","percent":0,"resetsAt":"2026-10-26T00:00:00.000Z"},
            "monthly":{"status":"ok","percent":62,"resetsAt":"2026-11-09T03:13:27.000Z"}
        }})
    }
    #[test]
    fn go_quota_declares_exact_separate_read_without_rule_inputs() {
        let provider = Provider {
            integration: Some("opencode-go".into()),
            ..Default::default()
        };
        let quota = observation_destination(&provider, "quota")
            .unwrap()
            .unwrap();
        assert_eq!(quota.url, "https://opencode.ai/zen/go/v1/usage");
        assert_ne!(
            quota.url,
            observation_destination(&provider, "models")
                .unwrap()
                .unwrap()
                .url
        );
        let declaration = inspect("opencode-go").unwrap();
        let operation = declaration["endpoint_support"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["purpose"] == "quota")
            .unwrap();
        assert_eq!(operation["method"], "GET");
        assert_eq!(operation["url"], quota.url);
        assert_eq!(operation["path_template"], "/zen/go/v1/usage");
        assert_eq!(declaration["provider_input_capabilities"], json!([]));
    }
    #[test]
    fn go_usage_retains_only_qualified_consumed_windows_and_exact_reset_announcements() {
        let mut body = go_usage_body();
        body["workspace"] = json!("must-not-retain");
        body["usage"]["monthly"]["remaining"] = json!(38);
        body["usage"]["other"] = json!({"identity":"must-not-retain"});
        let normalized = normalize_observation("go_usage_api", &body).unwrap();
        assert_eq!(normalized["usage"], go_usage_body()["usage"]);
        assert_eq!(normalized["percent_basis"], "consumed");
        assert_eq!(normalized["percent_precision"], "floored");
        assert_eq!(normalized["reset_basis"], "announcement");
        assert_eq!(normalized.as_object().unwrap().len(), 4);
        assert_eq!(
            normalize_observation("go_usage_api", &normalized).unwrap(),
            normalized
        );
        // There is no published current maximum: do not clamp an overage.
        body["usage"]["monthly"]["percent"] = json!(101);
        assert_eq!(
            normalize_observation("go_usage_api", &body).unwrap()["usage"]["monthly"]["percent"],
            101
        );
    }
    #[test]
    fn malformed_or_unqualified_go_window_never_becomes_zero_or_partial_success() {
        let valid = go_usage_body();
        let mut cases = vec![
            Value::Null,
            json!({}),
            json!({"usage":[]}),
            json!({"rollingUsage":{"status":"ok","usagePercent":0,"resetInSec":3600}}),
        ];
        for name in ["rolling", "weekly", "monthly"] {
            for field in ["status", "percent", "resetsAt"] {
                let mut body = valid.clone();
                body["usage"][name].as_object_mut().unwrap().remove(field);
                cases.push(body);
            }
            for percent in [json!(-1), json!(0.5), json!("0"), json!(null), json!(true)] {
                let mut body = valid.clone();
                body["usage"][name]["percent"] = percent;
                cases.push(body);
            }
            for status in ["rate-limited", "unknown", "", "ok\n"] {
                let mut body = valid.clone();
                body["usage"][name]["status"] = json!(status);
                cases.push(body);
            }
            for reset in [
                "2026-02-30T00:00:00Z",
                "2026-10-23T00:00:00+00:00",
                "2026-10-23",
                "2026-10-23T00:00:00Z\n",
            ] {
                let mut body = valid.clone();
                body["usage"][name]["resetsAt"] = json!(reset);
                cases.push(body);
            }
            let mut body = valid.clone();
            body["usage"][name]["resetsAt"] =
                json!(format!("2026-10-23T00:00:00.{}Z", "0".repeat(80)));
            cases.push(body);
        }
        for body in cases {
            assert_eq!(
                normalize_observation("go_usage_api", &body)
                    .unwrap_err()
                    .code,
                "source_format_error"
            );
        }
    }
    #[tokio::test]
    async fn go_usage_transport_sends_truthful_read_headers_and_bounds_untrusted_responses() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::at(dir.path().into());
        let owner = crate::secrets::owner_tag(&store, "provider", "go-test").unwrap();
        let key_ref = crate::secrets::put(&store, &owner, b"synthetic-test-key").unwrap();
        let provider = Provider {
            integration: Some("opencode-go".into()),
            auth: Some(crate::config::Auth::ApiKey { key_ref }),
            ..Default::default()
        };
        let mut oversized = go_usage_body();
        oversized["ignored"] = json!("x".repeat(64 * 1024));
        for (status, body, expected) in [
            (200, go_usage_body().to_string(), None),
            (200, oversized.to_string(), Some("source_format_error")),
            (
                403,
                "private provider error must not escape".into(),
                Some("upstream_rejection"),
            ),
            (302, String::new(), Some("upstream_rejection")),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0 && request.len() + count <= 8192);
                    request.extend_from_slice(&chunk[..count]);
                }
                let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
                assert!(request.starts_with("get /zen/go/v1/usage http/1.1\r\n"));
                assert!(request.contains(concat!(
                    "\r\nuser-agent: tmax/",
                    env!("CARGO_PKG_VERSION"),
                    "\r\n"
                )));
                assert!(request.contains("\r\naccept: application/json\r\n"));
                assert!(request.contains("\r\nauthorization: bearer synthetic-test-key\r\n"));
                for header in ["x-opencode-", "x-session-id:", "x-tmax-", "session_id:"] {
                    assert!(!request.contains(header));
                }
                let response = format!(
                    "HTTP/1.1 {status} fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nLocation: /must-not-follow\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                // An oversized-response reader may close as soon as its bound
                // is reached; the fixture need not finish writing those bytes.
                let _ = socket.write_all(response.as_bytes()).await;
            });
            let url = format!("http://{address}/zen/go/v1/usage");
            let destination = ObservationDestination {
                url: &url,
                ..GO_USAGE_DESTINATION
            };
            let result =
                acquire_observation(&store, &Config::empty(), &provider, destination).await;
            tokio::time::timeout(Duration::from_secs(5), server)
                .await
                .unwrap()
                .unwrap();
            match expected {
                Some(code) => assert_eq!(result.err().unwrap().code, code),
                None => {
                    let acquired = result.unwrap();
                    assert_eq!(acquired.status, 200);
                    assert_eq!(acquired.data["usage"], go_usage_body()["usage"]);
                }
            }
        }
    }
    #[test]
    fn model_discovery_preserves_exact_ids_and_rejects_invalid_or_excessive_records() {
        let source = DEEPSEEK_MODELS_DESTINATION.source;
        let exact = "模型/Exact-ID ";
        let longest = "m".repeat(512);
        let normalized = normalize_observation(
            source,
            &json!({"data":[{"id":exact,"owned_by":"not-an-account-identity","protocol":"responses"},{"id":longest}]}),
        )
        .unwrap();
        assert_eq!(
            normalized,
            json!({"models":[exact,longest],"registered_models":[],"registration":"not_performed"})
        );
        assert_eq!(
            normalize_observation(source, &json!({"data":[]})).unwrap()["models"],
            json!([])
        );
        let mut records = vec![json!({"id":"model"}); 10000];
        assert_eq!(
            normalize_observation(source, &json!({"data":records})).unwrap()["models"]
                .as_array()
                .unwrap()
                .len(),
            10000
        );
        records.push(json!({"id":"one-too-many"}));
        let mut malformed = vec![
            Value::Null,
            json!({}),
            json!({"data":{}}),
            json!({"data":records}),
        ];
        for id in [
            json!(""),
            json!("m".repeat(513)),
            json!("模型".repeat(171)),
            json!("model\n"),
            json!("model\u{007f}"),
            json!(null),
            json!(42),
        ] {
            // One valid record must not turn a malformed list into partial success.
            malformed.push(json!({"data":[{"id":"valid"},{"id":id}]}));
        }
        malformed.push(json!({"data":[{"id":"valid"},{}]}));
        for body in malformed {
            assert_eq!(
                normalize_observation(source, &body).unwrap_err().code,
                "source_format_error"
            );
        }
    }

    #[tokio::test]
    async fn deepseek_models_transport_authenticates_and_bounds_untrusted_discovery() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::at(dir.path().into());
        let owner = crate::secrets::owner_tag(&store, "provider", "deepseek-test").unwrap();
        let key_ref = crate::secrets::put(&store, &owner, b"synthetic-deepseek-key").unwrap();
        let provider = Provider {
            integration: Some("deepseek".into()),
            auth: Some(crate::config::Auth::ApiKey { key_ref }),
            ..Default::default()
        };
        let destination = observation_destination(&provider, "models")
            .unwrap()
            .unwrap();
        assert_eq!(destination.url, "https://api.deepseek.com/models");
        assert_eq!(destination.contract, "deepseek_models_v1");
        let valid = json!({"data":[{"id":"deepseek-flash","owned_by":"must-not-retain"}]});
        let mut oversized = valid.clone();
        oversized["ignored"] = json!("x".repeat(4 * 1024 * 1024));
        for (status, body, expected) in [
            (200, valid.to_string(), None),
            (200, oversized.to_string(), Some("source_format_error")),
            (200, "not JSON".into(), Some("source_format_error")),
            (
                200,
                json!({"data":[{"id":""}]}).to_string(),
                Some("source_format_error"),
            ),
            (401, "private rejection".into(), Some("upstream_rejection")),
            (429, "private limit".into(), Some("upstream_rejection")),
            (302, String::new(), Some("upstream_rejection")),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0 && request.len() + count <= 8192);
                    request.extend_from_slice(&chunk[..count]);
                }
                let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
                assert!(request.starts_with("get /models http/1.1\r\n"));
                assert!(request.contains("\r\nauthorization: bearer synthetic-deepseek-key\r\n"));
                assert!(request.contains("\r\naccept: application/json\r\n"));
                let response = format!(
                    "HTTP/1.1 {status} fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nLocation: /must-not-follow\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
            let url = format!("http://{address}/models");
            let result = acquire_observation(
                &store,
                &Config::empty(),
                &provider,
                ObservationDestination {
                    url: &url,
                    ..destination
                },
            )
            .await;
            tokio::time::timeout(Duration::from_secs(5), server)
                .await
                .unwrap()
                .unwrap();
            match expected {
                Some(code) => {
                    let error = result.err().unwrap();
                    assert_eq!(error.code, code);
                    assert!(!error.to_string().contains("private"));
                }
                None => {
                    let acquired = result.unwrap();
                    assert_eq!(acquired.status, 200);
                    assert_eq!(
                        acquired.data,
                        json!({"models":["deepseek-flash"],"registered_models":[],"registration":"not_performed"})
                    );
                }
            }
        }
    }
}
