#[cfg(all(feature = "desktop", feature = "server"))]
compile_error!("desktop and server features are mutually exclusive");

mod access_keys;
#[cfg(feature = "server")]
mod embed;
mod gateway;
mod hook;
#[cfg(feature = "server")]
mod http;
mod mcp;
mod platform;
mod state;
mod util;
mod workflow;

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::menu::{Menu, MenuItem};
#[cfg(feature = "desktop")]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
#[cfg(feature = "desktop")]
use tauri::Manager;
#[cfg(feature = "desktop")]
use tauri_plugin_autostart::ManagerExt;

#[cfg(feature = "desktop")]
use crate::gateway::server::start_desktop_model_gateway;
#[cfg(feature = "desktop")]
use crate::mcp::server::start_desktop_mcp_http_server;
#[cfg(feature = "desktop")]
use crate::mcp::servers::start_auto_start_servers;
#[cfg(feature = "desktop")]
use crate::platform::platform_call;
#[cfg(feature = "desktop")]
use crate::platform::skills::initialize_skill_files;
#[cfg(feature = "desktop")]
use crate::state::DesktopState;

#[cfg(feature = "desktop")]
pub fn run_desktop() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("MCP Link")
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().local_data_dir()?.join("MCP Link");
            std::fs::create_dir_all(&data_dir)?;
            let state = Arc::new(DesktopState::load(data_dir.join("mcp.db")));
            platform::agents::install_bundled_agent_plugins(&state);
            let auto_start_app = state
                .store
                .lock()
                .ok()
                .and_then(|store| {
                    store
                        .settings
                        .get("showWindowOnStartup")
                        .and_then(serde_json::Value::as_bool)
                })
                .unwrap_or(false);
            let autolaunch = app.autolaunch();
            let autostart_result = match autolaunch.is_enabled() {
                Ok(current) if current == auto_start_app => Ok(()),
                Ok(_) if auto_start_app => autolaunch.enable(),
                Ok(_) => autolaunch.disable(),
                Err(error) => Err(error),
            };
            if let Err(error) = autostart_result {
                eprintln!("Failed to synchronize app autostart setting: {error}");
            }
            app.manage(state.clone());

            let open_item = MenuItem::with_id(app, "open", "打开 MCP Link", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&open_item, &quit_item])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .tooltip("MCP Link");
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.on_menu_event(|app, event| match event.id().as_ref() {
                "open" => show_main_window(app),
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_main_window(tray.app_handle());
                }
            })
            .build(app)?;

            start_desktop_mcp_http_server(state.clone());
            start_desktop_model_gateway(state.clone());
            tauri::async_runtime::spawn(async move {
                initialize_skill_files(&state);
                start_auto_start_servers(state).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let minimize_to_tray = window
                    .state::<Arc<DesktopState>>()
                    .store
                    .lock()
                    .ok()
                    .and_then(|store| {
                        store
                            .settings
                            .get("closeBehavior")
                            .and_then(serde_json::Value::as_str)
                            .map(|behavior| behavior == "minimizeToTray")
                    })
                    .unwrap_or(false);
                if minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![platform_call])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}

#[cfg(feature = "desktop")]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(not(feature = "desktop"))]
pub fn run_desktop() {
    panic!("desktop feature is not enabled");
}

#[cfg(feature = "server")]
pub fn run_server() {
    use std::sync::Arc;

    let state = Arc::new(state::DesktopState::load(executable_db_path()));
    platform::agents::install_bundled_agent_plugins(&state);
    let addr = std::env::var("MCP_LINK_HTTP_ADDR").unwrap_or_else(|_| "127.0.0.1:3284".into());
    let addr = addr
        .parse::<std::net::SocketAddr>()
        .unwrap_or_else(|error| panic!("invalid MCP_LINK_HTTP_ADDR: {error}"));

    if state.server_password() == "admin" {
        eprintln!(
            "WARNING: MCP Link is using the default server password 'admin'. Change it in Settings."
        );
    } else {
        eprintln!("MCP Link server password is configured.");
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    runtime.block_on(async move {
        platform::skills::initialize_skill_files(&state);
        mcp::servers::start_auto_start_servers(state.clone()).await;
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .unwrap_or_else(|error| panic!("failed to bind {addr}: {error}"));
        let actual_addr = listener.local_addr().unwrap_or(addr);
        if let Ok(mut endpoint) = state.model_gateway_endpoint.lock() {
            *endpoint = Some(format!("http://{actual_addr}"));
        }
        mcp::server::start_server_mcp_http_server(state.clone(), actual_addr).await;
        eprintln!("MCP Link server listening on http://{actual_addr}");
        axum::serve(listener, http::build_router(state))
            .await
            .expect("MCP Link server stopped unexpectedly");
    });
}

#[cfg(feature = "server")]
fn executable_db_path() -> std::path::PathBuf {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(std::env::temp_dir);
    dir.join("mcp.db")
}

#[cfg(not(feature = "server"))]
pub fn run_server() {
    panic!("server feature is not enabled");
}

#[cfg(test)]
mod tests {
    use crate::hook::runtime::{execute_hook_script, validate_hook_script};
    use crate::workflow::topology::{is_valid_workflow, validate_workflow};
    use serde_json::{json, Value};

    #[test]
    fn hook_script_returns_json_value() {
        let result = execute_hook_script(
            "return { value: context.value + 1, name: context.name };",
            &json!({ "value": 41, "name": "hook" }),
        )
        .expect("hook should execute");

        assert_eq!(result, json!({ "value": 42, "name": "hook" }));
    }

    #[test]
    fn hook_script_validation_rejects_invalid_syntax() {
        let result = validate_hook_script("return {");
        assert_eq!(result.get("valid").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn workflow_validation_accepts_start_mcp_end_path() {
        let workflow = json!({
            "id": "workflow-1",
            "name": "Valid Workflow",
            "workflowType": "tools/call",
            "enabled": true,
            "nodes": [
                { "id": "start", "type": "start", "data": { "label": "Start" } },
                { "id": "call", "type": "mcp-call", "data": { "label": "MCP Call" } },
                { "id": "end", "type": "end", "data": { "label": "End" } }
            ],
            "edges": [
                { "source": "start", "target": "call" },
                { "source": "call", "target": "end" }
            ]
        });

        assert!(is_valid_workflow(&workflow));
        assert!(validate_workflow(&workflow).is_ok());
    }

    #[test]
    fn workflow_validation_rejects_cycles() {
        let workflow = json!({
            "id": "workflow-1",
            "name": "Cyclic Workflow",
            "workflowType": "tools/call",
            "enabled": true,
            "nodes": [
                { "id": "start", "type": "start", "data": { "label": "Start" } },
                { "id": "call", "type": "mcp-call", "data": { "label": "MCP Call" } },
                { "id": "end", "type": "end", "data": { "label": "End" } }
            ],
            "edges": [
                { "source": "start", "target": "call" },
                { "source": "call", "target": "end" },
                { "source": "end", "target": "call" }
            ]
        });

        assert!(!is_valid_workflow(&workflow));
        assert!(validate_workflow(&workflow).is_err());
    }

    #[test]
    fn legacy_sse_endpoint_resolves_relative_to_sse_url() {
        use crate::mcp::transport::legacy_sse::resolve_legacy_sse_endpoint;
        use reqwest::Url;

        let base = Url::parse("https://example.com/sse").expect("valid base url");
        let endpoint = resolve_legacy_sse_endpoint(&base, "/messages?session=abc")
            .expect("endpoint should resolve");

        assert_eq!(
            endpoint.as_str(),
            "https://example.com/messages?session=abc"
        );
    }
}
