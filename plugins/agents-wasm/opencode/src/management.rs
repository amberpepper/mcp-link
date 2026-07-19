use mcp_link_agent_wasm_sdk::{
    management_section, management_section_descriptor, masked_secret, read_json_config,
    write_json_config,
};
use serde_json::{json, Map, Value};

const RESOURCE: &str = "config";
const PATH: &str = "";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params
        .get("instance")
        .ok_or("OpenCode instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "opencode",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", true),
            section("mcp", false),
            section("skills", false),
            section("prompts", false),
            section("providers", false),
            section("models", false),
            section("permissions", false),
            section("environment", true),
            section("raw-config", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let section_id = params
        .get("section")
        .and_then(Value::as_str)
        .ok_or("OpenCode management section is required")?;
    let (config, revision) = read_json_config(RESOURCE, PATH)?;
    let data = match section_id {
        "overview" => overview(params, &config),
        "mcp" => json!({ "servers": mcp_servers(&config) }),
        "providers" => json!({ "providers": providers(&config) }),
        "models" => models(&config),
        "permissions" => permissions(&config),
        "environment" => environment(params, &config),
        _ => {
            return Err(format!(
                "Unsupported OpenCode management section: {section_id}"
            ))
        }
    };
    Ok(management_section(section_id, &revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("OpenCode management mutation is required")?;
    let section_id = mutation
        .get("section")
        .and_then(Value::as_str)
        .ok_or("OpenCode mutation section is required")?;
    let action = mutation
        .get("action")
        .and_then(Value::as_str)
        .ok_or("OpenCode mutation action is required")?;
    let expected = mutation
        .get("expectedRevision")
        .and_then(Value::as_str)
        .ok_or("OpenCode expectedRevision is required")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (mut config, current) = read_json_config(RESOURCE, PATH)?;
    if current != expected {
        return Err(format!(
            "CONFIG_CONFLICT: configuration changed on disk (expected {expected}, found {current})"
        ));
    }
    match section_id {
        "mcp" => mutate_mcp(&mut config, action, mutation)?,
        "providers" => mutate_provider(&mut config, action, mutation)?,
        "models" => mutate_models(&mut config, mutation)?,
        "permissions" => mutate_permissions(&mut config, mutation)?,
        _ => return Err(format!("OpenCode section is read-only: {section_id}")),
    }
    let revision = if dry_run {
        current
    } else {
        write_json_config(RESOURCE, PATH, &config, expected)?
    };
    Ok(json!({
        "section": section_id,
        "revision": revision,
        "changed": true,
        "changedResources": ["opencode.json"],
        "restartRequired": false,
        "warnings": [],
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
    json!({
        "cliName": "OpenCode",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "defaultModel": config.get("model"),
        "defaultProvider": config.get("model").and_then(Value::as_str).and_then(|model| model.split('/').next()),
        "mcpServerCount": mcp_servers(config).len(),
        "providerCount": providers(config).len(),
        "skillTargetCount": 1,
        "warnings": []
    })
}

fn mcp_servers(config: &Value) -> Vec<Value> {
    config
        .get("mcp")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|servers| servers.iter())
        .map(|(id, server)| {
            let kind = server.get("type").and_then(Value::as_str).unwrap_or("local");
            let command = server.get("command").and_then(Value::as_array);
            json!({
                "id": id,
                "name": id,
                "transport": if kind == "remote" { "http" } else { "stdio" },
                "command": command.and_then(|items| items.first()).and_then(Value::as_str),
                "args": command.map(|items| items.iter().skip(1).filter_map(Value::as_str).collect::<Vec<_>>()).unwrap_or_default(),
                "url": server.get("url"),
                "env": server.get("environment").cloned().unwrap_or_else(|| json!({})),
                "headers": server.get("headers").cloned().unwrap_or_else(|| json!({})),
                "enabled": server.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                "scope": "global"
            })
        })
        .collect()
}

fn providers(config: &Value) -> Vec<Value> {
    config
        .get("provider")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|providers| providers.iter())
        .map(|(id, provider)| {
            let options = provider.get("options").unwrap_or(&Value::Null);
            let models = provider
                .get("models")
                .and_then(Value::as_object)
                .map(|models| models.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            json!({
                "id": id,
                "name": provider.get("name").and_then(Value::as_str).unwrap_or(id),
                "protocol": provider.get("protocol").and_then(Value::as_str).unwrap_or("openai"),
                "baseUrl": options.get("baseURL").or_else(|| options.get("baseUrl")),
                "apiKey": masked_secret(options.get("apiKey").or_else(|| options.get("api_key")).and_then(Value::as_str)),
                "defaultModel": config.get("model").and_then(Value::as_str).filter(|model| model.starts_with(&format!("{id}/"))).and_then(|model| model.split_once('/').map(|(_, model)| model)),
                "models": models,
                "enabled": true
            })
        })
        .collect()
}

fn models(config: &Value) -> Value {
    json!({
        "defaultModel": config.get("model"),
        "smallModel": config.get("small_model").or_else(|| config.get("smallModel")),
        "reasoningModel": Value::Null,
        "reasoningEffort": Value::Null,
        "availableModels": providers(config).iter().flat_map(|provider| provider.get("models").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string)).collect::<Vec<_>>(),
        "aliases": {}
    })
}

fn permissions(config: &Value) -> Value {
    let rules = config
        .get("permission")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|rules| rules.iter())
        .map(|(target, decision)| {
            json!({
                "id": target,
                "decision": decision.as_str().unwrap_or("ask"),
                "target": target,
                "kind": "tool"
            })
        })
        .collect::<Vec<_>>();
    json!({ "approvalMode": Value::Null, "sandboxMode": Value::Null, "rules": rules })
}

fn environment(params: &Value, config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "configFiles": [{ "id": "config", "label": "opencode.json", "path": "~/.config/opencode/opencode.json", "exists": !config.as_object().is_none_or(Map::is_empty) }],
        "variables": [],
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let servers = object_field(config, "mcp")?;
    let id = mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| {
            mutation
                .get("payload")
                .and_then(|payload| payload.get("id"))
                .and_then(Value::as_str)
        })
        .ok_or("MCP server id is required")?;
    if action == "remove" {
        servers.remove(id);
        return Ok(());
    }
    let payload = mutation
        .get("payload")
        .ok_or("MCP server payload is required")?;
    let transport = payload
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    let previous = servers.get(id).cloned().unwrap_or_else(|| json!({}));
    let mut value = previous.as_object().cloned().unwrap_or_default();
    if transport == "stdio" {
        let mut command = vec![json!(payload
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default())];
        command.extend(
            payload
                .get("args")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        value.insert("type".to_string(), json!("local"));
        value.insert("command".to_string(), Value::Array(command));
        value.insert(
            "environment".to_string(),
            payload.get("env").cloned().unwrap_or_else(|| json!({})),
        );
        value.remove("url");
        value.remove("headers");
    } else {
        value.insert("type".to_string(), json!("remote"));
        value.insert(
            "url".to_string(),
            payload.get("url").cloned().unwrap_or(Value::Null),
        );
        value.insert(
            "headers".to_string(),
            payload.get("headers").cloned().unwrap_or_else(|| json!({})),
        );
        value.remove("command");
        value.remove("environment");
    }
    value.insert(
        "enabled".to_string(),
        json!(payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)),
    );
    servers.insert(id.to_string(), Value::Object(value));
    Ok(())
}

fn mutate_provider(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let providers = object_field(config, "provider")?;
    let id = mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| {
            mutation
                .get("payload")
                .and_then(|payload| payload.get("id"))
                .and_then(Value::as_str)
        })
        .ok_or("Provider id is required")?;
    if action == "remove" {
        providers.remove(id);
        return Ok(());
    }
    let payload = mutation
        .get("payload")
        .ok_or("Provider payload is required")?;
    let previous = providers.get(id).cloned().unwrap_or_else(|| json!({}));
    let mut provider = previous.as_object().cloned().unwrap_or_default();
    provider.insert(
        "name".to_string(),
        json!(payload.get("name").and_then(Value::as_str).unwrap_or(id)),
    );
    provider.insert(
        "protocol".to_string(),
        json!(payload
            .get("protocol")
            .and_then(Value::as_str)
            .unwrap_or("openai")),
    );
    let mut options = provider
        .remove("options")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    if let Some(base_url) = payload.get("baseUrl").and_then(Value::as_str) {
        options.insert("baseURL".to_string(), json!(base_url));
    }
    if let Some(api_key) = payload
        .get("apiKeyValue")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        options.insert("apiKey".to_string(), json!(api_key));
    }
    provider.insert("options".to_string(), Value::Object(options));
    if let Some(models) = payload.get("models").and_then(Value::as_array) {
        let previous_models = provider
            .get("models")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        provider.insert(
            "models".to_string(),
            Value::Object(
                models
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|model| {
                        (
                            model.to_string(),
                            previous_models
                                .get(model)
                                .cloned()
                                .unwrap_or_else(|| json!({ "name": model })),
                        )
                    })
                    .collect(),
            ),
        );
    }
    providers.insert(id.to_string(), Value::Object(provider));
    Ok(())
}

fn mutate_models(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Model settings payload is required")?;
    let object = config
        .as_object_mut()
        .ok_or("OpenCode configuration must be an object")?;
    set_optional_string(object, "model", payload.get("defaultModel"));
    set_optional_string(object, "small_model", payload.get("smallModel"));
    Ok(())
}

fn mutate_permissions(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let rules = mutation
        .get("payload")
        .and_then(|payload| payload.get("rules"))
        .and_then(Value::as_array)
        .ok_or("Permission rules are required")?;
    let value = rules
        .iter()
        .filter_map(|rule| {
            Some((
                rule.get("target")?.as_str()?.to_string(),
                json!(rule.get("decision")?.as_str()?),
            ))
        })
        .collect::<Map<_, _>>();
    config
        .as_object_mut()
        .ok_or("OpenCode configuration must be an object")?
        .insert("permission".to_string(), Value::Object(value));
    Ok(())
}

fn object_field<'a>(
    config: &'a mut Value,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let object = config
        .as_object_mut()
        .ok_or("OpenCode configuration must be an object")?;
    if !object.get(key).is_some_and(Value::is_object) {
        object.insert(key.to_string(), json!({}));
    }
    object
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("OpenCode {key} must be an object"))
}

fn set_optional_string(object: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    match value
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_read_masks_configured_key() {
        let config = json!({ "provider": { "demo": {
            "options": { "apiKey": "test-secret", "baseURL": "https://example.com" }
        } } });
        let values = providers(&config);
        assert_eq!(values[0]["apiKey"]["configured"], true);
        assert!(!serde_json::to_string(&values)
            .unwrap()
            .contains("test-secret"));
    }

    #[test]
    fn provider_update_writes_trimmed_key() {
        let mut config = json!({ "provider": {} });
        mutate_provider(
            &mut config,
            "upsert",
            &json!({ "payload": { "id": "demo", "apiKeyValue": "  new-secret  " } }),
        )
        .unwrap();
        assert_eq!(
            config["provider"]["demo"]["options"]["apiKey"],
            "new-secret"
        );
    }

    #[test]
    fn provider_update_keeps_secret_and_unknown_model_fields() {
        let mut config = json!({
            "provider": { "demo": {
                "options": { "apiKey": "secret" },
                "models": { "model-a": { "limit": { "context": 100 } } },
                "unknown": true
            }}
        });
        mutate_provider(
            &mut config,
            "upsert",
            &json!({
                "entityId": "demo",
                "payload": { "id": "demo", "models": ["model-a"], "apiKeyValue": "" }
            }),
        )
        .unwrap();
        assert_eq!(config["provider"]["demo"]["options"]["apiKey"], "secret");
        assert_eq!(
            config["provider"]["demo"]["models"]["model-a"]["limit"]["context"],
            100
        );
        assert_eq!(config["provider"]["demo"]["unknown"], true);
    }

    #[test]
    fn mcp_update_keeps_unknown_fields() {
        let mut config = json!({ "mcp": { "demo": { "type": "local", "unknown": 7 } } });
        mutate_mcp(&mut config, "upsert", &json!({
            "entityId": "demo", "payload": { "id": "demo", "transport": "stdio", "command": "npx", "args": [], "enabled": true }
        })).unwrap();
        assert_eq!(config["mcp"]["demo"]["unknown"], 7);
    }
}
