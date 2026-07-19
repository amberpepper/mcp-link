#[cfg(feature = "desktop")]
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::{
    collections::BTreeSet,
    pin::Pin,
    sync::{Arc, OnceLock},
};

use axum::{
    body::{to_bytes, Body, Bytes},
    extract::State,
    http::{header, HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures::{Stream, StreamExt};
use serde_json::{json, Value};

use super::protocol::{
    convert_error_body, convert_json_body, convert_request_payload, usage_from_json, GatewayFormat,
    GatewayStreamConverter, StreamUsageTracker,
};
use crate::{
    gateway::{
        config::{active_provider_id, list_providers, list_routes, GatewayProvider},
        logs::{
            start_call, GatewayCallCompletion, GatewayCallGuard, GatewayCallStart,
            GatewayTokenUsage,
        },
    },
    state::DesktopState,
};

const MAX_GATEWAY_REQUEST_SIZE: usize = 64 * 1024 * 1024;

pub(crate) fn build_model_gateway(state: Arc<DesktopState>) -> Router<Arc<DesktopState>> {
    Router::<Arc<DesktopState>>::new()
        .route("/openai/v1/models", get(openai_models))
        .route("/openai/v1/responses", post(openai_responses))
        .route("/openai/v1/chat/completions", post(openai_chat_completions))
        .route("/anthropic/v1/messages", post(anthropic_messages))
        .route("/anthropic/v1/models", get(anthropic_models))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_gateway_key,
        ))
}

#[cfg(feature = "desktop")]
pub(crate) fn start_desktop_model_gateway(state: Arc<DesktopState>) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = restart_desktop_model_gateway(state).await {
            eprintln!("Failed to start model gateway: {error}");
        }
    });
}

#[cfg(feature = "desktop")]
pub(crate) async fn restart_desktop_model_gateway(state: Arc<DesktopState>) -> Result<(), String> {
    let previous_task = state
        .model_gateway_task
        .lock()
        .ok()
        .and_then(|mut task| task.take());
    if let Some(handle) = previous_task {
        handle.abort();
        let _ = handle.await;
    }
    if let Ok(mut endpoint) = state.model_gateway_endpoint.lock() {
        *endpoint = None;
    }
    if let Ok(mut error) = state.model_gateway_listener_error.lock() {
        *error = None;
    }

    let bind_addr = desktop_gateway_bind_addr(&state);
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|error| {
            let message = format!("Failed to bind model gateway on {bind_addr}: {error}");
            if let Ok(mut current) = state.model_gateway_listener_error.lock() {
                *current = Some(message.clone());
            }
            message
        })?;
    let actual_addr = listener.local_addr().unwrap_or(bind_addr);
    let endpoint = format!("http://{actual_addr}");
    if let Ok(mut current) = state.model_gateway_endpoint.lock() {
        *current = Some(endpoint.clone());
    }
    let task_state = state.clone();
    let router = build_model_gateway(task_state.clone()).with_state(task_state.clone());
    let handle = tauri::async_runtime::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            let message = format!("Model gateway stopped: {error}");
            eprintln!("{message}");
            if let Ok(mut current) = task_state.model_gateway_listener_error.lock() {
                *current = Some(message);
            }
            if let Ok(mut current) = task_state.model_gateway_endpoint.lock() {
                *current = None;
            }
        }
    });

    if let Ok(mut task) = state.model_gateway_task.lock() {
        *task = Some(handle);
    }
    eprintln!("MCP Link model gateway listening on {endpoint}");
    Ok(())
}

#[cfg(feature = "desktop")]
fn desktop_gateway_bind_addr(state: &DesktopState) -> SocketAddr {
    let (host, port) = state
        .store
        .lock()
        .ok()
        .map(|store| {
            let host = store
                .settings
                .get("modelGatewayListenHost")
                .and_then(Value::as_str)
                .unwrap_or("127.0.0.1");
            let port = store
                .settings
                .get("modelGatewayListenPort")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok())
                .filter(|value| *value > 0)
                .unwrap_or(3285);
            (host.to_string(), port)
        })
        .unwrap_or_else(|| ("127.0.0.1".to_string(), 3285));
    let ip = host
        .parse::<IpAddr>()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    SocketAddr::new(ip, port)
}

async fn require_gateway_key(
    State(state): State<Arc<DesktopState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let protocol = if request.uri().path().starts_with("/anthropic/") {
        "anthropic"
    } else {
        "openai"
    };
    let supplied = bearer_token(request.headers()).or_else(|| {
        request
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
    });
    let expected = state
        .store
        .lock()
        .ok()
        .and_then(|store| {
            store
                .settings
                .get("modelGatewayAccessKey")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    if supplied.is_some_and(|value| {
        !value.is_empty() && constant_time_eq(value.as_bytes(), expected.as_bytes())
    }) {
        next.run(request).await
    } else {
        gateway_error(
            protocol,
            StatusCode::UNAUTHORIZED,
            "Invalid or missing gateway key",
        )
    }
}

async fn openai_models(State(state): State<Arc<DesktopState>>) -> Response {
    let models = match active_models(&state) {
        Ok(models) => models,
        Err(error) => return gateway_error("openai", StatusCode::BAD_REQUEST, &error),
    };
    Json(json!({
        "object": "list",
        "data": models
            .into_iter()
            .map(|model| json!({
                "id": model,
                "object": "model",
                "created": 0,
                "owned_by": "mcp-link"
            }))
            .collect::<Vec<_>>()
    }))
    .into_response()
}

async fn anthropic_models(State(state): State<Arc<DesktopState>>) -> Response {
    let models = match active_models(&state) {
        Ok(models) => models,
        Err(error) => return gateway_error("anthropic", StatusCode::BAD_REQUEST, &error),
    };
    let first_id = models.first().cloned();
    let last_id = models.last().cloned();
    Json(json!({
        "data": models.into_iter().map(|model| json!({
            "id": model.clone(),
            "type": "model",
            "display_name": model,
            "created_at": "1970-01-01T00:00:00Z"
        })).collect::<Vec<_>>(),
        "has_more": false,
        "first_id": first_id,
        "last_id": last_id
    }))
    .into_response()
}

async fn openai_responses(
    State(state): State<Arc<DesktopState>>,
    request: Request<Body>,
) -> Response {
    proxy_request(state, "openai", "responses", request).await
}

async fn openai_chat_completions(
    State(state): State<Arc<DesktopState>>,
    request: Request<Body>,
) -> Response {
    proxy_request(state, "openai", "chat", request).await
}

async fn anthropic_messages(
    State(state): State<Arc<DesktopState>>,
    request: Request<Body>,
) -> Response {
    proxy_request(state, "anthropic", "messages", request).await
}

async fn proxy_request(
    state: Arc<DesktopState>,
    client_protocol: &str,
    operation: &str,
    request: Request<Body>,
) -> Response {
    let client_format = match GatewayFormat::for_operation(operation) {
        Ok(format) => format,
        Err(error) => return gateway_error(client_protocol, StatusCode::BAD_REQUEST, &error),
    };
    let (parts, body) = request.into_parts();
    let body = match to_bytes(body, MAX_GATEWAY_REQUEST_SIZE).await {
        Ok(body) => body,
        Err(error) => {
            return gateway_error(client_protocol, StatusCode::BAD_REQUEST, &error.to_string())
        }
    };
    let mut payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(error) => {
            return gateway_error(
                client_protocol,
                StatusCode::BAD_REQUEST,
                &format!("Invalid JSON request: {error}"),
            )
        }
    };
    let Some(alias) = payload
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return gateway_error(
            client_protocol,
            StatusCode::BAD_REQUEST,
            "Request model is required",
        );
    };
    let (upstream_model, provider) = match resolve_model_target(&state, &alias) {
        Ok(value) => value,
        Err(error) => return gateway_error(client_protocol, StatusCode::BAD_REQUEST, &error),
    };
    let upstream_format = match GatewayFormat::for_provider(&provider.protocol) {
        Ok(format) => format,
        Err(error) => return gateway_error(client_protocol, StatusCode::BAD_REQUEST, &error),
    };
    let streaming = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let needs_conversion = client_format != upstream_format;
    if let Err(error) = set_payload_model(&mut payload, upstream_model.clone()) {
        return gateway_error(client_protocol, StatusCode::BAD_REQUEST, &error);
    }
    if needs_conversion {
        let converted = convert_request_payload(payload, client_format, upstream_format);
        payload = match converted {
            Ok(payload) => payload,
            Err(error) => return gateway_error(client_protocol, StatusCode::BAD_REQUEST, &error),
        };
    }
    let upstream_path = upstream_format.upstream_path();
    let mut upstream_url = join_upstream_url(&provider.base_url, upstream_path);
    if let Some(query) = parts.uri.query() {
        upstream_url.push('?');
        upstream_url.push_str(query);
    }
    let mut upstream = gateway_http_client().request(parts.method, upstream_url);
    for (name, value) in &parts.headers {
        if should_forward_request_header(name.as_str()) {
            upstream = upstream.header(name, value);
        }
    }
    if !provider.api_key.is_empty() {
        upstream = if upstream_format == GatewayFormat::Anthropic {
            upstream.header("x-api-key", &provider.api_key)
        } else {
            upstream.bearer_auth(&provider.api_key)
        };
    }
    if upstream_format == GatewayFormat::Anthropic
        && !parts.headers.contains_key("anthropic-version")
    {
        upstream = upstream.header("anthropic-version", "2023-06-01");
    }
    let serialized = match serde_json::to_vec(&payload) {
        Ok(serialized) => serialized,
        Err(error) => {
            return gateway_error(
                client_protocol,
                StatusCode::BAD_REQUEST,
                &format!("Failed to serialize request: {error}"),
            )
        }
    };
    let mut call = start_call(
        &state,
        GatewayCallStart {
            client_protocol: client_format.protocol_name(),
            upstream_protocol: upstream_format.protocol_name(),
            requested_model: &alias,
            upstream_model: &upstream_model,
            provider_id: &provider.id,
            provider_name: &provider.name,
            streaming,
        },
    );
    let request_id = call.request_id().to_string();
    let upstream = match upstream.body(serialized).send().await {
        Ok(response) => response,
        Err(error) => {
            call.finish(GatewayCallCompletion {
                status: "failed",
                http_status: None,
                first_token_ms: None,
                usage: &GatewayTokenUsage::default(),
                error: Some("Upstream connection failed"),
            });
            return gateway_error_with_request_id(
                client_protocol,
                StatusCode::BAD_GATEWAY,
                &format!("Upstream request failed: {error}"),
                &request_id,
            );
        }
    };
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = upstream.headers().clone();
    if status.is_success() && streaming {
        return response_with_gateway_stream(
            status,
            &headers,
            upstream,
            upstream_format,
            client_format,
            call,
            client_protocol,
        );
    }
    let body = match upstream.bytes().await {
        Ok(body) => body,
        Err(error) => {
            call.finish(GatewayCallCompletion {
                status: "failed",
                http_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                first_token_ms: None,
                usage: &GatewayTokenUsage::default(),
                error: Some("Failed to read upstream response"),
            });
            return gateway_error_with_request_id(
                client_protocol,
                StatusCode::BAD_GATEWAY,
                &error.to_string(),
                &request_id,
            );
        }
    };
    let usage = if status.is_success() {
        usage_from_json(&body, upstream_format)
    } else {
        GatewayTokenUsage::default()
    };
    let converted = if !needs_conversion {
        Ok(body.to_vec())
    } else if status.is_success() {
        convert_json_body(&body, upstream_format, client_format)
    } else {
        convert_error_body(&body, upstream_format, client_format)
    };
    match converted {
        Ok(body) => {
            let succeeded = status.is_success();
            let error = (!succeeded).then(|| upstream_error_summary(&body, status));
            call.finish(GatewayCallCompletion {
                status: if succeeded { "succeeded" } else { "failed" },
                http_status: Some(status.as_u16()),
                first_token_ms: None,
                usage: &usage,
                error: error.as_deref(),
            });
            response_with_body(status, &headers, body, &request_id)
        }
        Err(error) => {
            call.finish(GatewayCallCompletion {
                status: "failed",
                http_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                first_token_ms: None,
                usage: &usage,
                error: Some("Gateway response conversion failed"),
            });
            gateway_error_with_request_id(
                client_protocol,
                StatusCode::BAD_GATEWAY,
                &error,
                &request_id,
            )
        }
    }
}

type UpstreamByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static>>;

struct GatewayResponseStream {
    upstream: UpstreamByteStream,
    converter: Option<GatewayStreamConverter>,
    tracker: StreamUsageTracker,
    call: GatewayCallGuard,
    first_token_ms: Option<u64>,
    http_status: u16,
    finished: bool,
}

fn response_with_gateway_stream(
    status: StatusCode,
    headers: &HeaderMap,
    upstream: reqwest::Response,
    from: GatewayFormat,
    to: GatewayFormat,
    mut call: GatewayCallGuard,
    client_protocol: &str,
) -> Response {
    let request_id = call.request_id().to_string();
    let converter = if from == to {
        None
    } else {
        match GatewayStreamConverter::new(from, to) {
            Ok(converter) => Some(converter),
            Err(error) => {
                call.finish(GatewayCallCompletion {
                    status: "failed",
                    http_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                    first_token_ms: None,
                    usage: &GatewayTokenUsage::default(),
                    error: Some("Gateway stream conversion setup failed"),
                });
                return gateway_error_with_request_id(
                    client_protocol,
                    StatusCode::BAD_GATEWAY,
                    &error,
                    &request_id,
                );
            }
        }
    };
    let stream = futures::stream::unfold(
        GatewayResponseStream {
            upstream: Box::pin(upstream.bytes_stream()),
            converter,
            tracker: StreamUsageTracker::new(from),
            call,
            first_token_ms: None,
            http_status: status.as_u16(),
            finished: false,
        },
        |mut state| async move {
            if state.finished {
                return None;
            }
            loop {
                match state.upstream.next().await {
                    Some(Ok(chunk)) => {
                        state.tracker.push(&chunk);
                        if state.first_token_ms.is_none() && state.tracker.saw_content() {
                            state.first_token_ms = Some(state.call.elapsed_ms());
                        }
                        let output = match state.converter.as_mut() {
                            Some(converter) => converter.push(&chunk),
                            None => Ok(chunk.to_vec()),
                        };
                        match output {
                            Ok(output) if output.is_empty() => continue,
                            Ok(output) => {
                                return Some((
                                    Ok::<Bytes, std::io::Error>(Bytes::from(output)),
                                    state,
                                ));
                            }
                            Err(error) => {
                                let usage = state.tracker.usage().clone();
                                state.call.finish(GatewayCallCompletion {
                                    status: "failed",
                                    http_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                                    first_token_ms: state.first_token_ms,
                                    usage: &usage,
                                    error: Some("Gateway stream conversion failed"),
                                });
                                state.finished = true;
                                return Some((Err(std::io::Error::other(error)), state));
                            }
                        }
                    }
                    Some(Err(error)) => {
                        let usage = state.tracker.usage().clone();
                        state.call.finish(GatewayCallCompletion {
                            status: "failed",
                            http_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                            first_token_ms: state.first_token_ms,
                            usage: &usage,
                            error: Some("Upstream stream failed"),
                        });
                        state.finished = true;
                        return Some((Err(std::io::Error::other(error)), state));
                    }
                    None => {
                        state.tracker.finish();
                        if state.first_token_ms.is_none() && state.tracker.saw_content() {
                            state.first_token_ms = Some(state.call.elapsed_ms());
                        }
                        let output = match state.converter.as_mut() {
                            Some(converter) => converter.finish(),
                            None => Ok(Vec::new()),
                        };
                        let usage = state.tracker.usage().clone();
                        match output {
                            Ok(output) => {
                                state.call.finish(GatewayCallCompletion {
                                    status: "succeeded",
                                    http_status: Some(state.http_status),
                                    first_token_ms: state.first_token_ms,
                                    usage: &usage,
                                    error: None,
                                });
                                state.finished = true;
                                return (!output.is_empty())
                                    .then(|| (Ok(Bytes::from(output)), state));
                            }
                            Err(error) => {
                                state.call.finish(GatewayCallCompletion {
                                    status: "failed",
                                    http_status: Some(StatusCode::BAD_GATEWAY.as_u16()),
                                    first_token_ms: state.first_token_ms,
                                    usage: &usage,
                                    error: Some("Gateway stream finalization failed"),
                                });
                                state.finished = true;
                                return Some((Err(std::io::Error::other(error)), state));
                            }
                        }
                    }
                }
            }
        },
    );
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-request-id", &request_id);
    for (name, value) in headers {
        if should_forward_response_header(name.as_str())
            && name != &header::CONTENT_TYPE
            && name != &header::CACHE_CONTROL
        {
            response = response.header(name, value);
        }
    }
    response
        .body(Body::from_stream(stream))
        .unwrap_or_else(|error| {
            gateway_error_with_request_id(
                client_protocol,
                StatusCode::BAD_GATEWAY,
                &error.to_string(),
                &request_id,
            )
        })
}

fn set_payload_model(payload: &mut Value, model: String) -> Result<(), String> {
    payload
        .as_object_mut()
        .ok_or_else(|| "Gateway request body must be a JSON object".to_string())?
        .insert("model".to_string(), Value::String(model));
    Ok(())
}

fn response_with_body(
    status: StatusCode,
    headers: &HeaderMap,
    body: Vec<u8>,
    request_id: &str,
) -> Response {
    let mut response = Response::builder()
        .status(status)
        .header("x-request-id", request_id);
    for (name, value) in headers {
        if should_forward_response_header(name.as_str()) && name != &header::CONTENT_TYPE {
            response = response.header(name, value);
        }
    }
    if body.starts_with(b"data:") || body.starts_with(b"event:") {
        response = response.header(header::CONTENT_TYPE, "text/event-stream");
    } else {
        response = response.header(header::CONTENT_TYPE, "application/json");
    }
    response.body(Body::from(body)).unwrap_or_else(|error| {
        gateway_error("openai", StatusCode::BAD_GATEWAY, &error.to_string())
    })
}

fn resolve_model_target(
    state: &DesktopState,
    alias: &str,
) -> Result<(String, GatewayProvider), String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock gateway state".to_string())?;
    let active_id = active_provider_id(&store)
        .ok_or_else(|| "No current API provider is selected".to_string())?;
    let provider = list_providers(&store)
        .into_iter()
        .find(|provider| provider.id == active_id && provider.enabled)
        .ok_or_else(|| "The current API provider is disabled or missing".to_string())?;
    let upstream_model = list_routes(&store)
        .into_iter()
        .find(|route| route.provider_id == provider.id && route.alias == alias)
        .map(|route| route.upstream_model)
        .unwrap_or_else(|| alias.to_string());
    Ok((upstream_model, provider))
}

fn active_models(state: &DesktopState) -> Result<Vec<String>, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock gateway state".to_string())?;
    let active_id = active_provider_id(&store)
        .ok_or_else(|| "No current API provider is selected".to_string())?;
    let provider = list_providers(&store)
        .into_iter()
        .find(|provider| provider.id == active_id && provider.enabled)
        .ok_or_else(|| "The current API provider is disabled or missing".to_string())?;
    let mut models = provider.models.into_iter().collect::<BTreeSet<_>>();
    models.extend(
        list_routes(&store)
            .into_iter()
            .filter(|route| route.provider_id == provider.id)
            .map(|route| route.alias),
    );
    Ok(models.into_iter().collect())
}

fn join_upstream_url(base_url: &str, path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") && path.starts_with("/v1/") {
        format!("{base}{}", &path[3..])
    } else {
        format!("{base}{path}")
    }
}

fn gateway_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("Bearer")
                .then_some(token.trim())
        })
}

fn should_forward_request_header(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "x-api-key"
            | "host"
            | "content-length"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn should_forward_response_header(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "content-length"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn gateway_error(protocol: &str, status: StatusCode, message: &str) -> Response {
    if protocol == "anthropic" {
        (status, Json(json!({ "type": "error", "error": { "type": "gateway_error", "message": message } }))).into_response()
    } else {
        (
            status,
            Json(json!({ "error": { "type": "gateway_error", "message": message } })),
        )
            .into_response()
    }
}

fn gateway_error_with_request_id(
    protocol: &str,
    status: StatusCode,
    message: &str,
    request_id: &str,
) -> Response {
    let mut response = gateway_error(protocol, status, message);
    if let Ok(value) = request_id.parse() {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

fn upstream_error_summary(body: &[u8], status: StatusCode) -> String {
    let kind = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("type").or_else(|| error.get("code")))
                .or_else(|| value.get("type"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    match kind {
        Some(kind) => format!("Upstream HTTP {} ({kind})", status.as_u16()),
        None => format!("Upstream HTTP {}", status.as_u16()),
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}
