use serde_json::{json, Map, Value};

use crate::hook::runtime::execute_hook_script;
use crate::util::{
    json::{merge_object, value_id},
    security::sanitize_for_security_boundary,
    time::now_millis,
};
use crate::workflow::topology::determine_workflow_execution_order;
use crate::workflow::McpValueHandler;

pub async fn execute_workflow_value(
    workflow: &Value,
    hooks: &[Value],
    context: Value,
    mcp_handler: Option<&McpValueHandler<'_>>,
) -> Value {
    let workflow_id = value_id(workflow).unwrap_or_default().to_string();
    let workflow_name = workflow
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Workflow")
        .to_string();

    if workflow.get("enabled").and_then(Value::as_bool) != Some(true) {
        return json!({
            "workflowId": workflow_id,
            "workflowName": workflow_name,
            "status": "error",
            "executedAt": now_millis(),
            "context": sanitize_for_security_boundary(&context),
            "results": {},
            "mcpResult": Value::Null,
            "error": format!("Workflow is disabled: {workflow_id}")
        });
    }

    let execution_order = match determine_workflow_execution_order(workflow) {
        Ok(order) => order,
        Err(error) => {
            return json!({
                "workflowId": workflow_id,
                "workflowName": workflow_name,
                "status": "error",
                "executedAt": now_millis(),
                "context": sanitize_for_security_boundary(&context),
                "results": {},
                "mcpResult": Value::Null,
                "error": error
            });
        }
    };

    let mut results = Map::new();
    let mut mcp_result = Value::Null;

    for node_id in execution_order {
        let Some(node) = workflow
            .get("nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|node| value_id(node) == Some(node_id.as_str()))
        else {
            continue;
        };

        let result =
            execute_workflow_node(workflow, hooks, node, &context, &results, mcp_handler).await;
        if node.get("type").and_then(Value::as_str) == Some("mcp-call") {
            if let Some(response) = result.get("mcpResponse") {
                mcp_result = response.clone();
            }
        }
        results.insert(node_id, result);
    }

    json!({
        "workflowId": workflow_id,
        "workflowName": workflow_name,
        "status": "completed",
        "executedAt": now_millis(),
        "context": sanitize_for_security_boundary(&context),
        "results": sanitize_for_security_boundary(&Value::Object(results)),
        "mcpResult": sanitize_for_security_boundary(&mcp_result)
    })
}

pub async fn execute_workflow_node(
    workflow: &Value,
    hooks: &[Value],
    node: &Value,
    context: &Value,
    previous_results: &Map<String, Value>,
    mcp_handler: Option<&McpValueHandler<'_>>,
) -> Value {
    match node.get("type").and_then(Value::as_str).unwrap_or_default() {
        "start" => json!({ "started": true, "timestamp": now_millis() }),
        "end" => json!({
            "completed": true,
            "timestamp": now_millis(),
            "previousResults": previous_results
        }),
        "hook" => execute_workflow_hook_node(workflow, hooks, node, context, previous_results),
        "mcp-call" => execute_workflow_mcp_call_node(mcp_handler).await,
        node_type => json!({
            "skipped": true,
            "reason": format!("Unknown node type: {node_type}")
        }),
    }
}

pub async fn execute_workflow_mcp_call_node(mcp_handler: Option<&McpValueHandler<'_>>) -> Value {
    let Some(handler) = mcp_handler else {
        return json!({
            "type": "mcp-call",
            "error": "MCP handler not found",
            "timestamp": now_millis()
        });
    };

    match handler().await {
        Ok(mcp_response) => json!({
            "type": "mcp-call",
            "success": true,
            "mcpResponse": mcp_response,
            "timestamp": now_millis()
        }),
        Err(error) => json!({
            "type": "mcp-call",
            "success": false,
            "error": error,
            "timestamp": now_millis()
        }),
    }
}

pub fn execute_workflow_hook_node(
    workflow: &Value,
    hooks: &[Value],
    node: &Value,
    context: &Value,
    previous_results: &Map<String, Value>,
) -> Value {
    let Some(hook) = node.get("data").and_then(|data| data.get("hook")) else {
        return json!({ "skipped": true, "reason": "No hook configuration" });
    };

    let hook_context = sanitize_for_security_boundary(&merge_object(
        context.clone(),
        json!({
            "workflowId": value_id(workflow).unwrap_or_default(),
            "workflowName": workflow.get("name").and_then(Value::as_str).unwrap_or("Workflow"),
            "nodeId": value_id(node).unwrap_or_default(),
            "nodeName": node.get("data").and_then(|data| data.get("label")).and_then(Value::as_str).unwrap_or_else(|| value_id(node).unwrap_or_default()),
            "previousResults": previous_results
        }),
    ));

    let script = if let Some(hook_module_id) = hook.get("hookModuleId").and_then(Value::as_str) {
        let Some(module) = hooks
            .iter()
            .find(|module| value_id(module) == Some(hook_module_id))
        else {
            return json!({
                "success": false,
                "error": format!("Hook module not found: {hook_module_id}"),
                "timestamp": now_millis()
            });
        };
        module.get("script").and_then(Value::as_str)
    } else {
        hook.get("script").and_then(Value::as_str)
    };

    let Some(script) = script.filter(|script| !script.trim().is_empty()) else {
        return json!({ "skipped": true, "reason": "No script specified" });
    };

    match execute_hook_script(script, &hook_context) {
        Ok(result) => json!({ "success": true, "result": result, "timestamp": now_millis() }),
        Err(error) => json!({ "success": false, "error": error, "timestamp": now_millis() }),
    }
}
