//! `provider refresh tmax` scopes: shared-key acceptance, route reachability,
//! and observed gateway usage.
//!
//! Every check proves reachability from tmax, never that an Agent target loaded
//! or used the configuration. A check failure is reported in its own item and
//! never prevents another requested scope from running.
use crate::{
    Error, Request, Result,
    config::{self, Config, GatewayDelivery, ModelAlias},
    storage::Store,
};
use serde_json::{Value, json};
use std::time::Duration;

const SCOPES: [&str; 3] = ["auth", "models", "quota"];
/// Preference order for the single protocol a route is checked with.
const PROTOCOLS: [&str; 3] = ["responses", "chat-completions", "messages"];
const MODELS_PATH: &str = "/v1/models";
const AUTH_DEADLINE: Duration = Duration::from_secs(5);
const ROUTE_DEADLINE: Duration = Duration::from_secs(60);
const ERROR_BODY_BYTES: usize = 64 * 1024;
const PUBLICATION_BODY_BYTES: usize = 4 * 1024 * 1024;
const BOUNDARY: &str = "reachability_only";

pub async fn provider_refresh(store: &Store, config: &Config, request: &Request) -> Result<Value> {
    let Some(delivery) = config::gateway_delivery(config) else {
        return Err(Error::new("unknown_resource", "Provider is not configured"));
    };
    let scope = crate::operations::required(request, "scope")?;
    let scopes = if scope == "all" {
        SCOPES.as_slice()
    } else if SCOPES.contains(&scope) {
        std::slice::from_ref(&scope)
    } else {
        return Err(Error::new("invalid_arguments", "Unknown refresh scope"));
    };
    // Only auth and models read daemon readiness. Dry-run contacts neither the
    // daemon nor the listener, so it never captures readiness.
    let ready = if request.dry_run
        || !scopes
            .iter()
            .any(|scope| matches!(*scope, "auth" | "models"))
    {
        None
    } else {
        crate::service::status(store)
            .await
            .ok()
            .as_ref()
            .and_then(|status| Ready::capture(status, &delivery))
    };
    let mut checks = Vec::with_capacity(scopes.len());
    for scope in scopes {
        checks.push(match *scope {
            "auth" => auth_scope(store, &delivery, request.dry_run, ready.as_ref()).await,
            "models" => {
                models_scope(store, config, &delivery, request.dry_run, ready.as_ref()).await
            }
            "quota" => quota_scope(store, request.dry_run).await,
            _ => continue,
        });
        if let Some(check) = checks.last_mut()
            && let Value::Object(fields) = check
        {
            fields.insert("boundary".into(), json!(BOUNDARY));
        }
    }
    Ok(json!({
        "provider": config::GATEWAY_PROVIDER_ID,
        "boundary": BOUNDARY,
        "checks": checks,
    }))
}

/// Daemon facts that make a bounded local check meaningful.
///
/// Both the daemon identity and the listener readiness must be current; a saved
/// reference or a running process alone is not proof that the gateway accepts
/// requests.
struct Ready {
    listen: String,
    loaded_key_ref: Option<String>,
}

impl Ready {
    fn capture(status: &Value, delivery: &GatewayDelivery) -> Option<Self> {
        if status["daemon"]["discovery_state"] != "compatible" || status["gateway"]["ready"] != true
        {
            return None;
        }
        // The running daemon must serve the saved listener and configuration
        // revision; otherwise a probe would verify stale state. A loaded key
        // reference that differs from the saved one is not treated as "not
        // ready": the checks name `key_rotated_not_reloaded` instead.
        if status["gateway"]["listen"].as_str()? != delivery.listen
            || status["gateway"]["config_revision"].as_str()?
                != status["saved_revision"].as_str()?
        {
            return None;
        }
        Some(Self {
            listen: status["gateway"]["listen"].as_str()?.to_owned(),
            loaded_key_ref: status["shared_key"]["loaded_reference"]
                .as_str()
                .map(str::to_owned),
        })
    }
}

async fn auth_scope(
    store: &Store,
    delivery: &GatewayDelivery,
    dry_run: bool,
    ready: Option<&Ready>,
) -> Value {
    if dry_run {
        return json!({"scope":"auth","outcome":"not_executed","reason_code":"dry_run",
            "planned":{"listen":delivery.listen,"method":"GET","path":MODELS_PATH}});
    }
    let Some(ready) = ready else {
        return json!({"scope":"auth","outcome":"failed","reason_code":"gateway_not_ready",
            "repair":"tmax gateway start"});
    };
    if ready.loaded_key_ref.as_deref() != Some(delivery.key_ref.as_str()) {
        return json!({"scope":"auth","outcome":"failed","reason_code":"key_rotated_not_reloaded",
            "repair":"tmax gateway restart"});
    }
    let key_bytes = match super::credential(store.clone(), delivery.key_ref.clone()).await {
        Ok(key) => key,
        Err(error) => return auth_failure(&error.code),
    };
    let key = match std::str::from_utf8(&key_bytes) {
        Ok(key) => key,
        Err(_) => return auth_failure("credential_invalid"),
    };
    let Ok(client) = local_client(AUTH_DEADLINE) else {
        return json!({"scope":"auth","outcome":"failed","reason_code":"gateway_unreachable"});
    };
    let url = format!(
        "http://{}{}",
        ready.listen.trim_end_matches('/'),
        MODELS_PATH
    );
    let response = match client
        .get(url)
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return json!({"scope":"auth","outcome":"failed","reason_code":"gateway_unreachable"});
        }
    };
    let status = response.status().as_u16();
    if response.status().is_success() {
        // Only the publication's model count is retained; response bodies are
        // never echoed into a result.
        let publication_model_count = bounded_body(response, PUBLICATION_BODY_BYTES)
            .await
            .and_then(|body| serde_json::from_slice::<Value>(&body).ok())
            .and_then(|body| body["data"].as_array().map(Vec::len));
        return json!({"scope":"auth","outcome":"completed","proof":"shared_key_accepted",
            "publication_model_count":publication_model_count});
    }
    match status {
        401 | 403 => json!({"scope":"auth","outcome":"failed","reason_code":"shared_key_rejected",
            "http_status":status,"repair":"tmax gateway key replace"}),
        _ => json!({"scope":"auth","outcome":"failed","reason_code":"gateway_response_invalid",
            "http_status":status}),
    }
}

fn auth_failure(reason_code: &str) -> Value {
    json!({"scope":"auth","outcome":"failed","reason_code":reason_code,
        "repair":"tmax gateway key replace"})
}

async fn models_scope(
    store: &Store,
    config: &Config,
    delivery: &GatewayDelivery,
    dry_run: bool,
    ready: Option<&Ready>,
) -> Value {
    if dry_run {
        return json!({"scope":"models","outcome":"not_executed","reason_code":"dry_run",
            "planned":{"routes":planned_routes(config, delivery)}});
    }
    let Some(ready) = ready else {
        return json!({"scope":"models","outcome":"not_executed","reason_code":"gateway_not_ready",
            "routes":delivery.routes,"boundary":BOUNDARY});
    };
    if ready.loaded_key_ref.as_deref() != Some(delivery.key_ref.as_str()) {
        let mut check = models_all_failed(config, delivery, "key_rotated_not_reloaded");
        check["repair"] = json!("tmax gateway restart");
        return check;
    }
    let key_bytes = match super::credential(store.clone(), delivery.key_ref.clone()).await {
        Ok(key) => key,
        Err(error) => return models_all_failed(config, delivery, &error.code),
    };
    let key = match std::str::from_utf8(&key_bytes) {
        Ok(key) => key,
        Err(_) => return models_all_failed(config, delivery, "credential_invalid"),
    };
    let Ok(client) = local_client(ROUTE_DEADLINE) else {
        return models_all_failed(config, delivery, "gateway_unreachable");
    };
    let mut routes = Vec::with_capacity(delivery.routes.len());
    for route in &delivery.routes {
        routes.push(model_route(&client, ready, key, config, route).await);
    }
    models_summary(routes)
}

async fn model_route(
    client: &reqwest::Client,
    ready: &Ready,
    key: &str,
    config: &Config,
    route: &str,
) -> Value {
    let Some(protocol) = route_protocol(config, route) else {
        return json!({"route":route,"protocol":Value::Null,"outcome":"failed",
            "reason_code":"protocol_unavailable"});
    };
    let Ok(path) = crate::provider_integration::operation_path(protocol) else {
        return json!({"route":route,"protocol":protocol,"outcome":"failed",
            "reason_code":"protocol_unavailable"});
    };
    let url = format!("http://{}{}", ready.listen.trim_end_matches('/'), path);
    let body = inference_body(protocol, route);
    let response = match client.post(url).bearer_auth(key).json(&body).send().await {
        Ok(response) => response,
        Err(_) => {
            return json!({"route":route,"protocol":protocol,"outcome":"failed",
                "reason_code":"unreachable"});
        }
    };
    let status = response.status().as_u16();
    if response.status().is_success() {
        return json!({"route":route,"protocol":protocol,"outcome":"completed",
            "proof":"route_reachable_through_gateway"});
    }
    // Only a stable gateway code is retained; response bodies are never echoed.
    let reason_code = bounded_body(response, ERROR_BODY_BYTES)
        .await
        .as_deref()
        .and_then(gateway_reason)
        .unwrap_or_else(|| format!("http_{status}"));
    json!({"route":route,"protocol":protocol,"outcome":"failed",
        "reason_code":reason_code,"http_status":status})
}

fn models_all_failed(config: &Config, delivery: &GatewayDelivery, reason_code: &str) -> Value {
    let routes = delivery
        .routes
        .iter()
        .map(|route| {
            json!({"route":route,"protocol":route_protocol(config, route),"outcome":"failed",
                "reason_code":reason_code})
        })
        .collect();
    models_summary(routes)
}

fn models_summary(routes: Vec<Value>) -> Value {
    let failed = routes
        .iter()
        .filter(|route| route["outcome"] != "completed")
        .count();
    let completed = routes.len() - failed;
    json!({"scope":"models","outcome":if failed == 0 {"completed"} else {"failed"},
        "routes":routes,
        "summary":{"routes":routes.len(),"completed":completed,"failed":failed},
        "boundary":BOUNDARY})
}

async fn quota_scope(store: &Store, dry_run: bool) -> Value {
    if dry_run {
        return json!({"scope":"quota","outcome":"not_executed","reason_code":"dry_run",
            "planned":{"source":"observed_gateway_usage","upstream_contact":false}});
    }
    let usage = crate::service::usage(store).await.unwrap_or_else(|error| {
        // Keep the absent shape so a check that cannot read retained history is
        // reported as not executed instead of as a provider-side balance.
        json!({"snapshot_state":"absent","coverage":{"prior_process_coverage":"unavailable"},
            "records":[],"reason_code":error.code})
    });
    let reason_code = usage.get("reason_code").and_then(Value::as_str);
    json!({"scope":"quota","outcome":if reason_code.is_some() {"not_executed"} else {"completed"},
        "reason_code":reason_code,
        "usage":usage,
        "proof":"observed_gateway_usage",
        "upstream_contact":false})
}

fn planned_routes(config: &Config, delivery: &GatewayDelivery) -> Vec<Value> {
    delivery
        .routes
        .iter()
        .map(|route| json!({"route":route,"protocol":route_protocol(config, route)}))
        .collect()
}

fn preferred_protocol(alias: &ModelAlias) -> Option<&'static str> {
    PROTOCOLS
        .into_iter()
        .find(|protocol| alias.publishes(protocol))
}

/// The declared protocol one enabled route is checked with.
fn route_protocol(config: &Config, route: &str) -> Option<&'static str> {
    config
        .model_aliases
        .iter()
        .find(|alias| alias.id.as_str() == route)
        .and_then(preferred_protocol)
}

/// The smallest legal inference body for one declared protocol.
///
/// It matches the probe body shape with the output bounded to the shortest
/// possible completion. The one-word prompt keeps a route check free of
/// caller content.
fn inference_body(protocol: &str, route: &str) -> Value {
    match protocol {
        "responses" => json!({"model":route,"input":"OK","store":false,"max_output_tokens":1}),
        _ => json!({"model":route,"messages":[{"role":"user","content":"OK"}],"max_tokens":1}),
    }
}

/// Stable reason code from a bounded gateway error body, if one is present.
fn gateway_reason(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let field = |path: &str| value.pointer(path).and_then(Value::as_str);
    field("/reason_code")
        .or_else(|| field("/error/type"))
        .or_else(|| field("/error/reason_code"))
        .or_else(|| field("/error/code"))
        .map(str::to_owned)
}

/// A loopback-only client with one finite per-request deadline.
fn local_client(deadline: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(deadline)
        .build()
        .map_err(|_| {
            Error::new(
                "gateway_unreachable",
                "Cannot initialize the local gateway transport",
            )
        })
}

async fn bounded_body(mut response: reqwest::Response, limit: usize) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len().saturating_add(chunk.len()) > limit {
                    return None;
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => return Some(body),
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    const SHARED_KEY: &[u8] = b"test-shared-gateway-key";

    #[derive(Clone)]
    struct Seen {
        method: String,
        path: String,
        authorization: Option<String>,
        body: Value,
    }

    /// Minimal HTTP/1.1 responder: records one request per connection and
    /// answers from a finite script, then closes the connection.
    async fn http_server(replies: Vec<(u16, &'static str)>) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = seen.clone();
        let replies = Arc::new(Mutex::new(std::collections::VecDeque::from(replies)));
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let Some(request) = read_request(&mut stream).await else {
                    continue;
                };
                recorded.lock().unwrap().push(request);
                let (status, body) = replies.lock().unwrap().pop_front().unwrap_or((500, "{}"));
                let header = format!(
                    "HTTP/1.1 {status} STATUS\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(body.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        (address, seen)
    }

    async fn read_request(stream: &mut TcpStream) -> Option<Seen> {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];
        let header_end = loop {
            let read = stream.read(&mut chunk).await.ok()?;
            if read == 0 {
                return None;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                break end;
            }
            if buffer.len() > 64 * 1024 {
                return None;
            }
        };
        let header = std::str::from_utf8(&buffer[..header_end]).ok()?;
        let mut lines = header.split("\r\n");
        let mut request_line = lines.next()?.split(' ');
        let method = request_line.next()?.to_owned();
        let path = request_line.next()?.to_owned();
        let mut authorization = None;
        let mut content_length = 0usize;
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_owned());
            }
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().ok()?;
            }
        }
        let mut body = buffer[header_end + 4..].to_vec();
        while body.len() < content_length {
            let read = stream.read(&mut chunk).await.ok()?;
            if read == 0 {
                return None;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        body.truncate(content_length);
        Some(Seen {
            method,
            path,
            authorization,
            body: serde_json::from_slice(&body).unwrap_or(Value::Null),
        })
    }

    fn store_with(text: &str) -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        (directory, store)
    }

    fn config_text(listen: &str, key_ref: &str) -> String {
        format!(
            "version=2\n[gateway]\nlisten='{listen}'\nkey_ref='{key_ref}'\n\
             [providers.up]\nintegration='custom'\n\
             [providers.up.endpoints]\n\
             chat-completions='http://127.0.0.1:9'\n\
             responses='http://127.0.0.1:9'\n\
             messages='http://127.0.0.1:9'\n\
             [providers.up.models.alpha]\n[providers.up.models.beta]\n\
             [providers.up.auth]\nmethod='api-key'\nkey_ref='secret:up'\n\
             [[model_aliases]]\nid='alpha'\nprotocols=['chat-completions','responses']\ncandidates=['up/alpha']\n\
             [[model_aliases]]\nid='beta'\nprotocols=['messages']\ncandidates=['up/beta']\n"
        )
    }

    fn request(scope: Option<&str>, dry_run: bool) -> Request {
        Request {
            command: vec!["provider".into(), "refresh".into()],
            operands: vec!["tmax".into()],
            arguments: scope
                .map(|scope| BTreeMap::from([("scope".to_owned(), vec![scope.to_owned()])]))
                .unwrap_or_default(),
            dry_run,
            if_revision: None,
        }
    }

    fn current_delivery(config: &Config) -> GatewayDelivery {
        config::gateway_delivery(config).unwrap()
    }

    #[test]
    fn readiness_requires_a_compatible_daemon_on_the_saved_listener_key_and_revision() {
        let delivery = GatewayDelivery {
            revision: "rev".into(),
            listen: "127.0.0.1:17840".into(),
            key_ref: "secret:gw".into(),
            routes: vec!["route".into()],
        };
        let ready = json!({
            "daemon":{"discovery_state":"compatible"},
            "gateway":{"ready":true,"listen":"127.0.0.1:17840","key_ref":"secret:gw","config_revision":"rev"},
            "saved_revision":"rev",
            "shared_key":{"loaded_reference":"secret:gw"},
        });
        let captured = Ready::capture(&ready, &delivery).unwrap();
        assert_eq!(captured.listen, "127.0.0.1:17840");
        assert_eq!(captured.loaded_key_ref.as_deref(), Some("secret:gw"));
        for not_ready in [
            // no daemon identity
            json!({"daemon":{"discovery_state":"absent"},"gateway":{"ready":true,"listen":"127.0.0.1:17840","key_ref":"secret:gw","config_revision":"rev"},"saved_revision":"rev","shared_key":{"loaded_reference":"secret:gw"}}),
            // listener not ready
            json!({"daemon":{"discovery_state":"compatible"},"gateway":{"ready":false,"listen":"127.0.0.1:17840","key_ref":"secret:gw","config_revision":"rev"},"saved_revision":"rev","shared_key":{"loaded_reference":"secret:gw"}}),
            // daemon still serves a different listener
            json!({"daemon":{"discovery_state":"compatible"},"gateway":{"ready":true,"listen":"127.0.0.1:17841","key_ref":"secret:gw","config_revision":"rev"},"saved_revision":"rev","shared_key":{"loaded_reference":"secret:gw"}}),
            // daemon serves a different saved configuration revision
            json!({"daemon":{"discovery_state":"compatible"},"gateway":{"ready":true,"listen":"127.0.0.1:17840","key_ref":"secret:gw","config_revision":"old"},"saved_revision":"rev","shared_key":{"loaded_reference":"secret:gw"}}),
        ] {
            assert!(
                Ready::capture(&not_ready, &delivery).is_none(),
                "{not_ready}"
            );
        }
        // A daemon that loaded a different key reference is still ready to be
        // checked: the scopes name `key_rotated_not_reloaded` themselves.
        let rotated = json!({
            "daemon":{"discovery_state":"compatible"},
            "gateway":{"ready":true,"listen":"127.0.0.1:17840","key_ref":"secret:old","config_revision":"rev"},
            "saved_revision":"rev",
            "shared_key":{"loaded_reference":"secret:old"},
        });
        let captured = Ready::capture(&rotated, &delivery).unwrap();
        assert_eq!(captured.loaded_key_ref.as_deref(), Some("secret:old"));
    }

    #[tokio::test]
    async fn scope_is_required_and_must_be_known() {
        let text = config_text("127.0.0.1:17840", "secret:gw");
        let (_directory, store) = store_with(&text);
        let config = config::parse(&text).unwrap();
        let error = provider_refresh(&store, &config, &request(None, false))
            .await
            .unwrap_err();
        assert_eq!(error.code, "invalid_arguments");
        assert_eq!(error.field.as_deref(), Some("scope"));
        let error = provider_refresh(&store, &config, &request(Some("unified"), false))
            .await
            .unwrap_err();
        assert_eq!(error.code, "invalid_arguments");
        assert_eq!(error.message, "Unknown refresh scope");
    }

    #[tokio::test]
    async fn dry_run_plans_every_scope_without_contacting_anything() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let text = config_text(&address, "secret:missing");
        let (_directory, store) = store_with(&text);
        let config = config::parse(&text).unwrap();
        let data = provider_refresh(&store, &config, &request(Some("all"), true))
            .await
            .unwrap();
        assert_eq!(data["provider"], "tmax");
        let checks = data["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 3);
        for check in checks {
            assert_eq!(check["outcome"], "not_executed");
            assert_eq!(check["reason_code"], "dry_run");
            assert!(check["planned"].is_object());
        }
        assert_eq!(checks[0]["planned"]["path"], MODELS_PATH);
        assert_eq!(checks[1]["planned"]["routes"][0]["route"], "alpha");
        assert_eq!(checks[1]["planned"]["routes"][0]["protocol"], "responses");
        assert_eq!(checks[1]["planned"]["routes"][1]["route"], "beta");
        assert_eq!(checks[1]["planned"]["routes"][1]["protocol"], "messages");
        assert_eq!(checks[2]["planned"]["upstream_contact"], false);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "dry-run must not contact the listener"
        );
    }

    #[tokio::test]
    async fn auth_reports_gateway_not_ready_without_a_daemon() {
        let text = config_text("127.0.0.1:17840", "secret:gw");
        let (_directory, store) = store_with(&text);
        let config = config::parse(&text).unwrap();
        let data = provider_refresh(&store, &config, &request(Some("auth"), false))
            .await
            .unwrap();
        let check = &data["checks"][0];
        assert_eq!(check["scope"], "auth");
        assert_eq!(check["outcome"], "failed");
        assert_eq!(check["reason_code"], "gateway_not_ready");
        assert_eq!(check["repair"], "tmax gateway start");
    }

    #[tokio::test]
    async fn auth_accepts_the_stored_shared_key_and_counts_the_publication() {
        let (address, seen) = http_server(vec![(
            200,
            r#"{"object":"list","data":[{"id":"alpha"},{"id":"beta"}]}"#,
        )])
        .await;
        let text = config_text(&address, "secret:pending");
        let (_directory, store) = store_with(&text);
        let key_ref = crate::secrets::put(&store, "gateway", SHARED_KEY).unwrap();
        let text = config_text(&address, &key_ref);
        let config = config::parse(&text).unwrap();
        let delivery = current_delivery(&config);
        let ready = Ready {
            listen: address.clone(),
            loaded_key_ref: Some(key_ref),
        };
        let check = auth_scope(&store, &delivery, false, Some(&ready)).await;
        assert_eq!(check["outcome"], "completed");
        assert_eq!(check["proof"], "shared_key_accepted");
        assert_eq!(check["publication_model_count"], 2);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "GET");
        assert_eq!(seen[0].path, MODELS_PATH);
        assert_eq!(
            seen[0].authorization.as_deref(),
            Some("Bearer test-shared-gateway-key")
        );
    }

    #[tokio::test]
    async fn auth_reports_a_rejected_shared_key_with_its_repair() {
        let (address, seen) = http_server(vec![(
            401,
            r#"{"error":{"type":"authentication_failed","message":"Current shared gateway key required"}}"#,
        )])
        .await;
        let text = config_text(&address, "secret:pending");
        let (_directory, store) = store_with(&text);
        let key_ref = crate::secrets::put(&store, "gateway", SHARED_KEY).unwrap();
        let text = config_text(&address, &key_ref);
        let config = config::parse(&text).unwrap();
        let delivery = current_delivery(&config);
        let ready = Ready {
            listen: address,
            loaded_key_ref: Some(key_ref),
        };
        let check = auth_scope(&store, &delivery, false, Some(&ready)).await;
        assert_eq!(check["outcome"], "failed");
        assert_eq!(check["reason_code"], "shared_key_rejected");
        assert_eq!(check["http_status"], 401);
        assert_eq!(check["repair"], "tmax gateway key replace");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn auth_reports_an_unreachable_listener_and_an_invalid_response() {
        // Reserve and release an address so the check proves a refused
        // connection rather than a listener that merely never answers.
        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().to_string()
        };
        let text = config_text(&closed, "secret:pending");
        let (_directory, store) = store_with(&text);
        let key_ref = crate::secrets::put(&store, "gateway", SHARED_KEY).unwrap();
        let text = config_text(&closed, &key_ref);
        let config = config::parse(&text).unwrap();
        let delivery = current_delivery(&config);
        let ready = Ready {
            listen: closed,
            loaded_key_ref: Some(key_ref.clone()),
        };
        let check = auth_scope(&store, &delivery, false, Some(&ready)).await;
        assert_eq!(check["outcome"], "failed");
        assert_eq!(check["reason_code"], "gateway_unreachable");

        let (address, _seen) = http_server(vec![(500, "{}")]).await;
        let text = config_text(&address, &key_ref);
        let config = config::parse(&text).unwrap();
        let delivery = current_delivery(&config);
        let ready = Ready {
            listen: address,
            loaded_key_ref: Some(key_ref),
        };
        let check = auth_scope(&store, &delivery, false, Some(&ready)).await;
        assert_eq!(check["outcome"], "failed");
        assert_eq!(check["reason_code"], "gateway_response_invalid");
        assert_eq!(check["http_status"], 500);
    }

    #[tokio::test]
    async fn auth_fails_a_rotated_loaded_key_reference_before_any_request_or_secret_read() {
        let (address, seen) = http_server(vec![(200, "{}")]).await;
        let text = config_text(&address, "secret:current");
        let (_directory, store) = store_with(&text);
        let config = config::parse(&text).unwrap();
        let delivery = current_delivery(&config);
        // The current reference has no stored secret; the mismatch must be
        // reported first, without resolving anything or contacting the listener.
        let ready = Ready {
            listen: address,
            loaded_key_ref: Some("secret:previous".into()),
        };
        let check = auth_scope(&store, &delivery, false, Some(&ready)).await;
        assert_eq!(check["outcome"], "failed");
        assert_eq!(check["reason_code"], "key_rotated_not_reloaded");
        assert_eq!(check["repair"], "tmax gateway restart");
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn models_check_each_route_with_its_preferred_protocol_and_bounded_body() {
        let (address, seen) = http_server(vec![
            (200, r#"{"object":"response"}"#),
            (
                500,
                r#"{"error":{"type":"upstream_transport_failed","message":"transport"}}"#,
            ),
        ])
        .await;
        let text = config_text(&address, "secret:pending");
        let (_directory, store) = store_with(&text);
        let key_ref = crate::secrets::put(&store, "gateway", SHARED_KEY).unwrap();
        let text = config_text(&address, &key_ref);
        let config = config::parse(&text).unwrap();
        let delivery = current_delivery(&config);
        let ready = Ready {
            listen: address,
            loaded_key_ref: Some(key_ref),
        };
        let check = models_scope(&store, &config, &delivery, false, Some(&ready)).await;
        assert_eq!(check["scope"], "models");
        assert_eq!(check["outcome"], "failed");
        assert_eq!(check["boundary"], "reachability_only");
        assert_eq!(
            check["summary"],
            json!({"routes":2,"completed":1,"failed":1})
        );
        let routes = check["routes"].as_array().unwrap();
        assert_eq!(routes[0]["route"], "alpha");
        assert_eq!(routes[0]["protocol"], "responses");
        assert_eq!(routes[0]["outcome"], "completed");
        assert_eq!(routes[0]["proof"], "route_reachable_through_gateway");
        assert_eq!(routes[1]["route"], "beta");
        assert_eq!(routes[1]["protocol"], "messages");
        assert_eq!(routes[1]["outcome"], "failed");
        assert_eq!(routes[1]["reason_code"], "upstream_transport_failed");
        assert_eq!(routes[1]["http_status"], 500);

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].path, "/responses");
        assert_eq!(
            seen[0].authorization.as_deref(),
            Some("Bearer test-shared-gateway-key")
        );
        assert_eq!(
            seen[0].body,
            json!({"model":"alpha","input":"OK","store":false,"max_output_tokens":1})
        );
        assert_eq!(seen[1].path, "/v1/messages");
        assert_eq!(
            seen[1].body,
            json!({"model":"beta","messages":[{"role":"user","content":"OK"}],"max_tokens":1})
        );
    }

    #[tokio::test]
    async fn models_fall_back_to_the_http_status_without_a_json_reason() {
        let (address, _seen) = http_server(vec![(200, "{}"), (503, "not json")]).await;
        let text = config_text(&address, "secret:pending");
        let (_directory, store) = store_with(&text);
        let key_ref = crate::secrets::put(&store, "gateway", SHARED_KEY).unwrap();
        let text = config_text(&address, &key_ref);
        let config = config::parse(&text).unwrap();
        let delivery = current_delivery(&config);
        let ready = Ready {
            listen: address,
            loaded_key_ref: Some(key_ref),
        };
        let check = models_scope(&store, &config, &delivery, false, Some(&ready)).await;
        let routes = check["routes"].as_array().unwrap();
        assert_eq!(routes[0]["outcome"], "completed");
        assert_eq!(routes[1]["reason_code"], "http_503");
    }

    #[tokio::test]
    async fn models_are_not_executed_when_the_gateway_is_not_ready() {
        let text = config_text("127.0.0.1:17840", "secret:gw");
        let (_directory, store) = store_with(&text);
        let config = config::parse(&text).unwrap();
        let data = provider_refresh(&store, &config, &request(Some("models"), false))
            .await
            .unwrap();
        let check = &data["checks"][0];
        assert_eq!(check["scope"], "models");
        assert_eq!(check["outcome"], "not_executed");
        assert_eq!(check["reason_code"], "gateway_not_ready");
        assert_eq!(check["routes"], json!(["alpha", "beta"]));
        assert_eq!(check["boundary"], "reachability_only");
        assert!(check.get("summary").is_none());
    }

    #[tokio::test]
    async fn quota_reports_no_daemon_history_without_contacting_anything() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let text = config_text(&address, "secret:gw");
        let (_directory, store) = store_with(&text);
        let config = config::parse(&text).unwrap();
        let data = provider_refresh(&store, &config, &request(Some("quota"), false))
            .await
            .unwrap();
        let check = &data["checks"][0];
        assert_eq!(check["scope"], "quota");
        assert_eq!(check["outcome"], "not_executed");
        assert_eq!(check["reason_code"], "no_daemon_history");
        assert_eq!(check["usage"]["reason_code"], "no_daemon_history");
        assert_eq!(
            check["usage"]["coverage"]["prior_process_coverage"],
            "unavailable"
        );
        assert_eq!(check["proof"], "observed_gateway_usage");
        assert_eq!(check["upstream_contact"], false);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "quota scope must not contact the gateway listener"
        );
    }

    #[tokio::test]
    async fn all_scopes_report_independently_when_no_daemon_runs() {
        let text = config_text("127.0.0.1:17840", "secret:gw");
        let (_directory, store) = store_with(&text);
        let config = config::parse(&text).unwrap();
        let data = provider_refresh(&store, &config, &request(Some("all"), false))
            .await
            .unwrap();
        let checks = data["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 3);
        assert_eq!(checks[0]["outcome"], "failed");
        assert_eq!(checks[0]["reason_code"], "gateway_not_ready");
        assert_eq!(checks[1]["outcome"], "not_executed");
        assert_eq!(checks[1]["reason_code"], "gateway_not_ready");
        assert_eq!(checks[2]["outcome"], "not_executed");
        assert_eq!(checks[2]["reason_code"], "no_daemon_history");
    }
}
