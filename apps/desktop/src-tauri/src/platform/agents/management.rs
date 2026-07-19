//! Generic host boundary for plugin-owned management capabilities.
//!
//! Section identifiers and `data` payloads are intentionally opaque here.
//! CLI configuration knowledge belongs in the corresponding WASM plugin.

use std::collections::HashSet;

use serde_json::Value;

use crate::{state::DesktopState, util::json::required_string};

use super::{agent_id_for_instance, external_plugin, plugins, require_capability};

pub(crate) fn get_agent_management_descriptor(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let instance_id = required_string(args, 0)?;
    let agent_id = agent_id_for_instance(state, &instance_id)?;
    require_capability(state, &agent_id, "management.read")?;
    let value =
        plugins::describe_management(state, &external_plugin(state, &agent_id)?, &instance_id)?;
    validate_descriptor(&value, &agent_id, &instance_id)?;
    Ok(value)
}

pub(crate) fn get_agent_management_section(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let instance_id = required_string(args, 0)?;
    let section = required_string(args, 1)?;
    validate_identifier(&section)?;
    let agent_id = agent_id_for_instance(state, &instance_id)?;
    require_capability(state, &agent_id, "management.read")?;
    let value = plugins::load_management_section(
        state,
        &external_plugin(state, &agent_id)?,
        &instance_id,
        &section,
    )?;
    validate_section_envelope(&value, &section)?;
    Ok(value)
}

pub(crate) fn apply_agent_management_mutation(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    mutate(state, args)
}

fn mutate(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let instance_id = required_string(args, 0)?;
    let mutation = args
        .get(1)
        .filter(|value| value.is_object())
        .ok_or_else(|| "Agent management mutation is required".to_string())?;
    let section = mutation
        .get("section")
        .and_then(Value::as_str)
        .filter(|section| validate_identifier(section).is_ok())
        .ok_or_else(|| "Agent management mutation section is invalid".to_string())?;
    mutation
        .get("action")
        .and_then(Value::as_str)
        .filter(|action| !action.trim().is_empty())
        .ok_or_else(|| "Agent management mutation action is required".to_string())?;
    mutation
        .get("expectedRevision")
        .and_then(Value::as_str)
        .filter(|revision| !revision.is_empty())
        .ok_or_else(|| "Agent management expectedRevision is required".to_string())?;
    let agent_id = agent_id_for_instance(state, &instance_id)?;
    require_capability(state, &agent_id, "management.write")?;
    let value = plugins::mutate_management_section(
        state,
        &external_plugin(state, &agent_id)?,
        &instance_id,
        mutation,
        false,
    )?;
    validate_mutation_envelope(&value, section)?;
    Ok(value)
}

fn validate_identifier(identifier: &str) -> Result<(), String> {
    let valid = !identifier.is_empty()
        && identifier.len() <= 64
        && identifier.bytes().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, b'-' | b'_' | b'.')
        });
    valid
        .then_some(())
        .ok_or_else(|| "Agent management identifier is invalid".to_string())
}

fn validate_descriptor(value: &Value, agent_id: &str, instance_id: &str) -> Result<(), String> {
    if value.get("schemaVersion").and_then(Value::as_u64) != Some(1)
        || value.get("agentId").and_then(Value::as_str) != Some(agent_id)
        || value.get("instanceId").and_then(Value::as_str) != Some(instance_id)
    {
        return Err("Agent plugin returned an invalid management descriptor identity".to_string());
    }
    let sections = value
        .get("sections")
        .and_then(Value::as_array)
        .filter(|sections| !sections.is_empty() && sections.len() <= 64)
        .ok_or_else(|| "Agent plugin returned invalid management sections".to_string())?;
    let mut seen = HashSet::new();
    for section in sections {
        let id = section
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Agent plugin returned a malformed management section".to_string())?;
        validate_identifier(id)?;
        let renderer = section
            .get("renderer")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "Agent plugin returned a management section without a renderer".to_string()
            })?;
        validate_identifier(renderer)?;
        if !matches!(
            section.get("source").and_then(Value::as_str),
            Some("plugin" | "host")
        ) {
            return Err("Agent plugin returned an invalid management section source".to_string());
        }
        if !seen.insert(id) || section.get("readOnly").and_then(Value::as_bool).is_none() {
            return Err(
                "Agent plugin returned a duplicate or malformed management section".to_string(),
            );
        }
    }
    Ok(())
}

fn validate_section_envelope(value: &Value, expected_id: &str) -> Result<(), String> {
    if value.get("id").and_then(Value::as_str) != Some(expected_id)
        || value
            .get("revision")
            .and_then(Value::as_str)
            .filter(|revision| !revision.is_empty())
            .is_none()
        || value.get("data").is_none()
    {
        return Err("Agent plugin returned an invalid management section".to_string());
    }
    Ok(())
}

fn validate_mutation_envelope(value: &Value, expected_id: &str) -> Result<(), String> {
    let resources_valid = value
        .get("changedResources")
        .and_then(Value::as_array)
        .is_some_and(|resources| resources.len() <= 16 && resources.iter().all(Value::is_string));
    if value.get("section").and_then(Value::as_str) != Some(expected_id)
        || value
            .get("revision")
            .and_then(Value::as_str)
            .filter(|revision| !revision.is_empty())
            .is_none()
        || value.get("changed").and_then(Value::as_bool).is_none()
        || !resources_valid
    {
        return Err("Agent plugin returned an invalid management mutation result".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn accepts_plugin_owned_sections_without_a_host_allowlist() {
        let custom = json!({
            "schemaVersion": 1,
            "agentId": "agent",
            "instanceId": "instance",
            "sections": [{ "id": "plugin-ui", "renderer": "form", "source": "plugin", "readOnly": false }]
        });
        assert!(validate_descriptor(&custom, "agent", "instance").is_ok());
    }

    #[test]
    fn rejects_duplicate_or_malformed_section_descriptors() {
        let duplicate = json!({
            "schemaVersion": 1,
            "agentId": "agent",
            "instanceId": "instance",
            "sections": [
                { "id": "custom", "renderer": "form", "source": "plugin", "readOnly": true },
                { "id": "custom", "renderer": "form", "source": "plugin", "readOnly": true }
            ]
        });
        assert!(validate_descriptor(&duplicate, "agent", "instance").is_err());

        let malformed = json!({
            "schemaVersion": 1,
            "agentId": "agent",
            "instanceId": "instance",
            "sections": [{ "id": "contains spaces", "renderer": "form", "source": "plugin", "readOnly": false }]
        });
        assert!(validate_descriptor(&malformed, "agent", "instance").is_err());
    }

    #[test]
    fn treats_plugin_section_data_as_opaque() {
        let value = json!({
            "id": "plugin-ui",
            "revision": "sha256:test",
            "data": { "pluginOwnedShape": [1, 2, 3] }
        });
        assert!(validate_section_envelope(&value, "plugin-ui").is_ok());
    }
}
