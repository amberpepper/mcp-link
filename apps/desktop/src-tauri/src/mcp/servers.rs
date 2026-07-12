use serde_json::{json, Value};
use std::{future::Future, time::Duration};

use crate::{
    mcp::client::connect_runtime,
    mcp::server::notify_mcp_clients_all_lists_changed,
    state::{find_server_mut, normalize_server, save_store, DesktopState, StoreState},
    util::{
        json::{merge_object, set_object_field, set_object_value, string_array},
        time::now_iso,
    },
};

const DEFAULT_MCP_LIST_CAPABILITIES_TIMEOUT: Duration = Duration::from_secs(15);

fn auto_start_server_ids(store: &StoreState) -> Vec<String> {
    store
        .servers
        .iter()
        .filter(|server| {
            server.get("autoStart").and_then(Value::as_bool) == Some(true)
                && server.get("disabled").and_then(Value::as_bool) != Some(true)
        })
        .filter_map(|server| server.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

pub(crate) async fn start_auto_start_servers(state: std::sync::Arc<DesktopState>) {
    let server_ids = state
        .store
        .lock()
        .map(|store| auto_start_server_ids(&store))
        .unwrap_or_default();

    futures::future::join_all(server_ids.into_iter().map(|id| {
        let state = state.clone();
        async move {
            if let Err(error) = start_mcp_server(&state, id.clone()).await {
                eprintln!("Failed to auto-start MCP server {id}: {error}");
            }
        }
    }))
    .await;
}

pub(crate) async fn start_mcp_server(state: &DesktopState, id: String) -> Result<Value, String> {
    {
        let runtimes = state
            .runtimes
            .lock()
            .map_err(|_| "Failed to lock desktop runtimes".to_string())?;
        if let Some(runtime) = runtimes.get(&id) {
            let mut store = state
                .store
                .lock()
                .map_err(|_| "Failed to lock desktop state".to_string())?;
            let server = find_server_mut(&mut store, &id)?;
            set_object_field(server, "status", "running");
            set_object_value(
                server,
                "pid",
                runtime.pid.map_or(Value::Null, |pid| json!(pid)),
            );
            return Ok(server.clone());
        }
    }

    let server_config = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        let server = find_server_mut(&mut store, &id)?;
        if server
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(format!(
                "{} is disabled",
                server
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("MCP Server")
            ));
        }
        set_object_field(server, "status", "starting");
        set_object_value(server, "errorMessage", Value::Null);
        append_log(server, &format!("Starting {}", describe_server(server)));
        let server_config = server.clone();
        save_store(&state.store_path, &store)?;
        server_config
    };

    match connect_runtime(&server_config).await {
        Ok(runtime) => {
            let peer = runtime.client.peer().clone();
            let mut capability_warnings = Vec::new();
            let capability_timeout = capability_list_timeout(&server_config);
            let tools = match timeout_capability_list(
                "tools/list",
                peer.list_all_tools(),
                capability_timeout,
            )
            .await
            {
                Ok(tools) => tools_to_json(tools),
                Err(error) => {
                    capability_warnings.push(error);
                    json!([])
                }
            };
            let prompts = match timeout_capability_list(
                "prompts/list",
                peer.list_all_prompts(),
                capability_timeout,
            )
            .await
            {
                Ok(prompts) => serde_json::to_value(prompts).unwrap_or_else(|_| json!([])),
                Err(error) => {
                    capability_warnings.push(error);
                    json!([])
                }
            };
            let resources = match timeout_capability_list(
                "resources/list",
                peer.list_all_resources(),
                capability_timeout,
            )
            .await
            {
                Ok(resources) => serde_json::to_value(resources).unwrap_or_else(|_| json!([])),
                Err(error) => {
                    capability_warnings.push(error);
                    json!([])
                }
            };
            let pid = runtime.pid;

            {
                let mut runtimes = state
                    .runtimes
                    .lock()
                    .map_err(|_| "Failed to lock desktop runtimes".to_string())?;
                runtimes.insert(id.clone(), runtime);
            }

            let server = {
                let mut store = state
                    .store
                    .lock()
                    .map_err(|_| "Failed to lock desktop state".to_string())?;
                let server = find_server_mut(&mut store, &id)?;
                set_object_field(server, "status", "running");
                set_object_value(server, "pid", pid.map_or(Value::Null, |pid| json!(pid)));
                set_object_value(server, "tools", tools);
                set_object_value(server, "prompts", prompts);
                set_object_value(server, "resources", resources);
                set_object_field(server, "updatedAt", now_iso());
                append_log(
                    server,
                    &format!(
                        "MCP initialized{}",
                        pid.map_or(String::new(), |pid| format!(" with PID {pid}"))
                    ),
                );
                for warning in capability_warnings {
                    append_log(server, &format!("Capability discovery warning: {warning}"));
                }
                let server = server.clone();
                save_store(&state.store_path, &store)?;
                server
            };
            notify_mcp_clients_all_lists_changed(state).await;
            Ok(server)
        }
        Err(error) => {
            let mut store = state
                .store
                .lock()
                .map_err(|_| "Failed to lock desktop state".to_string())?;
            let server = find_server_mut(&mut store, &id)?;
            set_object_field(server, "status", "error");
            set_object_value(server, "pid", Value::Null);
            set_object_value(server, "errorMessage", Value::String(error.clone()));
            append_log(server, &format!("Failed to start: {error}"));
            save_store(&state.store_path, &store)?;
            Err(error)
        }
    }
}

async fn timeout_capability_list<T, E>(
    method: &str,
    future: impl Future<Output = Result<T, E>>,
    timeout: Duration,
) -> Result<T, String>
where
    E: std::fmt::Display,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err(format!(
            "{method} timed out after {} seconds",
            timeout.as_secs()
        )),
    }
}

fn capability_list_timeout(server: &Value) -> Duration {
    server
        .get("capabilityTimeoutSec")
        .and_then(Value::as_u64)
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_MCP_LIST_CAPABILITIES_TIMEOUT)
}

pub(crate) async fn disconnect_mcp_runtime(state: &DesktopState, id: &str) -> Result<bool, String> {
    let runtime = {
        let mut runtimes = state
            .runtimes
            .lock()
            .map_err(|_| "Failed to lock desktop runtimes".to_string())?;
        runtimes.remove(id)
    };

    let Some(mut runtime) = runtime else {
        return Ok(false);
    };

    let _ = runtime
        .client
        .close_with_timeout(std::time::Duration::from_secs(5))
        .await;

    Ok(true)
}

pub(crate) async fn stop_mcp_server(state: &DesktopState, id: String) -> Result<Value, String> {
    let disconnected = disconnect_mcp_runtime(state, &id).await?;

    let server = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        let server = find_server_mut(&mut store, &id)?;
        set_object_field(server, "status", "stopped");
        set_object_value(server, "pid", Value::Null);
        append_log(server, "Stopped");
        let server = server.clone();
        save_store(&state.store_path, &store)?;
        server
    };
    if disconnected {
        notify_mcp_clients_all_lists_changed(state).await;
    }
    Ok(server)
}

pub(crate) async fn list_mcp_server_tools(
    state: &DesktopState,
    id: String,
) -> Result<Value, String> {
    let peer = {
        let runtimes = state
            .runtimes
            .lock()
            .map_err(|_| "Failed to lock desktop runtimes".to_string())?;
        runtimes
            .get(&id)
            .map(|runtime| runtime.client.peer().clone())
    };

    if let Some(peer) = peer {
        let tools = peer
            .list_all_tools()
            .await
            .map_err(|error| error.to_string())?;
        let tools = tools_to_json(tools);

        let mut store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        let server = find_server_mut(&mut store, &id)?;
        set_object_value(server, "tools", tools.clone());
        let tools = apply_tool_permissions(server, tools);
        save_store(&state.store_path, &store)?;
        return Ok(tools);
    }

    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    let server = find_server_mut(&mut store, &id)?;
    let tools = server.get("tools").cloned().unwrap_or_else(|| json!([]));
    Ok(apply_tool_permissions(server, tools))
}

pub(crate) fn list_servers(store: &mut StoreState) -> Vec<Value> {
    store.servers = store.servers.iter().map(normalize_server).collect();
    store.servers.clone()
}

pub(crate) fn validate_server(server: &Value) -> Result<(), String> {
    let name = server
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name.trim().is_empty() {
        return Err("Server name is required".to_string());
    }

    let server_type = server
        .get("serverType")
        .and_then(Value::as_str)
        .unwrap_or("local");
    if server_type == "local"
        && server
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err("Command is required for local MCP servers".to_string());
    }
    if server_type != "local"
        && server
            .get("remoteUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err("Remote URL is required for remote MCP servers".to_string());
    }
    Ok(())
}

fn append_log(server: &mut Value, message: &str) {
    let Some(object) = server.as_object_mut() else {
        return;
    };
    let logs = object.entry("logs").or_insert_with(|| Value::Array(vec![]));
    let Some(logs) = logs.as_array_mut() else {
        return;
    };
    for line in message
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
    {
        logs.push(Value::String(format!("[{}] {line}", now_iso())));
    }
    if logs.len() > 300 {
        let drain_count = logs.len() - 300;
        logs.drain(0..drain_count);
    }
}

fn describe_server(server: &Value) -> String {
    if server
        .get("serverType")
        .and_then(Value::as_str)
        .unwrap_or("local")
        == "local"
    {
        let command = server
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = string_array(server.get("args")).join(" ");
        return format!("{command} {args}").trim().to_string();
    }

    format!(
        "{} {}",
        server
            .get("serverType")
            .and_then(Value::as_str)
            .unwrap_or("remote"),
        server
            .get("remoteUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
    )
}

fn tools_to_json(tools: Vec<rmcp::model::Tool>) -> Value {
    serde_json::to_value(tools).unwrap_or_else(|_| json!([]))
}

fn apply_tool_permissions(server: &Value, tools: Value) -> Value {
    let permissions = server
        .get("toolPermissions")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let tools = tools
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|tool| {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            merge_object(
                tool,
                json!({ "enabled": permissions.get(&name).and_then(Value::as_bool) != Some(false) }),
            )
        })
        .collect();
    Value::Array(tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::create_default_state;

    #[test]
    fn auto_start_ids_include_enabled_servers_only() {
        let mut store = create_default_state();
        store.servers = vec![
            json!({ "id": "enabled", "autoStart": true, "disabled": false }),
            json!({ "id": "manual", "autoStart": false, "disabled": false }),
            json!({ "id": "disabled", "autoStart": true, "disabled": true }),
        ];

        assert_eq!(auto_start_server_ids(&store), vec!["enabled"]);
    }
}
