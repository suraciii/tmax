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
            "custom"
                | "deepseek"
                | "opencode-go"
                | "codex-subscription"
                | "kimi-coding-plan"
                | "glm-coding-plan"
                | "stepfun-token-plan"
                | "minimax-m-plan"
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
        Some("stepfun-step-plan") => Ok("stepfun-token-plan"),
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
        "glm-coding-plan" => vec![(
            "default",
            map(&[
                (
                    "chat-completions",
                    "https://open.bigmodel.cn/api/coding/paas/v4",
                ),
                ("responses", "https://open.bigmodel.cn/api/v1"),
                ("messages", "https://open.bigmodel.cn/api/anthropic"),
            ]),
        )],
        "stepfun-token-plan" => vec![(
            "default",
            map(&[
                ("chat-completions", "https://api.stepfun.com/step_plan/v1"),
                ("messages", "https://api.stepfun.com/step_plan"),
            ]),
        )],
        "minimax-m-plan" => vec![
            (
                "global",
                map(&[("messages", "https://api.minimax.io/anthropic")]),
            ),
            (
                "china",
                map(&[("messages", "https://api.minimaxi.com/anthropic")]),
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
            "kimi-coding-plan",
            "glm-coding-plan",
            "stepfun-token-plan",
            "minimax-m-plan"
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
    if id == "kimi-coding-plan" {
        for (region, url) in [
            ("china", KIMI_USAGE_DESTINATION.url),
            ("overseas", "https://api.kimi.ai/coding/v1/usages"),
        ] {
            operations.push(json!({"region":region,"protocol":null,"method":"GET","path_template":"/usages","url":url,"transport":"http","requires_original_binding":false,"purpose":"quota","source":KIMI_USAGE_DESTINATION.source,"source_contract":KIMI_USAGE_DESTINATION.contract,"authorization":"bearer"}));
        }
    }
    if id == "glm-coding-plan" {
        operations.push(json!({"region":"default","protocol":null,"method":"GET","path_template":"/api/monitor/usage/quota/limit","url":GLM_MONITOR_DESTINATION.url,"transport":"http","requires_original_binding":false,"purpose":"quota","source":GLM_MONITOR_DESTINATION.source,"source_contract":GLM_MONITOR_DESTINATION.contract,"authorization":"raw_key"}));
    }
    if id == "stepfun-token-plan" {
        operations.push(json!({"region":"default","protocol":null,"method":"GET","path_template":"/step_plan/v1/usage","url":STEPFUN_USAGE_DESTINATION.url,"transport":"http","requires_original_binding":false,"purpose":"quota","source":STEPFUN_USAGE_DESTINATION.source,"source_contract":STEPFUN_USAGE_DESTINATION.contract,"authorization":"bearer"}));
    }
    if id == "minimax-m-plan" {
        for (region, url) in [
            ("global", MINIMAX_REMAINS_DESTINATION.url),
            ("china", "https://api.minimaxi.com/v1/token_plan/remains"),
        ] {
            operations.push(json!({"region":region,"protocol":null,"method":"GET","path_template":"/v1/token_plan/remains","url":url,"transport":"http","requires_original_binding":false,"purpose":"quota","source":MINIMAX_REMAINS_DESTINATION.source,"source_contract":MINIMAX_REMAINS_DESTINATION.contract,"authorization":"bearer"}));
        }
    }
    let auth = if id == "codex-subscription" {
        vec!["login"]
    } else if id == "kimi-coding-plan" {
        vec!["api-key", "login"]
    } else {
        vec!["api-key"]
    };
    Ok(
        json!({"id":id,"integration":id,"protocols":protocols,"endpoint_setup":setup,"authentication_methods":auth,"endpoint_support":{"kind":if id=="custom"{"custom"}else{"known"},"setup":setup,"regions":r.into_iter().map(|(id,endpoints)|json!({"id":id,"endpoints":endpoints})).collect::<Vec<_>>(),"operations":operations},"login_sources":crate::agent_integration::login_source_agents(id),"provider_input_capabilities":[],"public_reset_input":if id=="codex-subscription"{json!({"source":"codex-resets","kind":"reset","scope":"codex","endpoint":"https://codex-resets.com/index.md"})}else{Value::Null},"support":{"documentation":"documented","runtime_verification":"unknown","version_range":null},"restrictions":match id{"codex-subscription"=>json!(["Native owner retains tokens and refresh","Gateway requires authorized managed policy and established native identity"]),"opencode-go"=>json!(["Truthful coding-client and stable native conversation identity on every inference request","Quota GET provides consumed-only percentages and reset announcements, with unqualified freshness and no priority-rule inputs; renewal remains unknown (Console or explicit manual information)"]),"kimi-coding-plan"=>json!(["Personal interactive coding only; no identity spoofing","Regional alternatives require explicit selection","Native omp OAuth cannot be copied to other consumers"]),"glm-coding-plan"=>json!(["Domestic plan scope only; international z.ai bases are not interchangeable","Monitor quota rows keep raw labels with unqualified unit and reset encodings","No monitor field becomes a rule input, renewal fact, or per-model allowance copy"]),"stepfun-token-plan"=>json!(["Plan-route billing only; standard /v1 API fallback is prohibited","Usage read is runtime-qualified and undocumented; parsing fails closed on schema drift","step-5-preview is the only qualified model scope"]),"minimax-m-plan"=>json!(["Quota display is percent-only; counter units are unpublished","Status enums and zero totals never mean unlimited or no active plan","Regional hosts require explicit selection and are not interchangeable"]),_=>json!([])}}),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthStyle {
    Bearer,
    RawKey,
}
#[derive(Clone, Copy, Debug)]
pub struct ObservationDestination<'a> {
    pub source: &'static str,
    pub url: &'a str,
    pub auth: AuthStyle,
    pub contract: &'static str,
}

const GO_USAGE_DESTINATION: ObservationDestination<'static> = ObservationDestination {
    source: "go_usage_api",
    url: "https://opencode.ai/zen/go/v1/usage",
    contract: "opencode_go_usage_2026_10_23",
    auth: AuthStyle::Bearer,
};

const DEEPSEEK_MODELS_DESTINATION: ObservationDestination<'static> = ObservationDestination {
    source: "official_models_api",
    url: "https://api.deepseek.com/models",
    contract: "deepseek_models_v1",
    auth: AuthStyle::Bearer,
};
const KIMI_USAGE_DESTINATION: ObservationDestination<'static> = ObservationDestination {
    source: "kimi_usage_api",
    url: "https://api.kimi.com/coding/v1/usages",
    contract: "kimi_coding_usage_2026_10_28",
    auth: AuthStyle::Bearer,
};
const GLM_MONITOR_DESTINATION: ObservationDestination<'static> = ObservationDestination {
    source: "glm_monitor_api",
    url: "https://open.bigmodel.cn/api/monitor/usage/quota/limit",
    contract: "glm_monitor_quota_2026_10_28",
    auth: AuthStyle::RawKey,
};
const STEPFUN_USAGE_DESTINATION: ObservationDestination<'static> = ObservationDestination {
    source: "stepfun_usage_api",
    url: "https://api.stepfun.com/step_plan/v1/usage",
    contract: "stepfun_usage_v1_2026_10_28",
    auth: AuthStyle::Bearer,
};
const MINIMAX_REMAINS_DESTINATION: ObservationDestination<'static> = ObservationDestination {
    source: "minimax_plan_remains_api",
    url: "https://api.minimax.io/v1/token_plan/remains",
    contract: "minimax_token_plan_remains_2026_10_28",
    auth: AuthStyle::Bearer,
};

pub fn observation_destination(
    provider: &Provider,
    scope: &str,
) -> Result<Option<ObservationDestination<'static>>> {
    let bases = resolve(provider)?;
    Ok(match (integration_id(provider)?, scope) {
        ("deepseek", "auth" | "quota") => Some(ObservationDestination {
            source: "official_balance_api",
            url: "https://api.deepseek.com/user/balance",
            contract: "deepseek_balance_v1",
            auth: AuthStyle::Bearer,
        }),
        ("deepseek", "models") => Some(DEEPSEEK_MODELS_DESTINATION),
        ("opencode-go", "auth" | "models") => Some(ObservationDestination {
            source: "official_models_api",
            url: "https://opencode.ai/inference/go/v1/models",
            contract: "opencode_go_models_v1",
            auth: AuthStyle::Bearer,
        }),
        ("opencode-go", "quota") => Some(GO_USAGE_DESTINATION),
        ("kimi-coding-plan", "quota") => {
            let overseas = bases
                .get("chat-completions")
                .is_some_and(|base| base == "https://api.kimi.ai/coding/v1");
            Some(if overseas {
                ObservationDestination {
                    url: "https://api.kimi.ai/coding/v1/usages",
                    ..KIMI_USAGE_DESTINATION
                }
            } else {
                KIMI_USAGE_DESTINATION
            })
        }
        ("glm-coding-plan", "quota") => Some(GLM_MONITOR_DESTINATION),
        ("stepfun-token-plan", "quota") => Some(STEPFUN_USAGE_DESTINATION),
        ("minimax-m-plan", "quota") => {
            let china = bases
                .get("messages")
                .is_some_and(|base| base == "https://api.minimaxi.com/anthropic");
            Some(if china {
                ObservationDestination {
                    url: "https://api.minimaxi.com/v1/token_plan/remains",
                    ..MINIMAX_REMAINS_DESTINATION
                }
            } else {
                MINIMAX_REMAINS_DESTINATION
            })
        }
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
        let request = client(config)?.get(destination.url);
        let request = match destination.auth {
            AuthStyle::Bearer => request.bearer_auth(key),
            AuthStyle::RawKey => request.header(
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(key).map_err(|_| {
                    Error::new(
                        "credential_unavailable",
                        "Selected key encoding is unusable",
                    )
                })?,
            ),
        };
        let response = request
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
        let limit = if [
            GO_USAGE_DESTINATION.source,
            KIMI_USAGE_DESTINATION.source,
            GLM_MONITOR_DESTINATION.source,
            STEPFUN_USAGE_DESTINATION.source,
            MINIMAX_REMAINS_DESTINATION.source,
        ]
        .contains(&destination.source)
        {
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
    if source == KIMI_USAGE_DESTINATION.source {
        return normalize_kimi_usage(body);
    }
    if source == GLM_MONITOR_DESTINATION.source {
        return normalize_glm_monitor(body);
    }
    if source == STEPFUN_USAGE_DESTINATION.source {
        return normalize_stepfun_usage(body);
    }
    if source == MINIMAX_REMAINS_DESTINATION.source {
        return normalize_minimax_remains(body);
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

fn plain_str(value: &Value, max: usize) -> Option<&str> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control))
}
fn decimal_str<'a>(object: &'a serde_json::Map<String, Value>, field: &str) -> Option<&'a str> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|v| !v.starts_with('-') && crate::config::decimal(v))
}
fn reset_str<'a>(object: &'a serde_json::Map<String, Value>, field: &str) -> Option<&'a str> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| s.len() <= 64 && s.contains('T') && s.ends_with('Z'))
        .filter(|s| crate::inputs::ExactInstant::parse(s).is_ok())
}
fn normalize_kimi_usage(body: &Value) -> Result<Value> {
    let invalid = || {
        Error::new(
            "source_format_error",
            "Kimi usage representation is unsupported",
        )
    };
    let usage = body
        .get("usage")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let weekly = json!({
        "limit": decimal_str(usage, "limit").ok_or_else(invalid)?,
        "used": decimal_str(usage, "used").ok_or_else(invalid)?,
        "remaining": decimal_str(usage, "remaining").ok_or_else(invalid)?,
        "resetTime": reset_str(usage, "resetTime").ok_or_else(invalid)?,
    });
    let limits = body
        .get("limits")
        .and_then(Value::as_array)
        .filter(|rows| rows.len() <= 8)
        .ok_or_else(invalid)?;
    let mut windows = Vec::with_capacity(limits.len());
    let mut seen = std::collections::BTreeSet::new();
    for row in limits {
        let row = row.as_object().ok_or_else(invalid)?;
        let window = row
            .get("window")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let duration = window
            .get("duration")
            .and_then(Value::as_u64)
            .ok_or_else(invalid)?;
        let time_unit =
            plain_str(window.get("timeUnit").ok_or_else(invalid)?, 64).ok_or_else(invalid)?;
        if !seen.insert(duration) {
            return Err(invalid());
        }
        let detail = row
            .get("detail")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        // The live payload omits `used` from window details; retained fields
        // keep their decimal-string form, and a row without limit, remaining,
        // and resetTime is not a qualified window.
        let mut detail_out = serde_json::Map::new();
        for field in ["limit", "used", "remaining"] {
            if detail.get(field).is_some() {
                detail_out.insert(
                    field.into(),
                    json!(decimal_str(detail, field).ok_or_else(invalid)?),
                );
            }
        }
        detail_out.insert(
            "resetTime".into(),
            json!(reset_str(detail, "resetTime").ok_or_else(invalid)?),
        );
        if !detail_out.contains_key("limit") || !detail_out.contains_key("remaining") {
            return Err(invalid());
        }
        windows.push(json!({
            "window": {"duration": duration, "timeUnit": time_unit},
            "detail": Value::Object(detail_out),
        }));
    }
    Ok(json!({"usage":weekly,"limits":windows,
        "counter_unit":"unqualified","reset_basis":"provider_timestamp"}))
}
fn normalize_glm_monitor(body: &Value) -> Result<Value> {
    let invalid = || {
        Error::new(
            "source_format_error",
            "GLM monitor quota representation is unsupported",
        )
    };
    // HTTP 200 carries business errors; semantic success is the only gate.
    if body.get("success").and_then(Value::as_bool) != Some(true)
        || body.get("code").and_then(Value::as_i64) != Some(200)
    {
        return Err(Error::new(
            "upstream_rejection",
            "GLM monitor quota request was rejected",
        ));
    }
    let data = body
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let level = plain_str(data.get("level").ok_or_else(invalid)?, 64).ok_or_else(invalid)?;
    let limits = data
        .get("limits")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= 16)
        .ok_or_else(invalid)?;
    let mut rows = Vec::with_capacity(limits.len());
    let mut seen = std::collections::BTreeSet::new();
    for row in limits {
        let row = row.as_object().ok_or_else(invalid)?;
        let kind = row
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if !matches!(kind, "TOKENS_LIMIT" | "TIME_LIMIT" | "CREDIT_LIMIT") {
            return Err(invalid());
        }
        let unit = row
            .get("unit")
            .and_then(Value::as_u64)
            .ok_or_else(invalid)?;
        let number = row
            .get("number")
            .and_then(Value::as_u64)
            .ok_or_else(invalid)?;
        if !seen.insert((kind, unit, number)) {
            return Err(invalid());
        }
        let mut out = serde_json::Map::new();
        out.insert("type".into(), json!(kind));
        out.insert("unit".into(), json!(unit));
        out.insert("number".into(), json!(number));
        if let Some(percent) = row.get("percentage") {
            out.insert(
                "percentage".into(),
                json!(percent.as_u64().ok_or_else(invalid)?),
            );
        }
        let mut counters = 0;
        for field in ["usage", "currentValue", "remaining"] {
            if let Some(value) = row.get(field) {
                out.insert(field.into(), json!(value.as_u64().ok_or_else(invalid)?));
                counters += 1;
            }
        }
        // A row must carry the provider percentage or the complete counter
        // triple; a partial triple never becomes a partial fact.
        if counters != 0 && counters != 3 {
            return Err(invalid());
        }
        if !out.contains_key("percentage") && counters == 0 {
            return Err(invalid());
        }
        if let Some(reset) = row.get("nextResetTime") {
            out.insert(
                "nextResetTime".into(),
                json!(reset.as_u64().ok_or_else(invalid)?),
            );
        }
        rows.push(Value::Object(out));
    }
    Ok(
        json!({"success":true,"code":200,"data":{"level":level,"limits":rows},
        "percent_basis":"consumed","percent_precision":"integer_rounded",
        "counter_unit":"unqualified","reset_encoding":"epoch_milliseconds_unqualified"}),
    )
}
fn normalize_stepfun_usage(body: &Value) -> Result<Value> {
    let invalid = || {
        Error::new(
            "source_format_error",
            "StepFun plan usage representation is unsupported",
        )
    };
    // The payload declares its own contract version; an unexpected version
    // fails closed instead of parsing optimistically.
    if body.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err(invalid());
    }
    let scope = plain_str(body.get("scope").ok_or_else(invalid)?, 64).ok_or_else(invalid)?;
    let observed_at_ms = body
        .get("observed_at_ms")
        .and_then(Value::as_u64)
        .ok_or_else(invalid)?;
    let plan = body
        .get("plan")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    let quota = body
        .get("quota")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    if quota.get("state").and_then(Value::as_str) != Some("known")
        || quota.get("allowed").and_then(Value::as_bool) != Some(true)
        || quota
            .get("reason")
            .is_some_and(|reason| reason != &Value::Null)
    {
        return Err(Error::new(
            "observation_failed",
            "Upstream quota state is not known",
        ));
    }
    // Only the empty no-rolling-window condition is qualified; a populated
    // windows array has unqualified semantics and must not pass silently.
    let windows = quota
        .get("windows")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if !windows.is_empty() {
        return Err(invalid());
    }
    let buckets = quota
        .get("credit_buckets")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= 8)
        .ok_or_else(invalid)?;
    let mut credit_buckets = Vec::with_capacity(buckets.len());
    for bucket in buckets {
        let bucket = bucket.as_object().ok_or_else(invalid)?;
        let kind = plain_str(bucket.get("type").ok_or_else(invalid)?, 32).ok_or_else(invalid)?;
        let total = decimal_str(bucket, "total").ok_or_else(invalid)?;
        let remaining = decimal_str(bucket, "remaining").ok_or_else(invalid)?;
        let mut out = serde_json::Map::new();
        out.insert("type".into(), json!(kind));
        out.insert("total".into(), json!(total));
        out.insert("remaining".into(), json!(remaining));
        for field in ["resets_at_ms", "expires_at_ms"] {
            if let Some(value) = bucket.get(field) {
                out.insert(field.into(), json!(value.as_u64().ok_or_else(invalid)?));
            }
        }
        credit_buckets.push(Value::Object(out));
    }
    let mut plan_out = serde_json::Map::new();
    for field in ["state", "family", "name"] {
        if let Some(value) = plan.get(field).and_then(|value| plain_str(value, 128)) {
            plan_out.insert(field.into(), json!(value));
        }
    }
    if let Some(expires) = plan
        .get("subscription")
        .and_then(Value::as_object)
        .and_then(|sub| sub.get("expires_at_ms"))
        .and_then(Value::as_u64)
    {
        plan_out.insert("subscription".into(), json!({"expires_at_ms": expires}));
    }
    if plan_out.is_empty() {
        return Err(invalid());
    }
    Ok(
        json!({"schema_version":1,"scope":scope,"observed_at_ms":observed_at_ms,
        "plan":Value::Object(plan_out),
        "quota":{"state":"known","allowed":true,"credit_buckets":credit_buckets,"windows":[]},
        "percent_basis":"counters_exact","identity_fields":"not_retained"}),
    )
}
fn normalize_minimax_remains(body: &Value) -> Result<Value> {
    let invalid = || {
        Error::new(
            "source_format_error",
            "MiniMax plan allowance representation is unsupported",
        )
    };
    let base = body
        .get("base_resp")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    // HTTP 200 carries business errors; base_resp.status_code is the only
    // qualified success signal.
    if base.get("status_code").and_then(Value::as_i64) != Some(0) {
        return Err(Error::new(
            "upstream_rejection",
            "MiniMax plan allowance request was rejected",
        ));
    }
    let remains = body
        .get("model_remains")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty() && rows.len() <= 16)
        .ok_or_else(invalid)?;
    let mut rows = Vec::with_capacity(remains.len());
    let mut seen = std::collections::BTreeSet::new();
    for row in remains {
        let row = row.as_object().ok_or_else(invalid)?;
        let name = plain_str(row.get("model_name").ok_or_else(invalid)?, 64).ok_or_else(invalid)?;
        if !seen.insert(name) {
            return Err(invalid());
        }
        let mut out = serde_json::Map::new();
        out.insert("model_name".into(), json!(name));
        let mut percent = false;
        for field in [
            "current_interval_remaining_percent",
            "current_weekly_remaining_percent",
        ] {
            if let Some(value) = row.get(field) {
                out.insert(field.into(), json!(value.as_u64().ok_or_else(invalid)?));
                percent = true;
            }
        }
        // Count fields carry unpublished units and historically ambiguous
        // semantics; they are not retained. A row without either remaining
        // percent is not a qualified allowance fact.
        if !percent {
            return Err(invalid());
        }
        for field in [
            "current_interval_status",
            "current_weekly_status",
            "start_time",
            "end_time",
            "weekly_start_time",
            "weekly_end_time",
            "remains_time",
        ] {
            if let Some(value) = row.get(field) {
                out.insert(field.into(), json!(value.as_u64().ok_or_else(invalid)?));
            }
        }
        rows.push(Value::Object(out));
    }
    Ok(json!({"base_resp":{"status_code":0},"model_remains":rows,
        "percent_basis":"remaining","counter_fields":"not_retained",
        "status_semantics":"uninterpreted","window_bounds_encoding":"epoch_milliseconds",
        "reset_semantics":"unqualified"}))
}
pub fn dashboard(id: &str) -> &'static str {
    match id {
        "deepseek" => "https://platform.deepseek.com/",
        "opencode-go" => "https://opencode.ai/console",
        "codex-subscription" => "https://chatgpt.com/codex/settings/usage",
        "kimi-coding-plan" => "https://www.kimi.com/code/console",
        "glm-coding-plan" => "https://www.bigmodel.cn/coding-plan/personal/usage",
        "stepfun-token-plan" => "https://platform.stepfun.com",
        "minimax-m-plan" => "https://platform.minimax.io/console/usage",
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

    #[test]
    fn new_integrations_resolve_declared_maps_and_scope_regions() {
        let glm = Provider {
            integration: Some("glm-coding-plan".into()),
            ..Default::default()
        };
        let bases = resolve(&glm).unwrap();
        assert_eq!(bases["messages"], "https://open.bigmodel.cn/api/anthropic");
        assert_eq!(bases["responses"], "https://open.bigmodel.cn/api/v1");
        assert_eq!(bases.len(), 3);
        let stepfun = Provider {
            integration: Some("stepfun-token-plan".into()),
            ..Default::default()
        };
        let bases = resolve(&stepfun).unwrap();
        assert_eq!(
            bases["chat-completions"],
            "https://api.stepfun.com/step_plan/v1"
        );
        assert_eq!(bases["messages"], "https://api.stepfun.com/step_plan");
        assert!(!bases.contains_key("responses"));
        assert_eq!(
            integration_id(&Provider {
                metadata: Some("stepfun-step-plan".into()),
                ..Default::default()
            })
            .unwrap(),
            "stepfun-token-plan"
        );
        let mut minimax = Provider {
            integration: Some("minimax-m-plan".into()),
            ..Default::default()
        };
        assert!(resolve(&minimax).is_err());
        minimax.region = Some("global".into());
        assert_eq!(
            resolve(&minimax).unwrap()["messages"],
            "https://api.minimax.io/anthropic"
        );
        minimax.region = Some("china".into());
        assert_eq!(
            resolve(&minimax).unwrap()["messages"],
            "https://api.minimaxi.com/anthropic"
        );
    }

    #[test]
    fn quota_destinations_follow_selected_region_and_declare_auth_style() {
        let mut kimi = Provider {
            integration: Some("kimi-coding-plan".into()),
            ..Default::default()
        };
        kimi.region = Some("china".into());
        assert_eq!(
            observation_destination(&kimi, "quota")
                .unwrap()
                .unwrap()
                .url,
            "https://api.kimi.com/coding/v1/usages"
        );
        kimi.region = Some("overseas".into());
        assert_eq!(
            observation_destination(&kimi, "quota")
                .unwrap()
                .unwrap()
                .url,
            "https://api.kimi.ai/coding/v1/usages"
        );
        let glm = Provider {
            integration: Some("glm-coding-plan".into()),
            ..Default::default()
        };
        let glm_quota = observation_destination(&glm, "quota").unwrap().unwrap();
        assert_eq!(
            glm_quota.url,
            "https://open.bigmodel.cn/api/monitor/usage/quota/limit"
        );
        assert_eq!(glm_quota.auth, AuthStyle::RawKey);
        assert!(observation_destination(&glm, "auth").unwrap().is_none());
        let mut minimax = Provider {
            integration: Some("minimax-m-plan".into()),
            ..Default::default()
        };
        minimax.region = Some("china".into());
        assert_eq!(
            observation_destination(&minimax, "quota")
                .unwrap()
                .unwrap()
                .url,
            "https://api.minimaxi.com/v1/token_plan/remains"
        );
        minimax.region = Some("global".into());
        assert_eq!(
            observation_destination(&minimax, "quota")
                .unwrap()
                .unwrap()
                .url,
            "https://api.minimax.io/v1/token_plan/remains"
        );
        let stepfun = Provider {
            integration: Some("stepfun-token-plan".into()),
            ..Default::default()
        };
        let stepfun_quota = observation_destination(&stepfun, "quota").unwrap().unwrap();
        assert_eq!(
            stepfun_quota.url,
            "https://api.stepfun.com/step_plan/v1/usage"
        );
        assert_eq!(stepfun_quota.auth, AuthStyle::Bearer);
        for id in [
            "kimi-coding-plan",
            "glm-coding-plan",
            "stepfun-token-plan",
            "minimax-m-plan",
        ] {
            let declaration = inspect(id).unwrap();
            assert_eq!(declaration["provider_input_capabilities"], json!([]));
            assert!(
                declaration["endpoint_support"]["operations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|op| op["purpose"] == "quota"
                        && op["method"] == "GET"
                        && op.get("authorization").is_some()),
                "{id} must declare its quota operation with an authorization style"
            );
        }
    }

    fn kimi_usage_body() -> Value {
        json!({
            "usage": {"limit":"100","used":"20","remaining":"80","resetTime":"2026-10-29T15:42:16.495050Z"},
            "limits": [
                {"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},
                 "detail":{"limit":"100","remaining":"100","resetTime":"2026-10-28T04:42:16.495050Z"}}
            ],
            "usages": {"limit_5h":{"used_ratio":0.4341,"reset_time":"2026-10-27T18:42:15Z"}},
            "booster_wallet": {"status":"STATUS_DISABLED","userId":"must-not-retain"}
        })
    }
    #[test]
    fn kimi_usage_keeps_reported_counters_and_drops_unqualified_or_identity_fields() {
        let normalized = normalize_observation("kimi_usage_api", &kimi_usage_body()).unwrap();
        assert_eq!(normalized["usage"]["remaining"], json!("80"));
        assert_eq!(
            normalized["usage"]["resetTime"],
            json!("2026-10-29T15:42:16.495050Z")
        );
        assert_eq!(normalized["limits"][0]["window"]["duration"], json!(300));
        assert_eq!(
            normalized["limits"][0]["window"]["timeUnit"],
            json!("TIME_UNIT_MINUTE")
        );
        assert_eq!(normalized["limits"][0]["detail"]["remaining"], json!("100"));
        assert!(normalized["limits"][0]["detail"].get("used").is_none());
        assert_eq!(normalized["counter_unit"], json!("unqualified"));
        assert!(normalized.get("usages").is_none());
        assert!(normalized.get("booster_wallet").is_none());
        assert_eq!(
            normalize_observation("kimi_usage_api", &normalized).unwrap(),
            normalized
        );
        let mut duplicate = kimi_usage_body();
        let repeated_row = duplicate["limits"][0].clone();
        duplicate["limits"]
            .as_array_mut()
            .unwrap()
            .push(repeated_row);
        assert_eq!(
            normalize_observation("kimi_usage_api", &duplicate)
                .unwrap_err()
                .code,
            "source_format_error"
        );
        let mutations: [fn(&mut Value); 4] = [
            |b| b["usage"]["limit"] = json!("-1"),
            |b| b["usage"]["resetTime"] = json!("2026-10-29 15:42:16Z"),
            |b| {
                b["limits"][0]["detail"]
                    .as_object_mut()
                    .unwrap()
                    .remove("remaining");
            },
            |b| b["limits"][0]["window"]["duration"] = json!(0.5),
        ];
        for mutate in mutations {
            let mut body = kimi_usage_body();
            mutate(&mut body);
            assert_eq!(
                normalize_observation("kimi_usage_api", &body)
                    .unwrap_err()
                    .code,
                "source_format_error"
            );
        }
    }

    fn glm_monitor_body() -> Value {
        json!({"code":200,"success":true,"msg":"ok","data":{"level":"max","limits":[
            {"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":17,"nextResetTime":1793128801192u64},
            {"type":"TOKENS_LIMIT","unit":6,"number":1,"percentage":24,"nextResetTime":1793161879984u64},
            {"type":"TIME_LIMIT","unit":5,"number":1,"usage":4000,"currentValue":0,"remaining":4000,"percentage":0,"nextResetTime":1795231968998u64,"usageDetails":[{"modelCode":"must-not-retain"}]}
        ]}})
    }
    #[test]
    fn glm_monitor_preserves_raw_rows_without_inventing_semantics() {
        let normalized = normalize_observation("glm_monitor_api", &glm_monitor_body()).unwrap();
        assert_eq!(normalized["data"]["level"], json!("max"));
        let rows = normalized["data"]["limits"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["type"], json!("TOKENS_LIMIT"));
        assert_eq!(rows[0]["unit"], json!(3));
        assert_eq!(rows[0]["number"], json!(5));
        assert_eq!(rows[0]["percentage"], json!(17));
        assert_eq!(rows[0]["nextResetTime"], json!(1793128801192u64));
        assert_eq!(rows[2]["remaining"], json!(4000));
        assert_eq!(rows[2]["currentValue"], json!(0));
        assert_eq!(normalized["percent_basis"], json!("consumed"));
        assert_eq!(normalized["counter_unit"], json!("unqualified"));
        assert!(rows[2].get("usageDetails").is_none());
        assert_eq!(
            normalize_observation("glm_monitor_api", &normalized).unwrap(),
            normalized
        );
        // A business error rides an HTTP 200; it is a rejection, not quota.
        let rejected = json!({"code":1315,"success":false,"msg":"wrong product","data":{"level":"max","limits":[]}});
        assert_eq!(
            normalize_observation("glm_monitor_api", &rejected)
                .unwrap_err()
                .code,
            "upstream_rejection"
        );
        let mut format_errors = vec![
            json!({"success":true,"code":200,"data":{"level":"max","limits":[]}}),
            json!({"success":true,"code":200,"data":{"level":"max","limits":[
                {"type":"CREDIT_LIMIT","unit":3,"number":5,"usage":12000,"currentValue":1438}
            ]}}),
        ];
        let mut unknown_type = glm_monitor_body();
        unknown_type["data"]["limits"][0]["type"] = json!("FUTURE_LIMIT");
        format_errors.push(unknown_type);
        let mut duplicate = glm_monitor_body();
        let repeated_row = duplicate["data"]["limits"][0].clone();
        duplicate["data"]["limits"]
            .as_array_mut()
            .unwrap()
            .push(repeated_row);
        format_errors.push(duplicate);
        let mut float_percent = glm_monitor_body();
        float_percent["data"]["limits"][0]["percentage"] = json!(17.5);
        format_errors.push(float_percent);
        for body in format_errors {
            assert_eq!(
                normalize_observation("glm_monitor_api", &body)
                    .unwrap_err()
                    .code,
                "source_format_error"
            );
        }
        // A complete counter triple without percentage stays qualified.
        let mut counters_only = glm_monitor_body();
        counters_only["data"]["limits"][2]
            .as_object_mut()
            .unwrap()
            .remove("percentage");
        let normalized = normalize_observation("glm_monitor_api", &counters_only).unwrap();
        assert!(normalized["data"]["limits"][2].get("percentage").is_none());
    }

    fn stepfun_usage_body() -> Value {
        json!({
            "account": {"id":"acct-must-not-retain"},
            "observed_at_ms": 1793138236124u64,
            "plan": {"state":"known","family":"token","name":"Token-Pro","subscription":{"id":"sub-must-not-retain","expires_at_ms":1798618579000u64}},
            "quota": {"state":"known","allowed":true,"reason":null,"windows":[],
                      "credit_buckets":[{"type":"subscription","total":"8000000000","remaining":"7771085440","used_percent":2.8614319999999998,"resets_at_ms":1795114271000u64,"expires_at_ms":1798664379000u64}]},
            "schema_version": 1,
            "scope": "personal_account"
        })
    }
    #[test]
    fn stepfun_usage_is_schema_gated_and_retains_exact_counters_only() {
        let normalized = normalize_observation("stepfun_usage_api", &stepfun_usage_body()).unwrap();
        assert_eq!(normalized["schema_version"], json!(1));
        assert_eq!(normalized["observed_at_ms"], json!(1793138236124u64));
        assert_eq!(normalized["plan"]["name"], json!("Token-Pro"));
        assert_eq!(
            normalized["quota"]["credit_buckets"][0]["remaining"],
            json!("7771085440")
        );
        assert_eq!(
            normalized["quota"]["credit_buckets"][0]["resets_at_ms"],
            json!(1795114271000u64)
        );
        assert_eq!(normalized["percent_basis"], json!("counters_exact"));
        assert!(
            normalized["quota"]["credit_buckets"][0]
                .get("used_percent")
                .is_none()
        );
        let text = normalized.to_string();
        assert!(!text.contains("acct-must-not-retain"));
        assert!(!text.contains("sub-must-not-retain"));
        assert_eq!(
            normalize_observation("stepfun_usage_api", &normalized).unwrap(),
            normalized
        );
        let mut unknown_state = stepfun_usage_body();
        unknown_state["quota"]["state"] = json!("unknown");
        assert_eq!(
            normalize_observation("stepfun_usage_api", &unknown_state)
                .unwrap_err()
                .code,
            "observation_failed"
        );
        let mut format_errors = Vec::new();
        let mut version = stepfun_usage_body();
        version["schema_version"] = json!(2);
        format_errors.push(version);
        let mut windows = stepfun_usage_body();
        windows["quota"]["windows"] = json!([{"unqualified":"row"}]);
        format_errors.push(windows);
        let mut exponent = stepfun_usage_body();
        exponent["quota"]["credit_buckets"][0]["total"] = json!("8e9");
        format_errors.push(exponent);
        let mut rejected_state = stepfun_usage_body();
        rejected_state["quota"]["reason"] = json!("paused");
        assert_eq!(
            normalize_observation("stepfun_usage_api", &rejected_state)
                .unwrap_err()
                .code,
            "observation_failed"
        );
        for body in format_errors {
            assert_eq!(
                normalize_observation("stepfun_usage_api", &body)
                    .unwrap_err()
                    .code,
                "source_format_error"
            );
        }
    }

    fn minimax_remains_body() -> Value {
        json!({"base_resp":{"status_code":0,"status_msg":"success"},"model_remains":[
            {"model_name":"general","start_time":1793124132831u64,"end_time":1793142132831u64,"remains_time":17999502,
             "current_interval_total_count":0,"current_interval_usage_count":0,"current_interval_remaining_percent":100,"current_interval_status":1,
             "weekly_start_time":1793068800000u64,"weekly_end_time":1793587200000u64,"weekly_remains_time":44694902,
             "current_weekly_total_count":0,"current_weekly_usage_count":0,"current_weekly_remaining_percent":100,"current_weekly_status":3},
            {"model_name":"video","current_interval_remaining_percent":100,"current_interval_status":3,
             "current_weekly_remaining_percent":100,"current_weekly_status":3}
        ]})
    }
    #[test]
    fn minimax_remains_keep_percent_rows_and_never_interpret_status_or_counts() {
        let normalized =
            normalize_observation("minimax_plan_remains_api", &minimax_remains_body()).unwrap();
        let rows = normalized["model_remains"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["model_name"], json!("general"));
        assert_eq!(rows[0]["current_interval_remaining_percent"], json!(100));
        assert_eq!(rows[0]["current_interval_status"], json!(1));
        assert_eq!(rows[0]["start_time"], json!(1793124132831u64));
        assert_eq!(rows[0]["end_time"], json!(1793142132831u64));
        assert_eq!(rows[0]["weekly_start_time"], json!(1793068800000u64));
        assert_eq!(rows[0]["weekly_end_time"], json!(1793587200000u64));
        assert_eq!(rows[0]["remains_time"], json!(17999502));
        assert!(rows[0].get("current_interval_total_count").is_none());
        assert!(rows[0].get("weekly_remains_time").is_none());
        assert_eq!(normalized["percent_basis"], json!("remaining"));
        assert_eq!(normalized["counter_fields"], json!("not_retained"));
        assert_eq!(
            normalize_observation("minimax_plan_remains_api", &normalized).unwrap(),
            normalized
        );
        let rejected = json!({"base_resp":{"status_code":2057,"status_msg":"not token plan token"},"model_remains":[]});
        assert_eq!(
            normalize_observation("minimax_plan_remains_api", &rejected)
                .unwrap_err()
                .code,
            "upstream_rejection"
        );
        let mut format_errors = Vec::new();
        let mut duplicate = minimax_remains_body();
        let repeated_row = duplicate["model_remains"][0].clone();
        duplicate["model_remains"]
            .as_array_mut()
            .unwrap()
            .push(repeated_row);
        format_errors.push(duplicate);
        let mut no_percent = minimax_remains_body();
        no_percent["model_remains"][0]
            .as_object_mut()
            .unwrap()
            .remove("current_interval_remaining_percent");
        no_percent["model_remains"][0]
            .as_object_mut()
            .unwrap()
            .remove("current_weekly_remaining_percent");
        format_errors.push(no_percent);
        format_errors.push(json!({"base_resp":{"status_code":0},"model_remains":[]}));
        for body in format_errors {
            assert_eq!(
                normalize_observation("minimax_plan_remains_api", &body)
                    .unwrap_err()
                    .code,
                "source_format_error"
            );
        }
    }

    #[tokio::test]
    async fn glm_monitor_transport_sends_raw_authorization_header() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::at(dir.path().into());
        let owner = crate::secrets::owner_tag(&store, "provider", "glm-test").unwrap();
        let key_ref = crate::secrets::put(&store, &owner, b"synthetic-glm-key").unwrap();
        let provider = Provider {
            integration: Some("glm-coding-plan".into()),
            auth: Some(crate::config::Auth::ApiKey { key_ref }),
            ..Default::default()
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let body = glm_monitor_body().to_string();
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
            assert!(request.starts_with("get /api/monitor/usage/quota/limit http/1.1\r\n"));
            assert!(request.contains("\r\nauthorization: synthetic-glm-key\r\n"));
            assert!(!request.contains("bearer"));
            let response = format!(
                "HTTP/1.1 200 fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });
        let url = format!("http://{address}/api/monitor/usage/quota/limit");
        let destination = ObservationDestination {
            url: &url,
            ..GLM_MONITOR_DESTINATION
        };
        let result = acquire_observation(&store, &Config::empty(), &provider, destination).await;
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
        let acquired = result.unwrap();
        assert_eq!(acquired.status, 200);
        assert_eq!(acquired.data["data"]["level"], json!("max"));
    }
}
