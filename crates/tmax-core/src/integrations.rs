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
pub fn protocols(provider: &Provider, model_id: &str) -> Result<Vec<String>> {
    let mut p: Vec<_> = resolve(provider)?.into_keys().collect();
    if integration_id(provider)? == "opencode-go" && model_id == "deepseek-v4.1-flash" {
        p.retain(|p| p == "chat-completions");
    }
    Ok(p)
}
pub fn check_request(
    provider: &Provider,
    model_id: &str,
    protocol: &str,
    body: &Value,
) -> Result<()> {
    if protocol == "responses" && body.get("background").and_then(Value::as_bool) == Some(true) {
        return Err(Error::new(
            "capability_incompatible",
            "Background execution is not a declared operation for this integration",
        ));
    }
    if !protocols(provider, model_id)?.iter().any(|p| p == protocol) {
        return Err(Error::new(
            "model_unavailable",
            "This exact model and native protocol combination is not qualified",
        ));
    }
    if integration_id(provider)? == "deepseek"
        && protocol == "responses"
        && [
            "previous_response_id",
            "conversation",
            "store",
            "background",
        ]
        .iter()
        .any(|k| {
            body.get(*k)
                .is_some_and(|v| !v.is_null() && v != &json!(false))
        })
    {
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
    let mut operations:Vec<_>=protocols.iter().map(|p|json!({"protocol":p,"method":"POST","path_template":operation_path(p).expect("registry protocol"),"transport":"http","streaming":"sse","requires_original_binding":false})).collect();
    if id == "codex-subscription" {
        operations.push(json!({"protocol":"responses","method":"GET","path_template":"/models","transport":"http","requires_original_binding":false,"purpose":"native_model_discovery"}));
    }
    let auth = if id == "codex-subscription" {
        vec!["login"]
    } else if id == "kimi-coding-plan" {
        vec!["api-key", "login"]
    } else {
        vec!["api-key"]
    };
    Ok(
        json!({"id":id,"integration":id,"protocols":protocols,"endpoint_setup":setup,"authentication_methods":auth,"endpoint_support":{"kind":if id=="custom"{"custom"}else{"known"},"setup":setup,"regions":r.into_iter().map(|(id,endpoints)|json!({"id":id,"endpoints":endpoints})).collect::<Vec<_>>(),"operations":operations},"login_sources":crate::native::login_source_agents(id),"provider_input_capabilities":[],"public_reset_input":if id=="codex-subscription"{json!({"source":"codex-resets","kind":"reset","scope":"codex","endpoint":"https://codex-resets.com/index.md"})}else{Value::Null},"support":{"documentation":"documented","runtime_verification":"unknown","version_range":null},"restrictions":match id{"codex-subscription"=>json!(["Native owner retains tokens and refresh","Gateway requires authorized managed policy and established native identity"]),"opencode-go"=>json!(["Truthful coding-client and stable native conversation identity on every inference request","deepseek-v4.1-flash Responses remains unqualified","Quota and renewal require Console or explicit manual information"]),"kimi-coding-plan"=>json!(["Personal interactive coding only; no identity spoofing","Regional alternatives require explicit selection","Native omp OAuth cannot be copied to other consumers"]),_=>json!([])}}),
    )
}
pub async fn refresh(
    store: &crate::storage::Store,
    config: &Config,
    provider_id: &str,
    scope: &str,
    unlock: Option<&[u8]>,
    dry_run: bool,
) -> Result<Value> {
    crate::observations::refresh(store, config, provider_id, scope, unlock, dry_run).await
}

/// The integration owns non-inference destinations; never derive these from a model base.
#[derive(Clone, Copy, Debug)]
pub struct ObservationDestination {
    pub source: &'static str,
    pub url: &'static str,
    pub contract: &'static str,
}

pub fn observation_destination(
    provider: &Provider,
    scope: &str,
) -> Result<Option<ObservationDestination>> {
    resolve(provider)?;
    Ok(match (integration_id(provider)?, scope) {
        ("deepseek", "auth" | "quota") => Some(ObservationDestination {
            source: "official_balance_api",
            url: "https://api.deepseek.com/user/balance",
            contract: "deepseek_balance_v1",
        }),
        ("opencode-go", "auth" | "models") => Some(ObservationDestination {
            source: "official_models_api",
            url: "https://opencode.ai/inference/go/v1/models",
            contract: "opencode_go_models_v1",
        }),
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
    destination: ObservationDestination,
    unlock: Option<&[u8]>,
) -> Result<AcquiredObservation> {
    let observation = async {
        let Some(crate::config::Auth::ApiKey { key_ref }) = &provider.auth else {
            return Err(Error::new(
                "credential_unavailable",
                "Selected API key is unavailable",
            ));
        };
        let key = crate::secrets::get(store, key_ref, unlock)?;
        let key = std::str::from_utf8(&key).map_err(|_| {
            Error::new(
                "credential_unavailable",
                "Selected key encoding is unusable",
            )
        })?;
        let response = client(config)?
            .get(destination.url)
            .bearer_auth(key)
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
        let bytes = read_bounded(response, 4 * 1024 * 1024).await?;
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
}
