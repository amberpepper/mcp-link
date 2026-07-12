use serde_json::{json, Value};

use crate::util::json::{merge_value_object, required_string, value_id};

pub(crate) fn get_entity(items: &[Value], args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    Ok(items
        .iter()
        .find(|item| value_id(item) == Some(id.as_str()))
        .cloned()
        .unwrap_or(Value::Null))
}

pub(crate) fn update_entity_with(
    items: &mut [Value],
    args: &[Value],
    extra_updates: Value,
) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let updates = args.get(1).cloned().unwrap_or_else(|| json!({}));
    let item = crate::state::find_entity_mut(items, &id)?;
    merge_value_object(item, updates);
    merge_value_object(item, extra_updates);
    Ok(item.clone())
}

pub(crate) fn delete_entity(items: &mut Vec<Value>, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let original_len = items.len();
    items.retain(|item| value_id(item) != Some(id.as_str()));
    Ok(Value::Bool(items.len() != original_len))
}
