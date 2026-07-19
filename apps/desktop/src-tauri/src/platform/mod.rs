pub mod agents;
#[cfg(feature = "desktop")]
pub mod dialogs;
pub mod entities;
pub mod hooks;
pub mod methods;
pub mod misc;
pub mod network;
pub mod registry;
pub mod skills;
pub mod workflows;

#[cfg(feature = "desktop")]
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::Manager;
#[cfg(feature = "desktop")]
use tauri_plugin_autostart::ManagerExt;
#[cfg(feature = "desktop")]
use tauri_plugin_dialog::DialogExt;

use crate::access_keys::{
    generate_access_key, list_access_keys, revoke_access_key, update_access_key_server_access,
};
use crate::gateway::config::{
    create_provider as create_gateway_provider, create_route as create_gateway_route,
    fetch_provider_models as fetch_gateway_provider_models, gateway_settings,
    list_providers as list_gateway_providers, list_routes as list_gateway_routes,
    regenerate_access_key as regenerate_gateway_access_key,
    remove_provider as remove_gateway_provider, remove_route as remove_gateway_route,
    set_active_provider as set_active_gateway_provider, update_gateway_settings,
    update_provider as update_gateway_provider, update_route as update_gateway_route,
};
use crate::gateway::logs::{clear_call_logs as clear_gateway_call_logs, list_call_logs};
use crate::hook::runtime::validate_hook_script;
use crate::mcp::bundles::{import_dxt_server, remove_installed_bundle};
#[cfg(feature = "desktop")]
use crate::mcp::server::restart_desktop_mcp_http_server;
use crate::mcp::server::{
    current_mcp_endpoint, mcp_endpoint_status, notify_mcp_clients_all_lists_changed,
    notify_mcp_clients_tools_changed,
};
use crate::mcp::servers::{
    disconnect_mcp_runtime, list_mcp_server_tools, list_servers, start_mcp_server, stop_mcp_server,
    validate_server,
};
#[cfg(feature = "desktop")]
use crate::platform::agents::build_agent_session_export;
#[cfg(feature = "desktop")]
use crate::platform::dialogs::{import_skill, open_skill_folder, select_path};
use crate::platform::{
    agents::{
        apply_agent_management_mutation, create_agent_instance, delete_agent_session,
        duplicate_agent_session, export_agent_session, get_agent_management_descriptor,
        get_agent_management_section, get_agent_session, get_agent_session_attachment,
        get_agent_session_stats, get_agent_session_user_messages, import_agent_session,
        install_agent_plugin_bytes, list_agent_config_files, list_agent_plugins,
        list_agent_sessions, list_session_terminals, read_agent_config_file, remove_agent_instance,
        remove_agent_plugin, rename_agent_session, resume_agent_session, save_agent_config_file,
        set_agent_plugin_enabled,
    },
    entities::{delete_entity, get_entity, update_entity_with},
    hooks::{create_hook, delete_hook, update_hook},
    misc::query_logs,
    registry::fetch_registry_servers,
    skills::{
        create_skill_files, delete_skill_files, list_available_skill_targets,
        list_skills_with_installations, remove_skill_installation, set_skill_installation,
        update_skill_files,
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
    if !methods::is_known(&method) {
        return Err(format!("Unsupported platform API method: {method}"));
    }
    if method == "serverSelectFile" {
        return select_path(app, args.first()).await;
    }

    if method == "openSkillFolder" {
        return open_skill_folder(app, state, args).await;
    }

    if method == "importSkill" {
        return import_skill(app, state).await;
    }

    if method == "importAgentPlugin" {
        let Some(file_paths) = app
            .dialog()
            .file()
            .add_filter("MCP Link Agent Plugin", &["mclagent", "zip"])
            .blocking_pick_files()
        else {
            return Ok(Value::Null);
        };
        let mut installed = Vec::new();
        let mut failed = Vec::new();
        for file_path in file_paths {
            let path = match file_path.into_path() {
                Ok(path) => path,
                Err(error) => {
                    failed.push(json!({
                        "fileName": "",
                        "error": error.to_string(),
                    }));
                    continue;
                }
            };
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string();
            let result = std::fs::read(&path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| install_agent_plugin_bytes(state.inner().as_ref(), bytes));
            match result {
                Ok(plugin) => installed.push(plugin),
                Err(error) => failed.push(json!({
                    "fileName": file_name,
                    "error": error,
                })),
            }
        }
        return Ok(json!({ "installed": installed, "failed": failed }));
    }

    if method == "restartDesktopMcpEndpoint" {
        restart_desktop_mcp_http_server(state.inner().clone()).await?;
        return Ok(mcp_endpoint_status(state.inner().as_ref()));
    }

    if method == "exportMcpConfig" {
        let file_name = required_string(&args, 0)?;
        let content = required_string(&args, 1)?;
        let mut dialog = app
            .dialog()
            .file()
            .set_file_name(file_name)
            .add_filter("JSON", &["json"]);
        if let Ok(download_dir) = app.path().download_dir() {
            dialog = dialog.set_directory(download_dir);
        }
        let Some(file_path) = dialog.blocking_save_file() else {
            return Ok(Value::Bool(false));
        };
        let path = file_path.into_path().map_err(|error| error.to_string())?;
        std::fs::write(path, content).map_err(|error| error.to_string())?;
        return Ok(Value::Bool(true));
    }

    if method == "exportAgentSessionToFile" {
        let exported = build_agent_session_export(state.inner().as_ref(), &args)?;
        let file_name = exported.file_name.clone();
        let mut dialog = app.dialog().file().set_file_name(&file_name);
        if let Ok(download_dir) = app.path().download_dir() {
            dialog = dialog.set_directory(download_dir);
        }
        let Some(file_path) = dialog.blocking_save_file() else {
            return Ok(json!({
                "saved": false,
                "fileName": file_name,
            }));
        };
        let path = file_path.into_path().map_err(|error| error.to_string())?;
        let bytes = if exported.encoding == "base64" {
            STANDARD
                .decode(exported.content.as_bytes())
                .map_err(|error| error.to_string())?
        } else {
            exported.content.into_bytes()
        };
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
        return Ok(json!({
            "saved": true,
            "fileName": file_name,
            "path": path.to_string_lossy(),
        }));
    }

    let requested_autostart = (method == "saveSettings")
        .then(|| {
            args.first()
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("showWindowOnStartup"))
                .and_then(Value::as_bool)
        })
        .flatten();
    let restart_model_gateway = method == "saveGatewaySettings";
    let result = dispatch_platform_method(state.inner().as_ref(), &method, args).await?;
    if let Some(enabled) = requested_autostart {
        let autolaunch = app.autolaunch();
        if enabled {
            autolaunch.enable().map_err(|error| error.to_string())?;
        } else {
            autolaunch.disable().map_err(|error| error.to_string())?;
        }
    }
    if restart_model_gateway {
        crate::gateway::server::restart_desktop_model_gateway(state.inner().clone()).await?;
        return Ok(gateway_settings(state.inner().as_ref()));
    }
    Ok(result)
}

pub(crate) async fn dispatch_platform_method(
    state: &DesktopState,
    method: &str,
    args: Vec<Value>,
) -> Result<Value, String> {
    if !methods::is_known(method) {
        return Err(format!("Unsupported platform API method: {method}"));
    }
    if method == "getPlatformCapabilities" {
        return Ok(json!({
            "platform": if cfg!(feature = "desktop") { "desktop" } else { "server" },
            "capabilities": {
                "desktopDialogs": cfg!(feature = "desktop"),
                "autostart": cfg!(feature = "desktop"),
                "mcpHttpEndpoint": true,
                "agentPlugins": true,
                "gateway": true,
                "workflows": true
            }
        }));
    }
    if method == "getMcpEndpoint" {
        return Ok(Value::String(current_mcp_endpoint(state)));
    }
    if method == "getMcpEndpointStatus" {
        return Ok(mcp_endpoint_status(state));
    }
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
    if method == "listSkills" {
        return list_skills_with_installations(state);
    }
    if method == "listSkillTargets" {
        return list_available_skill_targets(state);
    }
    if method == "setSkillInstallation" {
        return set_skill_installation(state, &args);
    }
    if method == "removeSkillInstallation" {
        return remove_skill_installation(state, &args);
    }

    if let Some(result) = dispatch_agent_method(state, method, &args) {
        return result;
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

    if method == "listSessionTerminals" {
        return list_session_terminals();
    }

    if let Some(result) = dispatch_gateway_method(state, method, &args).await {
        return result;
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

fn dispatch_agent_method(
    state: &DesktopState,
    method: &str,
    args: &[Value],
) -> Option<Result<Value, String>> {
    Some(match method {
        "listAgentPlugins" => list_agent_plugins(state),
        "createAgentInstance" => create_agent_instance(state, args),
        "removeAgentInstance" => remove_agent_instance(state, args),
        "listAgentConfigFiles" => list_agent_config_files(state, args),
        "readAgentConfigFile" => read_agent_config_file(state, args),
        "saveAgentConfigFile" => save_agent_config_file(state, args),
        "listAgentSessions" => list_agent_sessions(state, args.first()),
        "getAgentSession" => get_agent_session(state, args),
        "getAgentManagementDescriptor" => get_agent_management_descriptor(state, args),
        "getAgentManagementSection" => get_agent_management_section(state, args),
        "getAgentSessionAttachment" => get_agent_session_attachment(state, args),
        "getAgentSessionStats" => get_agent_session_stats(state, args),
        "getAgentSessionUserMessages" => get_agent_session_user_messages(state, args),
        "resumeAgentSession" => resume_agent_session(state, args),
        "duplicateAgentSession" => duplicate_agent_session(state, args),
        "deleteAgentSession" => delete_agent_session(state, args),
        "renameAgentSession" => rename_agent_session(state, args),
        "exportAgentSession" => export_agent_session(state, args),
        "importAgentSession" => import_agent_session(state, args),
        "applyAgentManagementMutation" => apply_agent_management_mutation(state, args),
        "setAgentPluginEnabled" => set_agent_plugin_enabled(state, args),
        "removeAgentPlugin" => remove_agent_plugin(state, args),
        "installAgentPluginBytes" => args
            .first()
            .cloned()
            .ok_or_else(|| "Agent plugin data is required".to_string())
            .and_then(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
            .and_then(|bytes| install_agent_plugin_bytes(state, bytes)),
        _ => return None,
    })
}

async fn dispatch_gateway_method(
    state: &DesktopState,
    method: &str,
    args: &[Value],
) -> Option<Result<Value, String>> {
    Some(match method {
        "getGatewaySettings" => Ok(gateway_settings(state)),
        "regenerateGatewayAccessKey" => regenerate_gateway_access_key(state),
        "fetchGatewayProviderModels" => fetch_gateway_provider_models(args.first()).await,
        "listGatewayCallLogs" => list_call_logs(state, args.first()),
        "clearGatewayCallLogs" => clear_gateway_call_logs(state),
        _ => return None,
    })
}

fn handle_platform_method(
    store: &mut StoreState,
    method: &str,
    args: &[Value],
) -> Result<Value, String> {
    if let Some(result) = dispatch_gateway_store_method(store, method, args) {
        return result;
    }
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
                    if key == "serverPassword" {
                        let password = value
                            .as_str()
                            .map(str::trim)
                            .filter(|password| !password.is_empty())
                            .ok_or_else(|| "Server password cannot be empty".to_string())?;
                        store.settings.insert(key.clone(), json!(password));
                    } else {
                        store.settings.insert(key.clone(), value.clone());
                    }
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
        _ => Err(format!("Unsupported platform API method: {method}")),
    }
}

fn dispatch_gateway_store_method(
    store: &mut StoreState,
    method: &str,
    args: &[Value],
) -> Option<Result<Value, String>> {
    Some(match method {
        "listGatewayProviders" => {
            serde_json::to_value(list_gateway_providers(store)).map_err(|error| error.to_string())
        }
        "createGatewayProvider" => create_gateway_provider(store, args.first()),
        "updateGatewayProvider" => update_gateway_provider(store, args),
        "setActiveGatewayProvider" => {
            required_string(args, 0).and_then(|id| set_active_gateway_provider(store, &id))
        }
        "removeGatewayProvider" => required_string(args, 0)
            .and_then(|id| remove_gateway_provider(store, &id).map(|_| Value::Bool(true))),
        "listGatewayRoutes" => {
            serde_json::to_value(list_gateway_routes(store)).map_err(|error| error.to_string())
        }
        "createGatewayRoute" => create_gateway_route(store, args.first()),
        "updateGatewayRoute" => update_gateway_route(store, args),
        "removeGatewayRoute" => required_string(args, 0)
            .and_then(|id| remove_gateway_route(store, &id).map(|_| Value::Bool(true))),
        "saveGatewaySettings" => update_gateway_settings(store, args.first()),
        _ => return None,
    })
}

fn mutates_state(method: &str) -> bool {
    !matches!(
        method,
        "listMcpServers"
            | "listMcpServerTools"
            | "listNetworkInterfaces"
            | "listSessionTerminals"
            | "getMcpEndpoint"
            | "getMcpEndpointStatus"
            | "getSettings"
            | "getRequestLogs"
            | "listGatewayProviders"
            | "listGatewayRoutes"
            | "listWorkflows"
            | "getWorkflow"
            | "getEnabledWorkflows"
            | "getWorkflowsByType"
            | "listHookModules"
            | "getHookModule"
            | "validateHookScript"
    )
}
