use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{
    access_keys::list_access_keys,
    mcp,
    platform::{agents::install_agent_plugin_bytes, dispatch_platform_method},
    state::DesktopState,
};

const MAX_AGENT_PLUGIN_UPLOAD_SIZE: usize = 128 * 1024 * 1024;
const SERVER_SESSION_COOKIE: &str = "mcp_link_session";
const SERVER_SESSION_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;

pub fn build_router(state: Arc<DesktopState>) -> Router {
    let protected = Router::new()
        .route("/api/auth/session", get(auth_session_handler))
        .route("/api/platform/{method}", post(platform_handler))
        .route(
            "/api/agent-plugins/install",
            post(agent_plugin_install_handler)
                .layer(DefaultBodyLimit::max(MAX_AGENT_PLUGIN_UPLOAD_SIZE)),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_server_password,
        ));

    Router::new()
        .route("/health", get(health_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/logout", post(logout_handler))
        .merge(protected)
        .merge(mcp::server::build_mcp_link(state.clone()))
        .merge(crate::gateway::server::build_model_gateway(state.clone()))
        .fallback(crate::embed::static_handler)
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::mirror_request())
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([header::ACCEPT, header::AUTHORIZATION, header::CONTENT_TYPE])
                .allow_credentials(true),
        )
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    password: String,
}

async fn login_handler(
    State(state): State<Arc<DesktopState>>,
    Json(body): Json<LoginRequest>,
) -> Response {
    let expected = state.server_password();
    if !constant_time_eq(body.password.as_bytes(), expected.as_bytes()) {
        return unauthorized("Invalid password");
    }

    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let expires_at = unix_seconds().saturating_add(SERVER_SESSION_TTL_SECONDS);
    if let Ok(mut sessions) = state.server_sessions.lock() {
        sessions.retain(|_, expiry| *expiry > unix_seconds());
        sessions.insert(token.clone(), expires_at);
    } else {
        return internal_error("Failed to create login session");
    }

    let cookie = format!(
        "{SERVER_SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/api; Max-Age={SERVER_SESSION_TTL_SECONDS}"
    );
    let mut response =
        Json(json!({ "ok": true, "result": { "authenticated": true } })).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

async fn logout_handler(State(state): State<Arc<DesktopState>>, request: Request) -> Response {
    if let Some(token) = session_cookie(&request) {
        if let Ok(mut sessions) = state.server_sessions.lock() {
            sessions.remove(token);
        }
    }
    let mut response =
        Json(json!({ "ok": true, "result": { "authenticated": false } })).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "mcp_link_session=; HttpOnly; SameSite=Strict; Path=/api; Max-Age=0",
        ),
    );
    response
}

async fn auth_session_handler() -> Json<Value> {
    Json(json!({ "ok": true, "result": { "authenticated": true } }))
}

async fn agent_plugin_install_handler(
    State(state): State<Arc<DesktopState>>,
    body: Bytes,
) -> Response {
    match install_agent_plugin_bytes(&state, body.to_vec()) {
        Ok(result) => Json(json!({ "ok": true, "result": result })).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": error })),
        )
            .into_response(),
    }
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
    let changes_password = method == "saveSettings"
        && body
            .args
            .first()
            .and_then(Value::as_object)
            .is_some_and(|settings| settings.contains_key("serverPassword"));
    match dispatch_platform_method(&state, &method, body.args).await {
        Ok(mut result) => {
            if method == "getSettings" {
                if let Some(settings) = result.as_object_mut() {
                    settings.remove("serverPassword");
                }
            }
            if changes_password {
                if let Ok(mut sessions) = state.server_sessions.lock() {
                    sessions.clear();
                }
            }
            Json(json!({ "ok": true, "result": result })).into_response()
        }
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
    let expected = state.server_password();
    let password_matches = bearer_token(&request)
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected.as_bytes()));
    let session_matches = session_cookie(&request).is_some_and(|token| {
        state
            .server_sessions
            .lock()
            .map(|mut sessions| {
                let now = unix_seconds();
                sessions.retain(|_, expiry| *expiry > now);
                sessions.contains_key(token)
            })
            .unwrap_or(false)
    });
    if !password_matches && !session_matches {
        return unauthorized("Authentication required");
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

fn session_cookie(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == SERVER_SESSION_COOKIE && !value.is_empty()).then_some(value)
            })
        })
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "ok": false, "error": message })),
    )
        .into_response()
}

fn internal_error(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binary_plugin_upload_is_not_limited_by_platform_json_body_size() {
        let root = std::env::temp_dir().join(format!(
            "mcp-link-http-upload-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = Arc::new(DesktopState::load(root.join("mcp.db")));
        let password = state.server_password();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, build_router(server_state))
                .await
                .unwrap();
        });

        let response = reqwest::Client::new()
            .post(format!("http://{address}/api/agent-plugins/install"))
            .bearer_auth(password)
            .header("Content-Type", "application/octet-stream")
            .body(vec![0_u8; 2_100_000])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_ne!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        server.abort();
        let _ = server.await;
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn login_uses_http_only_cookie_and_settings_do_not_expose_password() {
        let root = std::env::temp_dir().join(format!(
            "mcp-link-http-login-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = Arc::new(DesktopState::load(root.join("mcp.db")));
        assert_eq!(state.server_password(), "admin");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, build_router(server_state))
                .await
                .unwrap();
        });
        let client = reqwest::Client::new();

        let preflight = client
            .request(Method::OPTIONS, format!("http://{address}/api/auth/login"))
            .header(header::ORIGIN, "http://localhost:1420")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
            .send()
            .await
            .unwrap();
        assert_eq!(preflight.status(), StatusCode::OK);
        assert_eq!(
            preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "http://localhost:1420"
        );
        assert_eq!(
            preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .unwrap(),
            "true"
        );

        let rejected = client
            .post(format!("http://{address}/api/auth/login"))
            .json(&json!({ "password": "wrong" }))
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);

        let login = client
            .post(format!("http://{address}/api/auth/login"))
            .json(&json!({ "password": "admin" }))
            .send()
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let set_cookie = login
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("SameSite=Strict"));
        let cookie = set_cookie.split(';').next().unwrap();

        let settings = client
            .post(format!("http://{address}/api/platform/getSettings"))
            .header(header::COOKIE, cookie)
            .json(&json!({ "args": [] }))
            .send()
            .await
            .unwrap();
        assert_eq!(settings.status(), StatusCode::OK);
        let settings: Value = settings.json().await.unwrap();
        assert!(settings["result"].get("serverPassword").is_none());

        let logout = client
            .post(format!("http://{address}/api/auth/logout"))
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(logout.status(), StatusCode::OK);
        let after_logout = client
            .post(format!("http://{address}/api/platform/getSettings"))
            .header(header::COOKIE, cookie)
            .json(&json!({ "args": [] }))
            .send()
            .await
            .unwrap();
        assert_eq!(after_logout.status(), StatusCode::UNAUTHORIZED);

        server.abort();
        let _ = server.await;
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }
}
