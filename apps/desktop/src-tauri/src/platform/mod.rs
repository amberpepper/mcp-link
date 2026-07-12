#[cfg(feature = "desktop")]
pub mod dialogs;
pub mod entities;
pub mod hooks;
pub mod misc;
pub mod network;
pub mod registry;
pub mod skills;
pub mod workflows;

use serde_json::{json, Value};
#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri_plugin_autostart::ManagerExt;

use crate::access_keys::{
    generate_access_key, list_access_keys, revoke_access_key, update_access_key_server_access,
};
use crate::hook::runtime::validate_hook_script;
use crate::mcp::bundles::{import_dxt_server, remove_installed_bundle};
use crate::mcp::external_configs::import_external_mcp_configs;
#[cfg(feature = "desktop")]
use crate::mcp::server::start_desktop_mcp_http_server;
use crate::mcp::server::{notify_mcp_clients_all_lists_changed, notify_mcp_clients_tools_changed};
use crate::mcp::servers::{
    disconnect_mcp_runtime, list_mcp_server_tools, list_servers, start_mcp_server, stop_mcp_server,
    validate_server,
};
#[cfg(feature = "desktop")]
use crate::platform::dialogs::{import_skill, open_skill_folder, select_path};
use crate::platform::{
    entities::{delete_entity, get_entity, update_entity_with},
    hooks::{create_hook, delete_hook, execute_hook_module_platform, update_hook},
    misc::query_logs,
    registry::fetch_registry_servers,
    skills::{
        create_skill_files, delete_skill_files, reconfigure_skill_targets, update_skill_files,
    },
    workflows::{create_workflow, execute_workflow_platform, set_active_workflow, update_workflow},
};
use crate::state::{find_server_mut, normalize_server, save_store, DesktopState, StoreState};
use crate::util::{
    json::{merge_value_object, required_string, set_object_field, set_object_value, value_id},
    time::{now_iso, now_millis},
};

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn platform_call(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DesktopState>>,
    method: String,
    args: Vec<Value>,
) -> Result<Value, String> {
    if method == "serverSelectFile" {
        return select_path(app, args.first()).await;
    }

    if method == "openSkillFolder" {
        return open_skill_folder(app, state, args).await;
    }

    if method == "importSkill" {
        return import_skill(app, state).await;
    }

    if method == "restartDesktopMcpEndpoint" {
        start_desktop_mcp_http_server(state.inner().clone());
        return Ok(Value::Bool(true));
    }

    let requested_autostart = (method == "saveSettings")
        .then(|| {
            args.first()
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("showWindowOnStartup"))
                .and_then(Value::as_bool)
        })
        .flatten();
    let result = dispatch_platform_method(state.inner().as_ref(), &method, args).await?;
    if let Some(enabled) = requested_autostart {
        let autolaunch = app.autolaunch();
        if enabled {
            autolaunch.enable().map_err(|error| error.to_string())?;
        } else {
            autolaunch.disable().map_err(|error| error.to_string())?;
        }
    }
    Ok(result)
}

pub(crate) async fn dispatch_platform_method(
    state: &DesktopState,
    method: &str,
    args: Vec<Value>,
) -> Result<Value, String> {
    if method == "addMcpServer"
        && args
            .first()
            .and_then(|input| input.get("type"))
            .and_then(Value::as_str)
            == Some("dxt")
    {
        return import_dxt_server(
            state,
            args.first()
                .ok_or_else(|| "DXT import payload is required".to_string())?,
        );
    }

    if method == "createSkill" {
        return create_skill_files(state, args.first(), None);
    }
    if method == "updateSkill" {
        return update_skill_files(state, &args);
    }
    if method == "deleteSkill" {
        return delete_skill_files(state, &args);
    }

    if method == "discoverRegistryServers" {
        return fetch_registry_servers(args.first()).await;
    }

    if method == "proxyFetch" {
        let url = required_string(&args, 0)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(concat!("mcp-link/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "HTTP {}: {}",
                status.as_u16(),
                &body[..body.len().min(200)]
            ));
        }
        let json: Value =
            serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))?;
        return Ok(json);
    }

    if method == "proxyFetchText" {
        let url = required_string(&args, 0)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(concat!("mcp-link/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Ok(Value::Null);
        }
        let body = resp.text().await.map_err(|e| e.to_string())?;
        return Ok(Value::String(body));
    }

    if method == "listNetworkInterfaces" {
        return network::list_network_interfaces();
    }

    if method == "startMcpServer" {
        return start_mcp_server(state, required_string(&args, 0)?).await;
    }

    if method == "stopMcpServer" {
        return stop_mcp_server(state, required_string(&args, 0)?).await;
    }

    if method == "listMcpServerTools" {
        return list_mcp_server_tools(state, required_string(&args, 0)?).await;
    }

    if method == "executeWorkflow" {
        return execute_workflow_platform(state, &args).await;
    }

    if method == "executeHookModule" {
        return execute_hook_module_platform(state, &args);
    }

    if method == "validateHookScript" {
        let script = args.first().and_then(Value::as_str).unwrap_or_default();
        return Ok(validate_hook_script(script));
    }

    if method == "listAccessKeys" {
        let db_path = state.access_keys_db_path();
        let keys = list_access_keys(&db_path)?;
        return serde_json::to_value(keys).map_err(|error| error.to_string());
    }

    if method == "generateAccessKey" {
        let db_path = state.access_keys_db_path();
        return Ok(Value::String(generate_access_key(&db_path, args.first())?));
    }

    if method == "revokeAccessKey" {
        let db_path = state.access_keys_db_path();
        revoke_access_key(&db_path, &required_string(&args, 0)?)?;
        return Ok(Value::Bool(true));
    }

    if method == "updateAccessKeyServerAccess" {
        let db_path = state.access_keys_db_path();
        let id = required_string(&args, 0)?;
        let server_access = args.get(1).cloned().unwrap_or_else(|| json!({}));
        let key = update_access_key_server_access(&db_path, &id, &server_access)?;
        notify_mcp_clients_all_lists_changed(state).await;
        return serde_json::to_value(key).map_err(|error| error.to_string());
    }

    let removed_server = if method == "removeMcpServer" {
        args.first().and_then(Value::as_str).and_then(|id| {
            state.store.lock().ok().and_then(|store| {
                store
                    .servers
                    .iter()
                    .find(|server| value_id(server) == Some(id))
                    .cloned()
            })
        })
    } else {
        None
    };
    let previous_skill_paths = (method == "saveSettings"
        && args
            .first()
            .and_then(Value::as_object)
            .is_some_and(|settings| settings.contains_key("skillAgentPaths")))
    .then(|| {
        state
            .store
            .lock()
            .ok()
            .and_then(|store| store.settings.get("skillAgentPaths").cloned())
            .unwrap_or(Value::Null)
    });

    let result = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;

        let result = handle_platform_method(&mut store, &method, &args)?;

        if mutates_state(&method) {
            save_store(&state.store_path, &store)?;
        }

        result
    };

    notify_after_platform_mutation(state, method, &args).await;

    if let Some(server) = removed_server.as_ref() {
        remove_installed_bundle(state, server)?;
    }
    if let Some(previous) = previous_skill_paths.as_ref() {
        let current = args
            .first()
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("skillAgentPaths"));
        if current != Some(previous) {
            reconfigure_skill_targets(state, Some(previous));
        }
    }

    if method == "saveSettings"
        && args
            .first()
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("loadExternalMCPConfigs"))
            .and_then(Value::as_bool)
            == Some(true)
    {
        import_external_mcp_configs(state)?;
    }

    Ok(result)
}

async fn notify_after_platform_mutation(state: &DesktopState, method: &str, args: &[Value]) {
    match method {
        "removeMcpServer" | "updateMcpServerConfig" => {
            let Some(id) = args.first().and_then(Value::as_str) else {
                return;
            };
            match disconnect_mcp_runtime(state, id).await {
                Ok(true) => notify_mcp_clients_all_lists_changed(state).await,
                Ok(false) => {}
                Err(error) => eprintln!("Failed to disconnect MCP runtime {id}: {error}"),
            }
        }
        "updateToolPermissions" => {
            notify_mcp_clients_tools_changed(state).await;
        }
        "createWorkflow" | "updateWorkflow" | "deleteWorkflow" | "setActiveWorkflow"
        | "disableWorkflow" => {
            notify_mcp_clients_all_lists_changed(state).await;
        }
        _ => {}
    }
}

fn handle_platform_method(
    store: &mut StoreState,
    method: &str,
    args: &[Value],
) -> Result<Value, String> {
    match method {
        "listMcpServers" => Ok(Value::Array(list_servers(store))),
        "addMcpServer" => {
            let input = args.first().cloned().unwrap_or_else(|| json!({}));
            let config = input
                .get("config")
                .cloned()
                .filter(|_| input.get("type").and_then(Value::as_str) == Some("config"))
                .unwrap_or(input);
            let server = normalize_server(&config);
            validate_server(&server)?;
            store.servers.push(server.clone());
            Ok(server)
        }
        "removeMcpServer" => {
            let id = required_string(args, 0)?;
            store
                .servers
                .retain(|server| value_id(server) != Some(id.as_str()));
            Ok(Value::Bool(true))
        }
        "updateMcpServerConfig" => {
            let id = required_string(args, 0)?;
            let updates = args.get(1).cloned().unwrap_or_else(|| json!({}));
            let server = find_server_mut(store, &id)?;
            merge_value_object(server, updates);
            set_object_field(server, "updatedAt", now_iso());
            set_object_field(server, "status", "stopped");
            validate_server(server)?;
            Ok(server.clone())
        }
        "updateToolPermissions" => {
            let id = required_string(args, 0)?;
            let permissions = args.get(1).cloned().unwrap_or_else(|| json!({}));
            let server = find_server_mut(store, &id)?;
            set_object_value(server, "toolPermissions", permissions);
            set_object_field(server, "updatedAt", now_iso());
            Ok(server.clone())
        }
        "getSettings" => Ok(Value::Object(store.settings.clone())),
        "saveSettings" => {
            if let Some(settings) = args.first().and_then(Value::as_object) {
                for (key, value) in settings {
                    store.settings.insert(key.clone(), value.clone());
                }
            }
            Ok(Value::Bool(true))
        }
        "getRequestLogs" => Ok(query_logs(store, args.first())),
        "listWorkflows" => Ok(Value::Array(store.workflows.clone())),
        "getWorkflow" => get_entity(&store.workflows, args),
        "createWorkflow" => create_workflow(store, args.first()),
        "updateWorkflow" => update_workflow(store, args),
        "deleteWorkflow" => delete_entity(&mut store.workflows, args),
        "setActiveWorkflow" => set_active_workflow(store, args),
        "disableWorkflow" => update_entity_with(
            &mut store.workflows,
            args,
            json!({ "enabled": false, "updatedAt": now_millis() }),
        ),
        "getEnabledWorkflows" => Ok(Value::Array(
            store
                .workflows
                .iter()
                .filter(|workflow| workflow.get("enabled").and_then(Value::as_bool) == Some(true))
                .cloned()
                .collect(),
        )),
        "getWorkflowsByType" => {
            let workflow_type = required_string(args, 0)?;
            Ok(Value::Array(
                store
                    .workflows
                    .iter()
                    .filter(|workflow| {
                        workflow.get("workflowType").and_then(Value::as_str)
                            == Some(workflow_type.as_str())
                    })
                    .cloned()
                    .collect(),
            ))
        }
        "listHookModules" => Ok(Value::Array(store.hooks.clone())),
        "getHookModule" => get_entity(&store.hooks, args),
        "createHookModule" | "importHookModule" => create_hook(store, args.first()),
        "updateHookModule" => update_hook(store, args),
        "deleteHookModule" => delete_hook(store, args),
        "listSkills" => Ok(Value::Array(store.skills.clone())),
        _ => Err(format!("Unsupported platform API method: {method}")),
    }
}

fn mutates_state(method: &str) -> bool {
    !matches!(
        method,
        "listMcpServers"
            | "listMcpServerTools"
            | "listNetworkInterfaces"
            | "getSettings"
            | "getRequestLogs"
            | "listWorkflows"
            | "getWorkflow"
            | "getEnabledWorkflows"
            | "getWorkflowsByType"
            | "listHookModules"
            | "getHookModule"
            | "validateHookScript"
            | "listSkills"
    )
}
