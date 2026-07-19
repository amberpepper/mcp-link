use mcp_link_agent_wasm_sdk::{
    management_section, management_section_descriptor, masked_secret, read_json_config,
    write_json_config,
};
use serde_json::{json, Map, Value};

const RESOURCE: &str = "settings";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params.get("instance").ok_or("Crush instance is required")?;
    Ok(json!({
        "schemaVersion": 1, "agentId": "crush", "instanceId": instance.get("id"),
        "sections": [
            section("overview", true), section("mcp", false), section("skills", false),
            section("prompts", false),
            section("providers", false), section("models", false),
            section("environment", true), section("raw-config", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let id = params
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Crush management section is required")?;
    let (config, revision) = read_json_config(RESOURCE, "")?;
    let data = match id {
        "overview" => overview(params, &config),
        "mcp" => json!({ "servers": mcp_servers(&config) }),
        "providers" => json!({ "providers": providers(&config) }),
        "models" => models(&config),
        "environment" => environment(params, &config),
        _ => return Err(format!("Unsupported Crush management section: {id}")),
    };
    Ok(management_section(id, &revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Crush management mutation is required")?;
    let section_id = mutation
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Crush mutation section is required")?;
    let action = mutation
        .get("action")
        .and_then(Value::as_str)
        .ok_or("Crush mutation action is required")?;
    let expected = mutation
        .get("expectedRevision")
        .and_then(Value::as_str)
        .ok_or("Crush expectedRevision is required")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (mut config, revision) = read_json_config(RESOURCE, "")?;
    if revision != expected {
        return Err(format!("CONFIG_CONFLICT: configuration changed on disk (expected {expected}, found {revision})"));
    }
    match section_id {
        "mcp" => mutate_mcp(&mut config, action, mutation)?,
        "providers" => mutate_provider(&mut config, action, mutation)?,
        "models" => mutate_models(&mut config, mutation)?,
        _ => return Err(format!("Crush section is read-only: {section_id}")),
    }
    let next_revision = if dry_run {
        revision
    } else {
        write_json_config(RESOURCE, "", &config, expected)?
    };
    Ok(json!({
        "section": section_id, "revision": next_revision, "changed": true,
        "changedResources": ["crush.json"], "restartRequired": false,
        "warnings": []
    }))
}

fn section(id: &str, read_only: bool) -> Value {
    let source = if matches!(id, "skills" | "prompts" | "raw-config") {
        "host"
    } else {
        "plugin"
    };
    management_section_descriptor(id, id, source, read_only)
}

fn overview(params: &Value, config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    let large = config.get("models").and_then(|value| value.get("large"));
    json!({
        "cliName": "Crush", "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"), "skillRoot": instance.get("skillRoot"),
        "defaultModel": large.and_then(|value| value.get("model")),
        "defaultProvider": large.and_then(|value| value.get("provider")),
        "mcpServerCount": mcp_servers(config).len(), "providerCount": providers(config).len(),
        "skillTargetCount": 1, "warnings": []
    })
}

fn mcp_servers(config: &Value) -> Vec<Value> {
    config
        .get("mcp")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|servers| servers.iter())
        .map(|(id, server)| {
            let transport = server
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("stdio");
            json!({
                "id": id,
                "name": id,
                "transport": transport,
                "command": server.get("command"),
                "args": server.get("args").cloned().unwrap_or_else(|| json!([])),
                "url": server.get("url"),
                "env": server.get("env").cloned().unwrap_or_else(|| json!({})),
                "headers": server.get("headers").cloned().unwrap_or_else(|| json!({})),
                "enabled": !server.get("disabled").and_then(Value::as_bool).unwrap_or(false),
                "scope": "global"
            })
        })
        .collect()
}

fn providers(config: &Value) -> Vec<Value> {
    let used_models = config.get("models").and_then(Value::as_object);
    config.get("providers").and_then(Value::as_object).into_iter().flat_map(|items| items.iter()).map(|(id, provider)| {
        let models = used_models.into_iter().flat_map(|items| items.values()).filter(|model| model.get("provider").and_then(Value::as_str) == Some(id)).filter_map(|model| model.get("model").and_then(Value::as_str)).collect::<Vec<_>>();
        json!({
            "id": id, "name": provider.get("name").and_then(Value::as_str).unwrap_or(id),
            "protocol": "custom", "baseUrl": provider.get("base_url").or_else(|| provider.get("baseUrl")),
            "apiKey": masked_secret(provider.get("api_key").or_else(|| provider.get("apiKey")).and_then(Value::as_str)),
            "defaultModel": models.first(), "models": models, "enabled": true
        })
    }).collect()
}

fn models(config: &Value) -> Value {
    let models = config.get("models").and_then(Value::as_object);
    let large = models.and_then(|items| items.get("large"));
    let small = models.and_then(|items| items.get("small"));
    let available = models
        .into_iter()
        .flat_map(|items| items.values())
        .filter_map(|item| item.get("model").and_then(Value::as_str))
        .collect::<Vec<_>>();
    json!({
        "defaultModel": large.and_then(|item| item.get("model")),
        "smallModel": small.and_then(|item| item.get("model")),
        "reasoningModel": Value::Null, "reasoningEffort": Value::Null,
        "availableModels": available, "aliases": {}
    })
}

fn environment(params: &Value, config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "configFiles": [{ "id": "settings", "label": "crush.json", "path": "~/.local/share/crush/crush.json", "exists": !config.as_object().is_none_or(Map::is_empty) }],
        "variables": [], "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"), "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation)?;
    let servers = object_field(config, "mcp")?;
    if action == "remove" {
        servers.remove(id);
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported Crush MCP action: {action}"));
    }
    let payload = mutation
        .get("payload")
        .ok_or("MCP server payload is required")?;
    let transport = payload
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    let mut server = servers
        .get(id)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    server.insert("type".into(), json!(transport));
    server.insert(
        "disabled".into(),
        json!(!payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)),
    );
    if transport == "stdio" {
        server.insert(
            "command".into(),
            json!(payload
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()),
        );
        server.insert(
            "args".into(),
            payload.get("args").cloned().unwrap_or_else(|| json!([])),
        );
        server.insert(
            "env".into(),
            payload.get("env").cloned().unwrap_or_else(|| json!({})),
        );
        server.remove("url");
        server.remove("headers");
    } else {
        server.insert(
            "url".into(),
            payload.get("url").cloned().unwrap_or(Value::Null),
        );
        server.insert(
            "headers".into(),
            payload.get("headers").cloned().unwrap_or_else(|| json!({})),
        );
        server.remove("command");
        server.remove("args");
        server.remove("env");
    }
    servers.insert(id.to_string(), Value::Object(server));
    Ok(())
}

fn mutate_provider(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation)?;
    let providers = object_field(config, "providers")?;
    if action == "remove" {
        providers.remove(id);
        return Ok(());
    }
    let payload = mutation
        .get("payload")
        .ok_or("Provider payload is required")?;
    let previous = providers.get(id).cloned().unwrap_or_else(|| json!({}));
    let mut provider = previous.as_object().cloned().unwrap_or_default();
    set_optional(&mut provider, "name", payload.get("name"));
    set_optional(&mut provider, "base_url", payload.get("baseUrl"));
    if let Some(key) = payload
        .get("apiKeyValue")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        provider.insert("api_key".to_string(), json!(key));
    }
    providers.insert(id.to_string(), Value::Object(provider));
    Ok(())
}

fn mutate_models(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Model settings payload is required")?;
    let models = object_field(config, "models")?;
    set_model_slot(models, "large", payload.get("defaultModel"));
    set_model_slot(models, "small", payload.get("smallModel"));
    Ok(())
}

fn set_model_slot(models: &mut Map<String, Value>, slot: &str, input: Option<&Value>) {
    let Some(model) = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let mut settings = models
        .get(slot)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    settings.insert("model".to_string(), json!(model));
    models.insert(slot.to_string(), Value::Object(settings));
}

fn object_field<'a>(
    config: &'a mut Value,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let root = config
        .as_object_mut()
        .ok_or("Crush configuration must be an object")?;
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), json!({}));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("Crush {key} must be an object"))
}

fn set_optional(object: &mut Map<String, Value>, key: &str, input: Option<&Value>) {
    match input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => {
            object.insert(key.to_string(), json!(value));
        }
        None => {
            object.remove(key);
        }
    }
}

fn entity_id(mutation: &Value) -> Result<&str, String> {
    mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| {
            mutation
                .get("payload")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
        })
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "Provider id is required".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_read_masks_configured_key() {
        let config = json!({ "providers": { "demo": { "api_key": "test-secret" } } });
        let values = providers(&config);
        assert_eq!(values[0]["apiKey"]["configured"], true);
        assert!(!serde_json::to_string(&values)
            .unwrap()
            .contains("test-secret"));
    }

    #[test]
    fn provider_update_writes_trimmed_key() {
        let mut config = json!({ "providers": {} });
        mutate_provider(
            &mut config,
            "upsert",
            &json!({ "payload": { "id": "demo", "apiKeyValue": "  new-secret  " } }),
        )
        .unwrap();
        assert_eq!(config["providers"]["demo"]["api_key"], "new-secret");
    }

    #[test]
    fn provider_update_preserves_api_key_when_blank() {
        let mut config = json!({ "providers": { "demo": { "api_key": "secret", "unknown": 1 } } });
        mutate_provider(&mut config, "upsert", &json!({ "entityId": "demo", "payload": { "id": "demo", "baseUrl": "https://example.com", "apiKeyValue": "" } })).unwrap();
        assert_eq!(config["providers"]["demo"]["api_key"], "secret");
        assert_eq!(config["providers"]["demo"]["unknown"], 1);
    }

    #[test]
    fn mcp_update_preserves_tool_filters_and_unknown_fields() {
        let mut config = json!({
            "mcp": { "demo": {
                "type": "stdio", "command": "old", "enabled_tools": ["search"],
                "disabled_tools": ["delete"], "unknown": 7
            }},
            "options": { "context_paths": ["PROJECT.md"] }
        });
        mutate_mcp(&mut config, "upsert", &json!({
            "entityId": "demo",
            "payload": { "id": "demo", "transport": "stdio", "command": "new", "args": [], "env": {}, "enabled": true }
        })).unwrap();
        assert_eq!(config["mcp"]["demo"]["enabled_tools"][0], "search");
        assert_eq!(config["mcp"]["demo"]["disabled_tools"][0], "delete");
        assert_eq!(config["mcp"]["demo"]["unknown"], 7);
        assert_eq!(config["options"]["context_paths"][0], "PROJECT.md");
    }
}
