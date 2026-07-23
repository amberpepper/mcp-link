use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::{collections::HashMap, future::Future, sync::Arc};

use axum::routing::get;
use axum::{
    body::Body,
    extract::State,
    http::{
        header::{AUTHORIZATION, WWW_AUTHENTICATE},
        request::Parts,
        Request, StatusCode,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::any_service,
    Json, Router,
};
use rmcp::{
    handler::server::ServerHandler,
    model::{
        CallToolRequestMethod, CallToolRequestParams, CallToolResult, ContentBlock,
        ErrorData as McpError, GetPromptRequestMethod, GetPromptRequestParams, GetPromptResult,
        Implementation, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestMethod,
        ReadResourceRequestParams, ReadResourceResult, ServerCapabilities, ServerInfo, Tool,
    },
    service::{NotificationContext, Peer, RequestContext, RoleClient, RoleServer},
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
};
use serde_json::{json, Map, Value};

use crate::access_keys::{authenticate_access_key, AccessKeyContext};
use crate::state::{find_server_mut, DesktopState};
use crate::util::{json::value_id, security::sanitize_for_security_boundary, time::now_millis};
use crate::workflow::{
    executor::{execute_workflow_hook_node, execute_workflow_value},
    topology::{determine_workflow_execution_order, is_valid_workflow},
    McpValueFuture, McpValueHandler,
};

const ROUTER_LIST_TOOLS_TOOL: &str = "mcp_router_list_tools";
const ROUTER_CALL_TOOL_TOOL: &str = "mcp_router_call_tool";

#[cfg(feature = "desktop")]
pub(crate) fn start_desktop_mcp_http_server(state: Arc<DesktopState>) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = restart_desktop_mcp_http_server(state).await {
            eprintln!("Failed to start desktop MCP endpoint: {error}");
        }
    });
}

#[cfg(feature = "desktop")]
pub(crate) async fn restart_desktop_mcp_http_server(
    state: Arc<DesktopState>,
) -> Result<(), String> {
    let previous_task = state
        .mcp_server_task
        .lock()
        .ok()
        .and_then(|mut task| task.take());
    if let Some(handle) = previous_task {
        handle.abort();
        let _ = handle.await;
    }
    clear_mcp_listener_status(&state);

    let bind_addr = configured_mcp_bind_addr(&state);
    let listener = bind_desktop_listener(bind_addr).await.map_err(|error| {
        set_mcp_listener_error(&state, error.clone());
        error
    })?;
    let actual_addr = listener.local_addr().unwrap_or(bind_addr);
    let endpoint = set_mcp_endpoint(&state, actual_addr);

    let router = Router::new()
        .route("/health", get(mcp_health))
        .merge(build_mcp_link(state.clone()))
        .with_state(state.clone());
    let task_state = state.clone();
    let handle = tauri::async_runtime::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            let message = format!("Desktop MCP endpoint stopped: {error}");
            eprintln!("{message}");
            set_mcp_listener_error(&task_state, message);
            if let Ok(mut current) = task_state.mcp_endpoint.lock() {
                *current = None;
            }
        }
    });
    if let Ok(mut task) = state.mcp_server_task.lock() {
        *task = Some(handle);
    }
    eprintln!("MCP Link desktop endpoint listening on {endpoint}");
    Ok(())
}

fn configured_mcp_bind_addr(state: &DesktopState) -> SocketAddr {
    let (host, port) = state
        .store
        .lock()
        .ok()
        .map(|store| {
            let host = store
                .settings
                .get("desktopMcpListenHost")
                .and_then(Value::as_str)
                .unwrap_or("127.0.0.1")
                .to_string();
            let port = store
                .settings
                .get("desktopMcpListenPort")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok())
                .filter(|value| *value > 0)
                .unwrap_or(3284);
            (host, port)
        })
        .unwrap_or_else(|| ("127.0.0.1".to_string(), 3284));

    let host = std::env::var("MCP_LINK_DESKTOP_HOST").unwrap_or(host);
    let port = std::env::var("MCP_LINK_DESKTOP_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(port);
    let ip = host
        .parse::<IpAddr>()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

    SocketAddr::new(ip, port)
}

#[cfg(feature = "server")]
pub(crate) async fn start_server_mcp_http_server(state: Arc<DesktopState>, main_addr: SocketAddr) {
    let bind_addr = configured_mcp_bind_addr(&state);
    if bind_addr == main_addr
        || (main_addr.ip().is_unspecified() && main_addr.port() == bind_addr.port())
    {
        let endpoint_addr = if main_addr.ip().is_unspecified() {
            bind_addr
        } else {
            main_addr
        };
        set_mcp_endpoint(&state, endpoint_addr);
        return;
    }

    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "Failed to bind configured MCP endpoint on {bind_addr}: {error}; using main server endpoint {main_addr}"
            );
            set_mcp_endpoint(&state, main_addr);
            return;
        }
    };
    let actual_addr = listener.local_addr().unwrap_or(bind_addr);
    let endpoint = set_mcp_endpoint(&state, actual_addr);
    let router = Router::new()
        .route("/health", get(mcp_health))
        .merge(build_mcp_link(state.clone()))
        .with_state(state);
    eprintln!("MCP Link configured endpoint listening on {endpoint}");
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            eprintln!("Configured MCP endpoint stopped: {error}");
        }
    });
}

#[cfg(feature = "desktop")]
async fn bind_desktop_listener(bind_addr: SocketAddr) -> Result<tokio::net::TcpListener, String> {
    match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(listener) => Ok(listener),
        Err(error) => {
            eprintln!("Failed to bind desktop MCP endpoint on {bind_addr}: {error}");
            let fallback = SocketAddr::from((Ipv4Addr::LOCALHOST, bind_addr.port()));
            if bind_addr == fallback {
                return Err(format!(
                    "Failed to bind desktop MCP endpoint on {bind_addr}: {error}"
                ));
            }
            match tokio::net::TcpListener::bind(fallback).await {
                Ok(listener) => {
                    eprintln!("Fell back to desktop MCP endpoint on {fallback}");
                    Ok(listener)
                }
                Err(fallback_error) => {
                    Err(format!(
                        "Failed to bind desktop MCP endpoint on {bind_addr} ({error}) and fallback {fallback} ({fallback_error})"
                    ))
                }
            }
        }
    }
}

fn format_mcp_endpoint(addr: SocketAddr) -> String {
    format!("http://{addr}/mcp")
}

pub(crate) fn set_mcp_endpoint(state: &DesktopState, addr: SocketAddr) -> String {
    let endpoint = format_mcp_endpoint(addr);
    if let Ok(mut current) = state.mcp_endpoint.lock() {
        *current = Some(endpoint.clone());
    }
    if let Ok(mut error) = state.mcp_listener_error.lock() {
        *error = None;
    }
    endpoint
}

#[cfg(feature = "desktop")]
fn clear_mcp_listener_status(state: &DesktopState) {
    if let Ok(mut endpoint) = state.mcp_endpoint.lock() {
        *endpoint = None;
    }
    if let Ok(mut error) = state.mcp_listener_error.lock() {
        *error = None;
    }
}

#[cfg(feature = "desktop")]
fn set_mcp_listener_error(state: &DesktopState, message: String) {
    if let Ok(mut error) = state.mcp_listener_error.lock() {
        *error = Some(message);
    }
}

pub(crate) fn mcp_endpoint_status(state: &DesktopState) -> Value {
    let endpoint = state
        .mcp_endpoint
        .lock()
        .ok()
        .and_then(|current| current.clone());
    let error = state
        .mcp_listener_error
        .lock()
        .ok()
        .and_then(|current| current.clone());
    json!({
        "endpoint": endpoint.clone().unwrap_or_else(|| current_mcp_endpoint(state)),
        "running": endpoint.is_some(),
        "error": error,
    })
}

pub(crate) fn current_mcp_endpoint(state: &DesktopState) -> String {
    if let Ok(current) = state.mcp_endpoint.lock() {
        if let Some(endpoint) = current.as_ref() {
            return endpoint.clone();
        }
    }

    let (host, port) = state
        .store
        .lock()
        .ok()
        .map(|store| {
            let host = store
                .settings
                .get("desktopMcpListenHost")
                .and_then(Value::as_str)
                .unwrap_or("127.0.0.1")
                .to_string();
            let port = store
                .settings
                .get("desktopMcpListenPort")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok())
                .filter(|value| *value > 0)
                .unwrap_or(3284);
            (host, port)
        })
        .unwrap_or_else(|| ("127.0.0.1".to_string(), 3284));
    // 0.0.0.0 is bind-all; clients still need a concrete host to connect.
    let display_host = if host == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    };
    format!("http://{display_host}:{port}/mcp")
}

pub(crate) fn build_mcp_link(state: Arc<DesktopState>) -> Router<Arc<DesktopState>> {
    let service_state = state.clone();
    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(DesktopMcpService {
                state: service_state.clone(),
                client_id: uuid::Uuid::new_v4().to_string(),
            })
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .disable_allowed_hosts()
            .with_stateful_mode(true),
    );

    Router::<Arc<DesktopState>>::new()
        .route("/mcp", any_service(mcp_service))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_mcp_access_key,
        ))
}

async fn require_mcp_access_key(
    State(state): State<Arc<DesktopState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(&request) else {
        return unauthorized_response("Missing bearer token");
    };
    let db_path = state.access_keys_db_path();

    match authenticate_access_key(&db_path, token) {
        Ok(Some(context)) => {
            request.extensions_mut().insert(context);
            next.run(request).await
        }
        Ok(None) => unauthorized_response("Invalid bearer token"),
        Err(error) => {
            eprintln!("Failed to authenticate MCP access key: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn bearer_token(request: &Request<Body>) -> Option<&str> {
    request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            scheme.eq_ignore_ascii_case("Bearer").then_some(token)
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn unauthorized_response(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(WWW_AUTHENTICATE, "Bearer")],
        Json(json!({ "error": message })),
    )
        .into_response()
}

async fn mcp_health(State(state): State<Arc<DesktopState>>) -> Json<Value> {
    let server_count = state
        .store
        .lock()
        .map(|store| store.servers.len())
        .unwrap_or_default();
    let connected_server_count = state
        .runtimes
        .lock()
        .map(|runtimes| runtimes.len())
        .unwrap_or_default();
    let endpoint = current_mcp_endpoint(&state);

    Json(json!({
        "ok": true,
        "name": "mcp-link-desktop",
        "serverCount": server_count,
        "connectedServerCount": connected_server_count,
        "mcpEndpoint": endpoint
    }))
}

#[derive(Clone)]
struct DesktopMcpService {
    state: Arc<DesktopState>,
    client_id: String,
}

impl ServerHandler for DesktopMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .enable_resources()
                .enable_resources_list_changed()
                .enable_prompts()
                .enable_prompts_list_changed()
                .build(),
        )
        .with_server_info(
            Implementation::new("mcp-link-desktop", env!("CARGO_PKG_VERSION"))
                .with_title("MCP Link Desktop"),
        )
    }

    fn on_initialized(
        &self,
        context: NotificationContext<RoleServer>,
    ) -> impl Future<Output = ()> + Send + '_ {
        let state = self.state.clone();
        let client_id = self.client_id.clone();
        async move {
            register_mcp_client(&state, &client_id, context.peer);
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let state = self.state.clone();
        let client_id = self.client_id.clone();
        async move {
            register_mcp_client(&state, &client_id, context.peer.clone());
            let access = access_context(&context)?;
            list_tools_with_workflows(state, access).await
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let state = self.state.clone();
        let client_id = self.client_id.clone();
        async move {
            register_mcp_client(&state, &client_id, context.peer.clone());
            let access = access_context(&context)?;
            call_tool_with_workflows(state, request, access).await
        }
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        let state = self.state.clone();
        let client_id = self.client_id.clone();
        async move {
            register_mcp_client(&state, &client_id, context.peer.clone());
            let access = access_context(&context)?;
            list_resources_with_workflows(state, access).await
        }
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_ {
        let state = self.state.clone();
        let client_id = self.client_id.clone();
        async move {
            register_mcp_client(&state, &client_id, context.peer.clone());
            let access = access_context(&context)?;
            list_resource_templates_direct(state, &access).await
        }
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        let state = self.state.clone();
        let client_id = self.client_id.clone();
        async move {
            register_mcp_client(&state, &client_id, context.peer.clone());
            let access = access_context(&context)?;
            read_resource_with_workflows(state, request, access).await
        }
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, McpError>> + Send + '_ {
        let state = self.state.clone();
        let client_id = self.client_id.clone();
        async move {
            register_mcp_client(&state, &client_id, context.peer.clone());
            let access = access_context(&context)?;
            list_prompts_with_workflows(state, access).await
        }
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResult, McpError>> + Send + '_ {
        let state = self.state.clone();
        let client_id = self.client_id.clone();
        async move {
            register_mcp_client(&state, &client_id, context.peer.clone());
            let access = access_context(&context)?;
            get_prompt_with_workflows(state, request, access).await
        }
    }
}

fn register_mcp_client(state: &DesktopState, client_id: &str, peer: Peer<RoleServer>) {
    let Ok(mut peers) = state.mcp_client_peers.lock() else {
        return;
    };
    peers.insert(client_id.to_string(), peer);
}

pub(crate) async fn notify_mcp_clients_all_lists_changed(state: &DesktopState) {
    notify_mcp_clients_list_changed(state, true, true, true).await;
}

pub(crate) async fn notify_mcp_clients_tools_changed(state: &DesktopState) {
    notify_mcp_clients_list_changed(state, true, false, false).await;
}

async fn notify_mcp_clients_list_changed(
    state: &DesktopState,
    tools: bool,
    resources: bool,
    prompts: bool,
) {
    let peers = state
        .mcp_client_peers
        .lock()
        .map(|peers| {
            peers
                .iter()
                .map(|(id, peer)| (id.clone(), peer.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut stale_peer_ids = Vec::new();
    for (id, peer) in peers {
        let mut failed = false;
        if tools && peer.notify_tool_list_changed().await.is_err() {
            failed = true;
        }
        if resources && peer.notify_resource_list_changed().await.is_err() {
            failed = true;
        }
        if prompts && peer.notify_prompt_list_changed().await.is_err() {
            failed = true;
        }
        if failed {
            stale_peer_ids.push(id);
        }
    }

    if stale_peer_ids.is_empty() {
        return;
    }

    if let Ok(mut peers) = state.mcp_client_peers.lock() {
        for id in stale_peer_ids {
            peers.remove(&id);
        }
    }
}

fn access_context(context: &RequestContext<RoleServer>) -> Result<AccessKeyContext, McpError> {
    context
        .extensions
        .get::<Parts>()
        .and_then(|parts| parts.extensions.get::<AccessKeyContext>())
        .cloned()
        .ok_or_else(|| McpError::internal_error("Missing MCP access key context".to_string(), None))
}

const MAX_LOGS_PER_SERVER: usize = 500;

fn append_request_log(
    state: &DesktopState,
    server_id: &str,
    request_type: &str,
    params: Value,
    status: &str,
    duration: i64,
    response_data: Option<Value>,
    error_message: Option<String>,
) {
    let Ok(mut store) = state.store.lock() else {
        return;
    };

    let server_name = store
        .servers
        .iter()
        .find(|s| value_id(s) == Some(server_id))
        .and_then(|s| s.get("name").and_then(Value::as_str))
        .unwrap_or("MCP Server")
        .to_string();

    let entry = json!({
        "id": format!("{}-{}", server_id, now_millis()),
        "timestamp": now_millis(),
        "clientId": "local",
        "clientName": "Local",
        "serverId": server_id,
        "serverName": server_name,
        "requestType": request_type,
        "requestParams": sanitize_for_security_boundary(&params),
        "responseStatus": status,
        "responseData": response_data
            .as_ref()
            .map(sanitize_for_security_boundary)
            .unwrap_or(Value::Null),
        "errorMessage": error_message,
        "duration": duration,
    });

    let Ok(server) = find_server_mut(&mut store, server_id) else {
        return;
    };
    let logs = if let Some(arr) = server.get_mut("logs").and_then(Value::as_array_mut) {
        arr
    } else {
        server["logs"] = json!([]);
        server.get_mut("logs").unwrap().as_array_mut().unwrap()
    };
    logs.push(entry);
    if logs.len() > MAX_LOGS_PER_SERVER {
        let drain = logs.len() - MAX_LOGS_PER_SERVER;
        logs.drain(..drain);
    }

    drop(store);
    let _ = state.save();
}

fn runtime_peers(
    state: &DesktopState,
    access: &AccessKeyContext,
) -> Result<Vec<(String, Peer<RoleClient>)>, McpError> {
    let runtimes = state.runtimes.lock().map_err(|_| {
        McpError::internal_error("Failed to lock desktop runtimes".to_string(), None)
    })?;
    Ok(runtimes
        .iter()
        .filter(|(id, _)| access.allows_server(id))
        .map(|(id, runtime)| (id.clone(), runtime.client.peer().clone()))
        .collect())
}

fn runtime_peer(
    state: &DesktopState,
    id: &str,
    access: &AccessKeyContext,
) -> Result<Peer<RoleClient>, McpError> {
    if !access.allows_server(id) {
        return Err(McpError::method_not_found::<ReadResourceRequestMethod>());
    }
    let runtimes = state.runtimes.lock().map_err(|_| {
        McpError::internal_error("Failed to lock desktop runtimes".to_string(), None)
    })?;
    runtimes
        .get(id)
        .map(|runtime| runtime.client.peer().clone())
        .ok_or_else(|| McpError::method_not_found::<ReadResourceRequestMethod>())
}

fn route_resource_uri(server_id: &str, uri: &str) -> String {
    format!("resource://{server_id}/{uri}")
}

fn split_routed_resource_uri(uri: &str) -> Option<(String, String)> {
    let rest = uri.strip_prefix("resource://")?;
    let (server_id, original_uri) = rest.split_once('/')?;
    Some((server_id.to_string(), original_uri.to_string()))
}

fn server_tool_permissions(
    state: &DesktopState,
) -> Result<HashMap<String, HashMap<String, bool>>, McpError> {
    let store = state
        .store
        .lock()
        .map_err(|_| McpError::internal_error("Failed to lock desktop state".to_string(), None))?;

    Ok(store
        .servers
        .iter()
        .filter_map(|server| {
            let id = value_id(server)?.to_string();
            let permissions = server
                .get("toolPermissions")
                .and_then(Value::as_object)
                .map(|object| {
                    object
                        .iter()
                        .filter_map(|(key, value)| {
                            value.as_bool().map(|value| (key.clone(), value))
                        })
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();
            Some((id, permissions))
        })
        .collect())
}

fn is_router_gateway_tool(name: &str) -> bool {
    matches!(name, ROUTER_LIST_TOOLS_TOOL | ROUTER_CALL_TOOL_TOOL)
}

fn router_gateway_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            ROUTER_LIST_TOOLS_TOOL,
            "List the MCP tools currently available through this router. Use this when a tool was added or enabled after the client session started. Results respect the current access key and tool permissions.",
            json!({
                "type": "object",
                "properties": {
                    "server_id": {
                        "type": "string",
                        "description": "Optional MCP server ID used to limit discovery to one server."
                    }
                },
                "additionalProperties": false
            })
            .as_object()
            .expect("router list tools schema must be an object")
            .clone(),
        ),
        Tool::new(
            ROUTER_CALL_TOOL_TOOL,
            "Call an MCP tool that was added or enabled after the client session started. Discover server_id, tool_name, and the input schema with mcp_router_list_tools first. The call respects the current access key and tool permissions.",
            json!({
                "type": "object",
                "properties": {
                    "server_id": {
                        "type": "string",
                        "description": "Exact server ID returned by mcp_router_list_tools."
                    },
                    "tool_name": {
                        "type": "string",
                        "description": "Exact downstream tool name returned by mcp_router_list_tools."
                    },
                    "arguments": {
                        "type": "object",
                        "description": "Arguments for the downstream tool, matching its input schema.",
                        "additionalProperties": true
                    }
                },
                "required": ["server_id", "tool_name"],
                "additionalProperties": false
            })
            .as_object()
            .expect("router call tool schema must be an object")
            .clone(),
        ),
    ]
}

fn required_gateway_argument(
    arguments: &Map<String, Value>,
    name: &str,
) -> Result<String, McpError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| McpError::invalid_params(format!("{name} must be a non-empty string"), None))
}

fn parse_gateway_call(
    arguments: Option<&Map<String, Value>>,
) -> Result<(String, CallToolRequestParams), McpError> {
    let arguments = arguments
        .ok_or_else(|| McpError::invalid_params("arguments are required".to_string(), None))?;
    let server_id = required_gateway_argument(arguments, "server_id")?;
    let tool_name = required_gateway_argument(arguments, "tool_name")?;
    if is_router_gateway_tool(&tool_name) {
        return Err(McpError::invalid_params(
            "router gateway tools cannot call themselves".to_string(),
            None,
        ));
    }

    let mut request = CallToolRequestParams::new(tool_name);
    if let Some(value) = arguments.get("arguments") {
        let downstream_arguments = value.as_object().cloned().ok_or_else(|| {
            McpError::invalid_params("arguments must be an object".to_string(), None)
        })?;
        request = request.with_arguments(downstream_arguments);
    }
    Ok((server_id, request))
}

async fn list_router_gateway_tools(
    state: Arc<DesktopState>,
    request: &CallToolRequestParams,
    access: &AccessKeyContext,
) -> Result<CallToolResult, McpError> {
    let requested_server_id = request
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get("server_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let peers = runtime_peers(&state, access)?;
    let permissions = server_tool_permissions(&state)?;
    let mut tools = Vec::new();

    for (server_id, peer) in peers {
        if requested_server_id.is_some_and(|requested| requested != server_id) {
            continue;
        }
        let server_permissions = permissions.get(&server_id);
        let Ok(server_tools) = peer.list_all_tools().await else {
            continue;
        };
        for tool in server_tools {
            let tool_name = tool.name.as_ref();
            let enabled = server_permissions
                .and_then(|permissions| permissions.get(tool_name))
                .copied()
                != Some(false);
            if !enabled || is_router_gateway_tool(tool_name) {
                continue;
            }
            let mut value = serde_json::to_value(tool).unwrap_or_else(|_| json!({}));
            if let Some(object) = value.as_object_mut() {
                object.insert("serverId".to_string(), Value::String(server_id.clone()));
            }
            tools.push(value);
        }
    }

    let structured = json!({ "tools": tools });
    let text = serde_json::to_string_pretty(&structured)
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(structured);
    Ok(result)
}

async fn execute_upstream_tool(
    state: &DesktopState,
    server_id: &str,
    peer: Peer<RoleClient>,
    request: CallToolRequestParams,
) -> Result<CallToolResult, McpError> {
    let params = serde_json::to_value(&request).unwrap_or_else(|_| json!({}));
    let start = now_millis();
    let result = peer.call_tool(request).await;
    let duration = now_millis() - start;
    match result {
        Ok(result) => {
            let response_data = serde_json::to_value(&result).unwrap_or_else(|_| json!({}));
            append_request_log(
                state,
                server_id,
                "CallTool",
                params,
                "success",
                duration,
                Some(response_data),
                None,
            );
            Ok(result)
        }
        Err(error) => {
            let error_msg = error.to_string();
            append_request_log(
                state,
                server_id,
                "CallTool",
                params,
                "error",
                duration,
                None,
                Some(error_msg.clone()),
            );
            Err(McpError::internal_error(error_msg, None))
        }
    }
}

async fn call_router_gateway_tool(
    state: Arc<DesktopState>,
    request: &CallToolRequestParams,
    access: &AccessKeyContext,
) -> Result<CallToolResult, McpError> {
    let (server_id, downstream_request) = parse_gateway_call(request.arguments.as_ref())?;
    let peer = runtime_peer(&state, &server_id, access)?;
    let permissions = server_tool_permissions(&state)?;
    let tool_name = downstream_request.name.as_ref();
    let enabled = permissions
        .get(&server_id)
        .and_then(|permissions| permissions.get(tool_name))
        .copied()
        != Some(false);
    let server_tools = peer
        .list_all_tools()
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    if !enabled
        || !server_tools
            .iter()
            .any(|tool| tool.name.as_ref() == tool_name)
    {
        return Err(McpError::method_not_found::<CallToolRequestMethod>());
    }

    execute_upstream_tool(&state, &server_id, peer, downstream_request).await
}

async fn list_tools_direct(
    state: Arc<DesktopState>,
    access: &AccessKeyContext,
) -> Result<ListToolsResult, McpError> {
    let peers = runtime_peers(&state, access)?;
    let permissions = server_tool_permissions(&state)?;
    let mut tools = router_gateway_tools();

    for (server_id, peer) in peers {
        let server_permissions = permissions.get(&server_id);
        let Ok(server_tools) = peer.list_all_tools().await else {
            continue;
        };
        let mut discovered_tools = Vec::new();
        for tool in server_tools {
            if is_router_gateway_tool(tool.name.as_ref()) {
                continue;
            }
            let enabled = server_permissions
                .and_then(|permissions| permissions.get(tool.name.as_ref()))
                .copied()
                != Some(false);
            if enabled {
                discovered_tools.push(json!({
                    "toolKey": tool.name.as_ref(),
                    "toolName": tool.name.as_ref(),
                    "serverName": server_id.clone(),
                    "relevance": 1
                }));
                tools.push(tool);
            }
        }
        append_request_log(
            &state,
            &server_id,
            "ToolDiscovery",
            json!({}),
            "success",
            0,
            Some(json!({ "tools": discovered_tools })),
            None,
        );
    }

    Ok(ListToolsResult::with_all_items(tools))
}

async fn call_tool_direct(
    state: Arc<DesktopState>,
    request: CallToolRequestParams,
    access: &AccessKeyContext,
) -> Result<CallToolResult, McpError> {
    if request.name.as_ref() == ROUTER_LIST_TOOLS_TOOL {
        return list_router_gateway_tools(state, &request, access).await;
    }
    if request.name.as_ref() == ROUTER_CALL_TOOL_TOOL {
        return call_router_gateway_tool(state, &request, access).await;
    }

    let peers = runtime_peers(&state, access)?;
    let permissions = server_tool_permissions(&state)?;
    let requested_name = request.name.to_string();

    for (server_id, peer) in peers {
        let server_permissions = permissions.get(&server_id);
        let Ok(server_tools) = peer.list_all_tools().await else {
            continue;
        };
        let has_tool = server_tools
            .iter()
            .any(|tool| tool.name.as_ref() == requested_name);
        let enabled = server_permissions
            .and_then(|permissions| permissions.get(requested_name.as_str()))
            .copied()
            != Some(false);
        if has_tool && enabled {
            return execute_upstream_tool(&state, &server_id, peer, request).await;
        }
    }

    Err(McpError::method_not_found::<CallToolRequestMethod>())
}

async fn list_resources_direct(
    state: Arc<DesktopState>,
    access: &AccessKeyContext,
) -> Result<ListResourcesResult, McpError> {
    let peers = runtime_peers(&state, access)?;
    let mut resources = Vec::new();

    for (server_id, peer) in peers {
        let Ok(server_resources) = peer.list_all_resources().await else {
            continue;
        };
        for mut resource in server_resources {
            resource.uri = route_resource_uri(&server_id, &resource.uri);
            resources.push(resource);
        }
    }

    Ok(ListResourcesResult::with_all_items(resources))
}

async fn list_resource_templates_direct(
    state: Arc<DesktopState>,
    access: &AccessKeyContext,
) -> Result<ListResourceTemplatesResult, McpError> {
    let peers = runtime_peers(&state, access)?;
    let mut resource_templates = Vec::new();

    for (server_id, peer) in peers {
        let Ok(server_templates) = peer.list_all_resource_templates().await else {
            continue;
        };
        for mut template in server_templates {
            template.uri_template = route_resource_uri(&server_id, &template.uri_template);
            resource_templates.push(template);
        }
    }

    Ok(ListResourceTemplatesResult::with_all_items(
        resource_templates,
    ))
}

async fn read_resource_direct(
    state: Arc<DesktopState>,
    request: ReadResourceRequestParams,
    access: &AccessKeyContext,
) -> Result<ReadResourceResult, McpError> {
    let Some((server_id, original_uri)) = split_routed_resource_uri(&request.uri) else {
        return Err(McpError::method_not_found::<ReadResourceRequestMethod>());
    };
    let peer = runtime_peer(&state, &server_id, access)?;
    let mut routed_request = ReadResourceRequestParams::new(original_uri.clone());
    if let Some(meta) = request.meta {
        routed_request = routed_request.with_meta(meta);
    }
    let start = now_millis();
    let result = peer.read_resource(routed_request).await;
    let duration = now_millis() - start;
    match result {
        Ok(result) => {
            let response_data = serde_json::to_value(&result).unwrap_or_else(|_| json!({}));
            append_request_log(
                &state,
                &server_id,
                "ReadResource",
                json!({ "uri": original_uri }),
                "success",
                duration,
                Some(response_data),
                None,
            );
            Ok(result)
        }
        Err(error) => {
            let error_msg = error.to_string();
            append_request_log(
                &state,
                &server_id,
                "ReadResource",
                json!({ "uri": original_uri }),
                "error",
                duration,
                None,
                Some(error_msg.clone()),
            );
            Err(McpError::internal_error(error_msg, None))
        }
    }
}

async fn list_prompts_direct(
    state: Arc<DesktopState>,
    access: &AccessKeyContext,
) -> Result<ListPromptsResult, McpError> {
    let peers = runtime_peers(&state, access)?;
    let mut prompts = Vec::new();

    for (_server_id, peer) in peers {
        let Ok(server_prompts) = peer.list_all_prompts().await else {
            continue;
        };
        prompts.extend(server_prompts);
    }

    Ok(ListPromptsResult::with_all_items(prompts))
}

async fn get_prompt_direct(
    state: Arc<DesktopState>,
    request: GetPromptRequestParams,
    access: &AccessKeyContext,
) -> Result<GetPromptResult, McpError> {
    let peers = runtime_peers(&state, access)?;
    let params = serde_json::to_value(&request).unwrap_or_else(|_| json!({}));
    for (server_id, peer) in peers {
        let Ok(server_prompts) = peer.list_all_prompts().await else {
            continue;
        };
        if server_prompts
            .iter()
            .any(|prompt| prompt.name == request.name)
        {
            let start = now_millis();
            let result = peer.get_prompt(request).await;
            let duration = now_millis() - start;
            return match result {
                Ok(result) => {
                    let response_data = serde_json::to_value(&result).unwrap_or_else(|_| json!({}));
                    append_request_log(
                        &state,
                        &server_id,
                        "GetPrompt",
                        params,
                        "success",
                        duration,
                        Some(response_data),
                        None,
                    );
                    Ok(result)
                }
                Err(error) => {
                    let error_msg = error.to_string();
                    append_request_log(
                        &state,
                        &server_id,
                        "GetPrompt",
                        params,
                        "error",
                        duration,
                        None,
                        Some(error_msg.clone()),
                    );
                    Err(McpError::internal_error(error_msg, None))
                }
            };
        }
    }

    Err(McpError::method_not_found::<GetPromptRequestMethod>())
}

async fn list_resources_with_workflows(
    state: Arc<DesktopState>,
    access: AccessKeyContext,
) -> Result<ListResourcesResult, McpError> {
    let handler_state = state.clone();
    let handler_access = access.clone();
    let handler = move || -> McpValueFuture<'static> {
        let state = handler_state.clone();
        let access = handler_access.clone();
        Box::pin(async move {
            let result = list_resources_direct(state, &access)
                .await
                .map_err(|error| format!("{error:?}"))?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        })
    };

    match execute_mcp_workflow(&state, "resources/list", json!({}), &handler).await {
        Ok(Some(value)) => serde_json::from_value(value)
            .map_err(|error| McpError::internal_error(error.to_string(), None)),
        Ok(None) => list_resources_direct(state, &access).await,
        Err(error) => {
            eprintln!(
                "Workflow handling failed for resources/list; falling back to direct handler: {error}"
            );
            list_resources_direct(state, &access).await
        }
    }
}

async fn read_resource_with_workflows(
    state: Arc<DesktopState>,
    request: ReadResourceRequestParams,
    access: AccessKeyContext,
) -> Result<ReadResourceResult, McpError> {
    let params = serde_json::to_value(&request).unwrap_or_else(|_| json!({}));
    let handler_state = state.clone();
    let handler_request = request.clone();
    let handler_access = access.clone();
    let handler = move || -> McpValueFuture<'static> {
        let state = handler_state.clone();
        let request = handler_request.clone();
        let access = handler_access.clone();
        Box::pin(async move {
            let result = read_resource_direct(state, request, &access)
                .await
                .map_err(|error| format!("{error:?}"))?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        })
    };

    match execute_mcp_workflow(&state, "resources/read", params, &handler).await {
        Ok(Some(value)) => serde_json::from_value(value)
            .map_err(|error| McpError::internal_error(error.to_string(), None)),
        Ok(None) => read_resource_direct(state, request, &access).await,
        Err(error) => {
            eprintln!(
                "Workflow handling failed for resources/read; falling back to direct handler: {error}"
            );
            read_resource_direct(state, request, &access).await
        }
    }
}

async fn list_prompts_with_workflows(
    state: Arc<DesktopState>,
    access: AccessKeyContext,
) -> Result<ListPromptsResult, McpError> {
    let handler_state = state.clone();
    let handler_access = access.clone();
    let handler = move || -> McpValueFuture<'static> {
        let state = handler_state.clone();
        let access = handler_access.clone();
        Box::pin(async move {
            let result = list_prompts_direct(state, &access)
                .await
                .map_err(|error| format!("{error:?}"))?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        })
    };

    match execute_mcp_workflow(&state, "prompts/list", json!({}), &handler).await {
        Ok(Some(value)) => serde_json::from_value(value)
            .map_err(|error| McpError::internal_error(error.to_string(), None)),
        Ok(None) => list_prompts_direct(state, &access).await,
        Err(error) => {
            eprintln!(
                "Workflow handling failed for prompts/list; falling back to direct handler: {error}"
            );
            list_prompts_direct(state, &access).await
        }
    }
}

async fn get_prompt_with_workflows(
    state: Arc<DesktopState>,
    request: GetPromptRequestParams,
    access: AccessKeyContext,
) -> Result<GetPromptResult, McpError> {
    let params = serde_json::to_value(&request).unwrap_or_else(|_| json!({}));
    let handler_state = state.clone();
    let handler_request = request.clone();
    let handler_access = access.clone();
    let handler = move || -> McpValueFuture<'static> {
        let state = handler_state.clone();
        let request = handler_request.clone();
        let access = handler_access.clone();
        Box::pin(async move {
            let result = get_prompt_direct(state, request, &access)
                .await
                .map_err(|error| format!("{error:?}"))?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        })
    };

    match execute_mcp_workflow(&state, "prompts/get", params, &handler).await {
        Ok(Some(value)) => serde_json::from_value(value)
            .map_err(|error| McpError::internal_error(error.to_string(), None)),
        Ok(None) => get_prompt_direct(state, request, &access).await,
        Err(error) => {
            eprintln!(
                "Workflow handling failed for prompts/get; falling back to direct handler: {error}"
            );
            get_prompt_direct(state, request, &access).await
        }
    }
}

async fn list_tools_with_workflows(
    state: Arc<DesktopState>,
    access: AccessKeyContext,
) -> Result<ListToolsResult, McpError> {
    let handler_state = state.clone();
    let handler_access = access.clone();
    let handler = move || -> McpValueFuture<'static> {
        let state = handler_state.clone();
        let access = handler_access.clone();
        Box::pin(async move {
            let result = list_tools_direct(state, &access)
                .await
                .map_err(|error| format!("{error:?}"))?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        })
    };

    match execute_mcp_workflow(&state, "tools/list", json!({}), &handler).await {
        Ok(Some(value)) => serde_json::from_value(value)
            .map_err(|error| McpError::internal_error(error.to_string(), None)),
        Ok(None) => list_tools_direct(state, &access).await,
        Err(error) => {
            eprintln!(
                "Workflow handling failed for tools/list; falling back to direct handler: {error}"
            );
            list_tools_direct(state, &access).await
        }
    }
}

async fn call_tool_with_workflows(
    state: Arc<DesktopState>,
    request: CallToolRequestParams,
    access: AccessKeyContext,
) -> Result<CallToolResult, McpError> {
    let params = serde_json::to_value(&request).unwrap_or_else(|_| json!({}));
    let handler_state = state.clone();
    let handler_request = request.clone();
    let handler_access = access.clone();
    let handler = move || -> McpValueFuture<'static> {
        let state = handler_state.clone();
        let request = handler_request.clone();
        let access = handler_access.clone();
        Box::pin(async move {
            let result = call_tool_direct(state, request, &access)
                .await
                .map_err(|error| format!("{error:?}"))?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        })
    };

    match execute_mcp_workflow(&state, "tools/call", params, &handler).await {
        Ok(Some(value)) => serde_json::from_value(value)
            .map_err(|error| McpError::internal_error(error.to_string(), None)),
        Ok(None) => call_tool_direct(state, request, &access).await,
        Err(error) => {
            eprintln!(
                "Workflow handling failed for tools/call; falling back to direct handler: {error}"
            );
            call_tool_direct(state, request, &access).await
        }
    }
}

async fn execute_mcp_workflow(
    state: &DesktopState,
    method: &str,
    params: Value,
    handler: &McpValueHandler<'_>,
) -> Result<Option<Value>, String> {
    let (workflows, hooks) = {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        let workflows = store
            .workflows
            .iter()
            .filter(|workflow| {
                workflow.get("enabled").and_then(Value::as_bool) == Some(true)
                    && workflow.get("workflowType").and_then(Value::as_str) == Some(method)
            })
            .cloned()
            .filter(is_valid_workflow)
            .collect::<Vec<_>>();
        if workflows.is_empty() {
            return Ok(None);
        }
        (workflows, store.hooks.clone())
    };

    let context = json!({
        "method": method,
        "params": params,
        "clientId": "mcp-client",
        "timestamp": now_millis()
    });

    if workflows.len() == 1 {
        let result = execute_workflow_value(&workflows[0], &hooks, context, Some(handler)).await;
        return Ok(result
            .get("mcpResult")
            .filter(|value| !value.is_null())
            .cloned());
    }

    let result = execute_hook_workflow_chain(&workflows, &hooks, context, handler).await?;

    Ok(Some(result))
}

async fn execute_hook_workflow_chain(
    workflows: &[Value],
    hooks: &[Value],
    context: Value,
    handler: &McpValueHandler<'_>,
) -> Result<Value, String> {
    let mut before_hooks = Vec::<(&Value, &Value)>::new();
    let mut after_hooks = Vec::<(&Value, &Value)>::new();

    for workflow in workflows {
        let order = determine_workflow_execution_order(workflow)?;
        let Some(mcp_index) = order.iter().position(|node_id| {
            workflow_node_by_id(workflow, node_id)
                .and_then(|node| node.get("type").and_then(Value::as_str))
                == Some("mcp-call")
        }) else {
            continue;
        };

        for (index, node_id) in order.iter().enumerate() {
            let Some(node) = workflow_node_by_id(workflow, node_id) else {
                continue;
            };
            if node.get("type").and_then(Value::as_str) != Some("hook") {
                continue;
            }
            if index < mcp_index {
                before_hooks.push((workflow, node));
            } else {
                after_hooks.push((workflow, node));
            }
        }
    }

    let mut previous_results = Map::new();
    for (workflow, node) in before_hooks {
        execute_chain_hook(workflow, hooks, node, &context, &mut previous_results);
    }

    let mcp_result = handler().await?;
    previous_results.insert(
        format!(
            "mcp:{method}",
            method = context
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ),
        json!({
            "type": "mcp-call",
            "success": true,
            "mcpResponse": mcp_result.clone(),
            "timestamp": now_millis()
        }),
    );

    let after_context = merge_context_value(
        context,
        json!({
            "mcpResult": mcp_result.clone()
        }),
    );

    for (workflow, node) in after_hooks {
        execute_chain_hook(workflow, hooks, node, &after_context, &mut previous_results);
    }

    Ok(mcp_result)
}

fn execute_chain_hook(
    workflow: &Value,
    hooks: &[Value],
    node: &Value,
    context: &Value,
    previous_results: &mut Map<String, Value>,
) {
    let result = execute_workflow_hook_node(workflow, hooks, node, context, previous_results);
    let workflow_id = value_id(workflow).unwrap_or("workflow");
    let node_id = value_id(node).unwrap_or("hook");
    previous_results.insert(format!("{workflow_id}:{node_id}"), result);
}

fn workflow_node_by_id<'a>(workflow: &'a Value, node_id: &str) -> Option<&'a Value> {
    workflow
        .get("nodes")
        .and_then(Value::as_array)?
        .iter()
        .find(|node| value_id(node) == Some(node_id))
}

fn merge_context_value(mut base: Value, patch: Value) -> Value {
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            base.insert(key.clone(), value.clone());
        }
    }
    base
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
