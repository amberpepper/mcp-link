use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    access_keys::list_access_keys, mcp, platform::dispatch_platform_method, state::DesktopState,
};

pub fn build_router(state: Arc<DesktopState>) -> Router {
    let protected = Router::new()
        .route("/api/platform/{method}", post(platform_handler))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_server_password,
        ));

    Router::new()
        .route("/health", get(health_handler))
        .merge(protected)
        .merge(mcp::server::build_mcp_link(state.clone()))
        .fallback(crate::embed::static_handler)
        .with_state(state)
}

async fn health_handler(State(state): State<Arc<DesktopState>>) -> Json<Value> {
    let store = state.store.lock();
    let (server_count, access_key_count) = match store {
        Ok(store) => (
            store.servers.len(),
            list_access_keys(&state.access_keys_db_path())
                .map(|keys| keys.len())
                .unwrap_or_default(),
        ),
        Err(_) => (0, 0),
    };

    Json(json!({
        "ok": true,
        "name": "mcp-link-server",
        "serverCount": server_count,
        "accessKeyCount": access_key_count
    }))
}

#[derive(Debug, Deserialize)]
struct PlatformRequest {
    #[serde(default)]
    args: Vec<Value>,
}

async fn platform_handler(
    State(state): State<Arc<DesktopState>>,
    Path(method): Path<String>,
    Json(body): Json<PlatformRequest>,
) -> Response {
    match dispatch_platform_method(&state, &method, body.args).await {
        Ok(result) => Json(json!({ "ok": true, "result": result })).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": error })),
        )
            .into_response(),
    }
}

async fn require_server_password(
    State(state): State<Arc<DesktopState>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(&request) else {
        return unauthorized("Missing bearer token");
    };

    let expected = state.server_password();
    if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
        return unauthorized("Invalid bearer token");
    }
    next.run(request).await
}

fn bearer_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            scheme.eq_ignore_ascii_case("Bearer").then_some(token)
        })
        .filter(|token| !token.is_empty())
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "ok": false, "error": message })),
    )
        .into_response()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}
