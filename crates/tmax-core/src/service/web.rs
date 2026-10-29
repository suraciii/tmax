//! Loopback Web surface, browser protections, protected inputs, and embedded assets.
#[cfg(windows)]
use super::windows_web;
use super::{
    daemon::{Shared, Surface},
    failure,
};
use crate::{Request, Result};
use axum::{
    Router,
    body::to_bytes,
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{future::Future, net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, sync::watch};
use zeroize::{Zeroize, Zeroizing};

const WEB_REQUEST_BYTES: usize = 1024 * 1024;

pub(super) async fn start(state: Shared, requested: Option<SocketAddr>) -> Result<Surface> {
    let listener = TcpListener::bind(
        requested.unwrap_or_else(|| "127.0.0.1:0".parse().expect("constant address")),
    )
    .await
    .map_err(|_| {
        failure(
            "endpoint_occupied",
            "Requested Web endpoint is occupied; no alternate port attempted",
        )
    })?;
    let actual = listener
        .local_addr()
        .map_err(|_| failure("listener_unavailable", "Cannot inspect actual Web address"))?;
    #[cfg(windows)]
    let (listener, listener_closed) = windows_web::Listener::new(listener);
    let (shutdown, mut cancelled) = watch::channel(false);
    let web = WebState {
        daemon: state,
        authority: actual.to_string(),
        origin: format!("http://{actual}"),
    };
    let task = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            Router::new().fallback(web_request).with_state(web),
        )
        .with_graceful_shutdown(async move {
            let _ = cancelled.changed().await;
        })
        .await;
    });
    Ok(Surface {
        address: actual,
        shutdown,
        task,
        #[cfg(windows)]
        listener_closed: Some(listener_closed),
    })
}

#[derive(Clone)]
struct WebState {
    daemon: Shared,
    authority: String,
    origin: String,
}
fn same_origin(headers: &HeaderMap, web: &WebState, mutation: bool) -> bool {
    if headers.get_all("host").iter().count() != 1
        || headers.get("host").and_then(|v| v.to_str().ok()) != Some(web.authority.as_str())
    {
        return false;
    }
    if headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| !matches!(v, "same-origin" | "none"))
    {
        return false;
    }
    match headers.get("origin").and_then(|v| v.to_str().ok()) {
        Some(origin) => origin == web.origin,
        None => !mutation,
    }
}
async fn web_request(State(web): State<WebState>, request: axum::extract::Request) -> Response {
    let (parts, body) = request.into_parts();
    let mutation = parts.method != Method::GET;
    if !same_origin(&parts.headers, &web, mutation) {
        return(StatusCode::FORBIDDEN,axum::Json(json!({"error":{"code":"untrusted_browser_origin","message":"Exact local Host and same-origin JSON required"}}))).into_response();
    }
    let mut response = match (parts.method.as_str(), parts.uri.path()) {
        ("GET", "/") => Html(include_str!("index.html")).into_response(),
        ("GET", "/app.css") => (
            [("content-type", "text/css; charset=utf-8")],
            include_str!("app.css"),
        )
            .into_response(),
        ("GET", "/app.js") => (
            [("content-type", "text/javascript; charset=utf-8")],
            include_str!("app.js"),
        )
            .into_response(),
        ("GET", "/favicon.svg") => (
            [("content-type", "image/svg+xml")],
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40"><rect width="40" height="40" rx="11" fill="#183e32"/><path d="M17 9v18q0 5 7 3M11 16h17M24 10h7v7M25 16l6-6" fill="none" stroke="#e7f3ec" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
        )
            .into_response(),
        ("GET", "/api/catalog") => axum::Json(crate::operations::catalog()).into_response(),
        ("GET", "/api/status") => {
            let store = web.daemon.lock().await.store().clone();
            match super::control::status(&store).await {
                Ok(value) => axum::Json(value).into_response(),
                Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({"error":error}))).into_response(),
            }
        }
        ("POST", "/api/execute") => {
            if parts
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_none_or(|v| v.split(';').next() != Some("application/json"))
            {
                return (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "Same-origin application/json required",
                )
                    .into_response();
            }
            let bytes = match tokio::time::timeout(
                Duration::from_secs(30),
                to_bytes(body, WEB_REQUEST_BYTES),
            )
            .await
            {
                Ok(Ok(v)) => v,
                _ => {
                    return (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Bounded JSON request required",
                    )
                        .into_response();
                }
            };
            let mut envelope:WebOperation=match serde_json::from_slice(&bytes){Ok(v)=>v,Err(_)=>return(StatusCode::BAD_REQUEST,axum::Json(json!({"error":{"code":"invalid_request","message":"Finite structured operation and separate protected inputs required"}}))).into_response()};
            let mut request = envelope.request.clone();
            if request.arguments.keys().any(|key| {
                key.ends_with("-fd") || key.ends_with("-stdin") || key == "non-interactive"
            }) {
                return (
                    StatusCode::BAD_REQUEST,
                    "CLI descriptors and terminal input are unavailable through Web requests",
                )
                    .into_response();
            }
            request
                .arguments
                .insert("non-interactive".into(), vec!["true".into()]);
            let path = request.command.join(" ");
            if envelope.key.is_some() && path != "provider key set" {
                return (
                    StatusCode::BAD_REQUEST,
                    "Supplied keys belong only to the explicit provider key action",
                )
                    .into_response();
            }
            let mut channels = Vec::new();
            if !request.dry_run
                && let Some(key) = envelope.key.take()
            {
                let key = Zeroizing::new(key);
                match protected_form_input(&key) {
                    Ok((file, fd)) => {
                        request
                            .arguments
                            .insert("key-fd".into(), vec![fd.to_string()]);
                        channels.push(file);
                    }
                    Err(error) => {
                        return (StatusCode::BAD_REQUEST, axum::Json(json!({"error":error})))
                            .into_response();
                    }
                }
            }
            // Explicit reveal is a separate protected surface; ordinary operation JSON
            // cannot accidentally expose the shared key to inspection or caching.
            if request.command == ["gateway", "key", "show"] {
                return(StatusCode::BAD_REQUEST,axum::Json(json!({"error":{"code":"protected_reveal_required","message":"Use the explicit Reveal shared key action"}}))).into_response();
            }
            #[cfg(windows)]
            let final_stop =
                if !request.dry_run && matches!(path.as_str(), "gateway stop" | "web stop") {
                    Some(web.daemon.lock().await.shutdown().clone())
                } else {
                    None
                };
            let response = execute_web(request).await;
            drop(channels);
            #[cfg(windows)]
            if let Some(shutdown) = final_stop
                && shutdown.is_stopping()
                && let Err(error) = shutdown.wait_endpoints_drained().await
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(json!({"error":error})),
                )
                    .into_response();
            }
            axum::Json(response).into_response()
        }
        ("POST", "/api/gateway-key/reveal") => {
            if parts
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                != Some("application/json")
            {
                return (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "application/json required",
                )
                    .into_response();
            }
            let bytes = match to_bytes(body, 64).await {
                Ok(v) => v,
                Err(_) => {
                    return (StatusCode::BAD_REQUEST, "Empty JSON object required").into_response();
                }
            };
            if serde_json::from_slice::<Value>(&bytes).ok() != Some(json!({})) {
                return (
                    StatusCode::BAD_REQUEST,
                    "Explicit empty JSON reveal action required",
                )
                    .into_response();
            }
            let daemon = web.daemon.lock().await;
            let config = match crate::config::load(daemon.store()) {
                Ok(v) => v,
                Err(e) => return axum::Json(json!({"error":e})).into_response(),
            };
            let Some(gateway) = config.config.gateway else {
                return (StatusCode::CONFLICT, "Gateway not configured").into_response();
            };
            match crate::secrets::get(daemon.store(), &gateway.key_ref) {
                Ok(key) => match std::str::from_utf8(&key) {
                    Ok(key) => axum::Json(json!({"key":key})).into_response(),
                    Err(_) => (StatusCode::CONFLICT, "Key unavailable").into_response(),
                },
                Err(e) => (StatusCode::CONFLICT, axum::Json(json!({"error":e}))).into_response(),
            }
        }
        _ => (StatusCode::NOT_FOUND, "Undeclared management operation").into_response(),
    };
    response.headers_mut().insert(
        "cache-control",
        "no-store".parse().expect("constant header"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        "nosniff".parse().expect("constant header"),
    );
    response.headers_mut().insert(
        "referrer-policy",
        "no-referrer".parse().expect("constant header"),
    );
    response.headers_mut().insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'".parse().expect("constant header"));
    response
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebOperation {
    request: Request,
    key: Option<String>,
}
impl Drop for WebOperation {
    fn drop(&mut self) {
        if let Some(value) = &mut self.key {
            value.zeroize();
        }
    }
}
fn execute_web(
    request: Request,
) -> std::pin::Pin<Box<dyn Future<Output = crate::Response> + Send>> {
    Box::pin(crate::execute(request))
}
fn protected_form_input(value: &str) -> Result<(std::io::PipeReader, crate::secrets::Descriptor)> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::fd::AsRawFd;
    #[cfg(windows)]
    use std::os::windows::io::AsRawHandle;
    if value.is_empty()
        || value.len() > 2048
        || value.bytes().any(|b| matches!(b, 0 | b'\n' | b'\r'))
    {
        return Err(failure(
            "invalid_protected_input",
            "Protected form input must be a nonempty bounded UTF-8 line",
        ));
    }
    let (reader, mut writer) = std::io::pipe().map_err(|_| {
        failure(
            "protected_input_unavailable",
            "Cannot create protected anonymous input channel",
        )
    })?;
    writer
        .write_all(value.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .map_err(|_| {
            failure(
                "protected_input_unavailable",
                "Cannot populate protected input channel",
            )
        })?;
    drop(writer);
    #[cfg(unix)]
    let fd = reader.as_raw_fd();
    #[cfg(windows)]
    let fd = reader.as_raw_handle() as usize;
    Ok((reader, fd))
}

#[cfg(test)]
mod tests {
    use super::super::daemon::{Daemon, Shutdown};
    use super::*;
    use crate::{gateway::Runtime, storage::Store};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    #[test]
    fn removed_unlock_fields_are_rejected_at_web_protocol_boundary() {
        let request = json!({"command":["gateway","start"],"operands":[],"arguments":{},"dry_run":false,"if_revision":null});
        assert!(
            serde_json::from_value::<WebOperation>(json!({"request":request,"key":null})).is_ok()
        );
        assert!(
            serde_json::from_value::<WebOperation>(
                json!({"request":request,"key":null,"unlock":null})
            )
            .is_err()
        );
    }
    #[test]
    fn protected_web_lines_reach_secret_consumer_without_relaxing_input_bounds() {
        for value in ["synthetic-web-key".to_owned(), "é".repeat(1024)] {
            let (_reader, fd) = protected_form_input(&value).unwrap();
            let request = Request {
                command: vec!["provider".into(), "key".into(), "set".into()],
                operands: vec!["test".into()],
                arguments: std::collections::BTreeMap::from([
                    ("key-fd".into(), vec![fd.to_string()]),
                    ("non-interactive".into(), vec!["true".into()]),
                ]),
                dry_run: false,
                if_revision: None,
            };
            assert_eq!(
                crate::secrets::read_protected_input(&request, "key").unwrap(),
                value.as_bytes()
            );
        }
        for value in [
            "".to_owned(),
            "x".repeat(2049),
            "key\n".into(),
            "key\r".into(),
            "key\0".into(),
        ] {
            let error = protected_form_input(&value).unwrap_err();
            assert_eq!(error.code, "invalid_protected_input");
            if !value.is_empty() {
                assert!(!error.message.contains(&value));
            }
        }
    }
    #[tokio::test]
    async fn web_origin_checks_reject_rebinding_and_forms() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        let runtime = Runtime::new(store.clone(), "test".into()).unwrap();
        let daemon = Arc::new(Mutex::new(Daemon::new(
            store,
            "test".into(),
            runtime,
            Arc::new(Shutdown::new()),
        )));
        let web = WebState {
            daemon,
            authority: "127.0.0.1:1234".into(),
            origin: "http://127.0.0.1:1234".into(),
        };
        let mut headers = HeaderMap::new();
        headers.insert("host", "127.0.0.1:1234".parse().unwrap());
        assert!(same_origin(&headers, &web, false));
        assert!(!same_origin(&headers, &web, true));
        headers.insert("origin", "http://evil.test".parse().unwrap());
        assert!(!same_origin(&headers, &web, true));
        headers.insert("origin", "http://127.0.0.1:1234".parse().unwrap());
        assert!(same_origin(&headers, &web, true));
        headers.insert("host", "evil.test:1234".parse().unwrap());
        assert!(!same_origin(&headers, &web, false));
        // The offline workspace assets use the same local-origin boundary as management.
        for (path, content_type) in [
            ("/", "text/html"),
            ("/app.css", "text/css"),
            ("/app.js", "text/javascript"),
            ("/favicon.svg", "image/svg+xml"),
        ] {
            let request = axum::extract::Request::builder()
                .uri(path)
                .header("host", "127.0.0.1:1234")
                .body(axum::body::Body::empty())
                .unwrap();
            let response = web_request(State(web.clone()), request).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert!(
                response.headers()["content-type"]
                    .to_str()
                    .unwrap()
                    .starts_with(content_type)
            );
            assert_eq!(response.headers()["cache-control"], "no-store");
            let policy = response.headers()["content-security-policy"]
                .to_str()
                .unwrap();
            assert!(policy.contains("script-src 'self'"));
            assert!(!policy.contains("unsafe-inline"));
            assert!(
                !to_bytes(response.into_body(), 1024 * 1024)
                    .await
                    .unwrap()
                    .is_empty()
            );

            let request = axum::extract::Request::builder()
                .uri(path)
                .header("host", "evil.test:1234")
                .body(axum::body::Body::empty())
                .unwrap();
            assert_eq!(
                web_request(State(web.clone()), request).await.status(),
                StatusCode::FORBIDDEN
            );
        }
    }
}
