use std::collections::HashMap;

use serde_json::Value;

pub fn value_id(value: &Value) -> Option<&str> {
    value.get("id").and_then(Value::as_str)
}

pub fn required_string(args: &[Value], index: usize) -> Result<String, String> {
    args.get(index)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Missing string argument at index {index}"))
}

pub fn merge_object(mut target: Value, patch: Value) -> Value {
    merge_value_object(&mut target, patch);
    target
}

pub fn merge_value_object(target: &mut Value, patch: Value) {
    let Some(target_object) = target.as_object_mut() else {
        return;
    };
    let Some(patch_object) = patch.as_object() else {
        return;
    };
    for (key, value) in patch_object {
        target_object.insert(key.clone(), value.clone());
    }
}

pub fn set_object_field(target: &mut Value, key: &str, value: impl Into<String>) {
    set_object_value(target, key, Value::String(value.into()));
}

pub fn set_object_value(target: &mut Value, key: &str, value: Value) {
    if let Some(object) = target.as_object_mut() {
        object.insert(key.to_string(), value);
    }
}

pub fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

pub fn string_map(value: Option<&Value>) -> HashMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}
