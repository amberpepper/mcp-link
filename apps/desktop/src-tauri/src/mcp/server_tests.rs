use super::*;

#[test]
fn routed_resource_uri_round_trips_original_uri() {
    let routed = route_resource_uri("server-1", "file:///tmp/example.txt");
    assert_eq!(
        split_routed_resource_uri(&routed),
        Some((
            "server-1".to_string(),
            "file:///tmp/example.txt".to_string()
        ))
    );
}

#[test]
fn router_gateway_tools_have_stable_names() {
    let names = router_gateway_tools()
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, vec![ROUTER_LIST_TOOLS_TOOL, ROUTER_CALL_TOOL_TOOL]);
}

#[test]
fn gateway_call_parses_target_and_arguments() {
    let arguments = json!({
        "server_id": "server-a",
        "tool_name": "sample_tool",
        "arguments": { "query": "MCP" }
    });
    let (server_id, request) =
        parse_gateway_call(arguments.as_object()).expect("gateway call should parse");
    assert_eq!(server_id, "server-a");
    assert_eq!(request.name.as_ref(), "sample_tool");
    assert_eq!(
        request
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.get("query"))
            .and_then(Value::as_str),
        Some("MCP")
    );
}

#[test]
fn gateway_call_rejects_recursive_calls() {
    let arguments = json!({
        "server_id": "router",
        "tool_name": ROUTER_CALL_TOOL_TOOL
    });
    assert!(parse_gateway_call(arguments.as_object()).is_err());
}

#[test]
fn endpoint_status_reports_runtime_failure_and_actual_listener() {
    let root =
        std::env::temp_dir().join(format!("mcp-link-endpoint-status-{}", uuid::Uuid::new_v4()));
    let state = DesktopState::load(root.join("mcp.db"));
    *state.mcp_listener_error.lock().unwrap() = Some("port is busy".to_string());

    let failed = mcp_endpoint_status(&state);
    assert_eq!(failed["running"], false);
    assert_eq!(failed["error"], "port is busy");

    set_mcp_endpoint(&state, "127.0.0.1:43210".parse().unwrap());
    let running = mcp_endpoint_status(&state);
    assert_eq!(running["running"], true);
    assert_eq!(running["endpoint"], "http://127.0.0.1:43210/mcp");
    assert!(running["error"].is_null());

    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
