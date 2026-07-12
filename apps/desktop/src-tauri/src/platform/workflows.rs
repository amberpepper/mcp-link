use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    state::{find_entity_mut, DesktopState, StoreState},
    util::{
        json::{merge_value_object, required_string, set_object_value, value_id},
        time::now_millis,
    },
    workflow::{
        executor::execute_workflow_value,
        topology::{is_valid_workflow, validate_workflow},
    },
};

pub(crate) async fn execute_workflow_platform(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let id = required_string(&args, 0)?;
    let context = args.get(1).cloned().unwrap_or_else(|| json!({}));
    let (workflow, hooks) = {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        let workflow = store
            .workflows
            .iter()
            .find(|workflow| value_id(workflow) == Some(id.as_str()))
            .cloned()
            .ok_or_else(|| format!("Workflow not found: {id}"))?;
        (workflow, store.hooks.clone())
    };

    Ok(execute_workflow_value(&workflow, &hooks, context, None).await)
}

pub(crate) fn create_workflow(
    store: &mut StoreState,
    input: Option<&Value>,
) -> Result<Value, String> {
    let input = input.cloned().unwrap_or_else(|| json!({}));
    let now = now_millis();
    let workflow = json!({
        "id": input.get("id").and_then(Value::as_str).map(ToOwned::to_owned).unwrap_or_else(|| Uuid::new_v4().to_string()),
        "name": input.get("name").and_then(Value::as_str).unwrap_or("Workflow"),
        "description": input.get("description").and_then(Value::as_str).unwrap_or(""),
        "workflowType": input.get("workflowType").and_then(Value::as_str).unwrap_or("tools/call"),
        "nodes": input.get("nodes").cloned().unwrap_or_else(|| json!([])),
        "edges": input.get("edges").cloned().unwrap_or_else(|| json!([])),
        "enabled": input.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        "createdAt": now,
        "updatedAt": now
    });
    validate_workflow(&workflow)?;
    store.workflows.push(workflow.clone());
    Ok(workflow)
}

pub(crate) fn update_workflow(store: &mut StoreState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let updates = args.get(1).cloned().unwrap_or_else(|| json!({}));
    let workflow = find_entity_mut(&mut store.workflows, &id)?;
    let mut merged = workflow.clone();
    merge_value_object(&mut merged, updates);
    set_object_value(&mut merged, "id", json!(id));
    set_object_value(&mut merged, "updatedAt", json!(now_millis()));
    validate_workflow(&merged)?;
    *workflow = merged.clone();
    Ok(merged)
}

pub(crate) fn set_active_workflow(store: &mut StoreState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let workflow = store
        .workflows
        .iter()
        .find(|workflow| value_id(workflow) == Some(id.as_str()))
        .ok_or_else(|| format!("Workflow not found: {id}"))?;
    if !is_valid_workflow(workflow) {
        return Err(format!(
            "Workflow \"{}\" is not valid. Ensure it has Start -> MCP Call -> End nodes properly connected.",
            workflow.get("name").and_then(Value::as_str).unwrap_or("Workflow")
        ));
    }
    for workflow in &mut store.workflows {
        let active = value_id(workflow) == Some(id.as_str());
        set_object_value(workflow, "enabled", Value::Bool(active));
        if active {
            set_object_value(workflow, "updatedAt", json!(now_millis()));
        }
    }
    Ok(Value::Bool(true))
}
