use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    hook::runtime::{execute_hook_script, validate_hook_module},
    platform::entities::delete_entity,
    state::{find_entity_mut, DesktopState, StoreState},
    util::{
        json::{merge_value_object, required_string, set_object_value, value_id},
        security::sanitize_for_security_boundary,
    },
};

pub(crate) fn execute_hook_module_platform(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let id = required_string(&args, 0)?;
    let context = args.get(1).cloned().unwrap_or_else(|| json!({}));
    let script = {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        store
            .hooks
            .iter()
            .find(|hook| value_id(hook) == Some(id.as_str()))
            .and_then(|hook| hook.get("script").and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .ok_or_else(|| format!("Hook module not found: {id}"))?
    };

    execute_hook_script(&script, &sanitize_for_security_boundary(&context))
}

pub(crate) fn create_hook(store: &mut StoreState, input: Option<&Value>) -> Result<Value, String> {
    let input = input.cloned().unwrap_or_else(|| json!({}));
    let hook = json!({
        "id": input.get("id").and_then(Value::as_str).map(ToOwned::to_owned).unwrap_or_else(|| Uuid::new_v4().to_string()),
        "name": input.get("name").and_then(Value::as_str).unwrap_or("Hook"),
        "script": input.get("script").and_then(Value::as_str).unwrap_or("")
    });
    validate_hook_module(&hook)?;
    store.hooks.push(hook.clone());
    Ok(hook)
}

pub(crate) fn update_hook(store: &mut StoreState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let updates = args.get(1).cloned().unwrap_or_else(|| json!({}));
    let hook = find_entity_mut(&mut store.hooks, &id)?;
    let mut merged = hook.clone();
    merge_value_object(&mut merged, updates);
    set_object_value(&mut merged, "id", json!(id));
    validate_hook_module(&merged)?;
    *hook = merged.clone();
    Ok(merged)
}

pub(crate) fn delete_hook(store: &mut StoreState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let using_workflows = store
        .workflows
        .iter()
        .filter(|workflow| {
            workflow
                .get("nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|node| {
                    node.get("type").and_then(Value::as_str) == Some("hook")
                        && node
                            .get("data")
                            .and_then(|data| data.get("hook"))
                            .and_then(|hook| hook.get("hookModuleId"))
                            .and_then(Value::as_str)
                            == Some(id.as_str())
                })
        })
        .filter_map(|workflow| workflow.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    if !using_workflows.is_empty() {
        return Err(format!(
            "Cannot delete hook module. It is used by workflow(s): {}",
            using_workflows.join(", ")
        ));
    }

    delete_entity(&mut store.hooks, args)
}
