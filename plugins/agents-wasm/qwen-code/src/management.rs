use mcp_link_agent_wasm_sdk::{
    finish_json_management_mutation, management_section, management_section_descriptor,
    masked_secret, read_json_document,
};
use serde_json::{json, Map, Value};

const SETTINGS: &str = "settings";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params
        .get("instance")
        .ok_or("Qwen Code instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "qwen-code",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", "overview", "plugin", true),
            section("mcp", "mcp", "plugin", false),
            section("skills", "skills", "host", false),
            section("prompts", "prompts", "host", false),
            section("providers", "providers", "plugin", false),
            section("models", "models", "plugin", false),
            section("environment", "environment", "plugin", true),
            section("raw-config", "raw-config", "host", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let id = required_string(params, "section")?;
    let (document, config) = read_json_document(SETTINGS, "", "Qwen settings.json")?;
    let data = match id {
        "overview" => overview(params, &config),
        "mcp" => json!({ "servers": mcp_servers(&config), "canDisable": false }),
        "providers" => json!({
            "providers": providers(&config),
            "canEditModels": false,
            "secretInput": {
                "mode": "environment-variable",
                "defaultEnvironmentVariable": "OPENAI_API_KEY"
            }
        }),
        "models" => models(&config),
        "environment" => environment(params, &config),
        _ => return Err(format!("Unsupported Qwen Code management section: {id}")),
    };
    Ok(management_section(id, &document.revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Qwen Code management mutation is required")?;
    let section = required_string(mutation, "section")?;
    let action = required_string(mutation, "action")?;
    let expected = required_string(mutation, "expectedRevision")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (document, mut config) = read_json_document(SETTINGS, "", "Qwen settings.json")?;
    ensure_revision(expected, &document.revision)?;
    match section {
        "mcp" => mutate_mcp(&mut config, action, mutation)?,
        "providers" => mutate_provider(&mut config, action, mutation)?,
        "models" => mutate_models(&mut config, mutation)?,
        _ => return Err(format!("Qwen Code section is read-only: {section}")),
    }
    finish_json_management_mutation(
        SETTINGS,
        "",
        section,
        "settings.json",
        &config,
        document,
        dry_run,
        section == "mcp",
    )
}

fn section(id: &str, renderer: &str, source: &str, read_only: bool) -> Value {
    management_section_descriptor(id, renderer, source, read_only)
}

fn overview(params: &Value, config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "Qwen Code",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "defaultModel": config.pointer("/model/name"),
        "defaultProvider": config.pointer("/security/auth/selectedType"),
        "mcpServerCount": mcp_servers(config).len(),
        "providerCount": providers(config).len(),
        "skillTargetCount": 3,
        "warnings": []
    })
}

fn mcp_servers(config: &Value) -> Vec<Value> {
    config
        .get("mcpServers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|servers| servers.iter())
        .map(|(id, server)| {
            let transport = server
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_else(|| {
                    if server.get("command").is_some() {
                        "stdio"
                    } else if server.get("httpUrl").is_some() {
                        "http"
                    } else {
                        "sse"
                    }
                });
            json!({
                "id": id,
                "name": server.get("description").and_then(Value::as_str).unwrap_or(id),
                "transport": transport,
                "command": server.get("command"),
                "args": server.get("args").cloned().unwrap_or_else(|| json!([])),
                "url": if transport == "http" { server.get("httpUrl").or_else(|| server.get("url")) } else { server.get("url") },
                "env": server.get("env").cloned().unwrap_or_else(|| json!({})),
                "headers": server.get("headers").cloned().unwrap_or_else(|| json!({})),
                "enabled": true,
                "scope": "global"
            })
        })
        .collect()
}

fn providers(config: &Value) -> Vec<Value> {
    let env = config.get("env").and_then(Value::as_object);
    config
        .get("modelProviders")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|groups| groups.iter())
        .flat_map(|(protocol, entries)| {
            entries
                .as_array()
                .into_iter()
                .flatten()
                .map(move |provider| (protocol, provider))
        })
        .filter_map(|(protocol, provider)| {
            let id = provider.get("id").and_then(Value::as_str)?;
            let env_key = provider.get("envKey").and_then(Value::as_str);
            let secret = env_key.and_then(|key| env.and_then(|values| values.get(key))).and_then(Value::as_str);
            Some(json!({
                "id": id,
                "name": provider.get("name").and_then(Value::as_str).unwrap_or(id),
                "protocol": normalize_protocol(protocol),
                "baseUrl": provider.get("baseUrl"),
                "apiKey": if secret.is_some() { masked_secret(secret) } else { masked_secret(env_key.map(|key| format!("${key}")).as_deref()) },
                "defaultModel": Value::Null,
                "models": [id],
                "enabled": true
            }))
        })
        .collect()
}

fn models(config: &Value) -> Value {
    json!({
        "defaultModel": config.pointer("/model/name"),
        "smallModel": Value::Null,
        "reasoningModel": Value::Null,
        "reasoningEffort": Value::Null,
        "availableModels": providers(config).iter().filter_map(|provider| provider.get("id").and_then(Value::as_str)).collect::<Vec<_>>(),
        "aliases": {}
    })
}

fn environment(params: &Value, config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    let variables = config
        .get("env")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|env| env.keys())
        .map(
            |name| json!({ "name": name, "secret": looks_secret(name), "source": "settings.json" }),
        )
        .collect::<Vec<_>>();
    json!({
        "configFiles": [{
            "id": "settings", "label": "settings.json", "path": "~/.qwen/settings.json",
            "exists": !config.as_object().is_none_or(Map::is_empty)
        }],
        "variables": variables,
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "MCP server")?;
    let servers = object_field(config, "mcpServers")?;
    if action == "remove" {
        servers.remove(id);
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported Qwen MCP action: {action}"));
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
    if let Some(name) = non_empty(payload.get("name")) {
        server.insert("description".into(), json!(name));
    }
    if transport == "stdio" {
        server.insert(
            "command".into(),
            json!(non_empty(payload.get("command")).unwrap_or_default()),
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
        server.remove("httpUrl");
        server.remove("headers");
    } else {
        let url = json!(non_empty(payload.get("url")).unwrap_or_default());
        if transport == "http" {
            server.insert("httpUrl".into(), url);
            server.remove("url");
        } else {
            server.insert("url".into(), url);
            server.remove("httpUrl");
        }
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
    let id = entity_id(mutation, "Provider")?;
    let payload = mutation.get("payload");
    let requested_protocol = payload
        .and_then(|value| value.get("protocol"))
        .and_then(Value::as_str)
        .map(provider_group)
        .unwrap_or("openai");

    let groups = object_field(config, "modelProviders")?;
    let mut previous = None;
    for entries in groups.values_mut().filter_map(Value::as_array_mut) {
        if let Some(index) = entries
            .iter()
            .position(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        {
            previous = Some(entries.remove(index));
        }
    }
    if action == "remove" {
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported Qwen provider action: {action}"));
    }
    let payload = payload.ok_or("Provider payload is required")?;
    let mut provider = previous
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    provider.insert("id".into(), json!(id));
    provider.insert(
        "name".into(),
        json!(non_empty(payload.get("name")).unwrap_or(id)),
    );
    if let Some(base_url) = non_empty(payload.get("baseUrl")) {
        provider.insert("baseUrl".into(), json!(base_url));
    } else {
        provider.remove("baseUrl");
    }
    let env_key = non_empty(payload.get("apiKeyEnvironmentVariable"))
        .or_else(|| provider.get("envKey").and_then(Value::as_str))
        .unwrap_or("OPENAI_API_KEY")
        .to_string();
    provider.insert("envKey".into(), json!(env_key));
    let entries = groups
        .entry(requested_protocol.to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Qwen modelProviders group must be an array")?;
    entries.push(Value::Object(provider));

    if let Some(key) = non_empty(payload.get("apiKeyValue")) {
        object_field(config, "env")?.insert(env_key, json!(key));
    }
    Ok(())
}

fn mutate_models(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Model settings payload is required")?;
    let model = object_field(config, "model")?;
    match non_empty(payload.get("defaultModel")) {
        Some(value) => {
            model.insert("name".into(), json!(value));
        }
        None => {
            model.remove("name");
        }
    }
    Ok(())
}

fn normalize_protocol(value: &str) -> &str {
    match value {
        "openai" | "anthropic" | "gemini" => value,
        _ => "custom",
    }
}

fn provider_group(value: &str) -> &str {
    match value {
        "anthropic" => "anthropic",
        "gemini" => "gemini",
        _ => "openai",
    }
}

fn object_field<'a>(
    config: &'a mut Value,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let root = config
        .as_object_mut()
        .ok_or("Qwen settings must be an object")?;
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), json!({}));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("Qwen settings.{key} must be an object"))
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Qwen Code {key} is required"))
}

fn entity_id<'a>(mutation: &'a Value, label: &str) -> Result<&'a str, String> {
    mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| mutation.pointer("/payload/id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} id is required"))
}

fn non_empty(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn ensure_revision(expected: &str, current: &str) -> Result<(), String> {
    if expected == current {
        Ok(())
    } else {
        Err(format!(
            "CONFIG_CONFLICT: configuration changed on disk (expected {expected}, found {current})"
        ))
    }
}

fn looks_secret(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    name.contains("KEY") || name.contains("TOKEN") || name.contains("SECRET")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_update_preserves_unknown_entry_and_unrelated_groups() {
        let mut config = json!({
            "modelProviders": {
                "openai": [{ "id": "demo", "name": "Old", "envKey": "DEMO_KEY", "unknown": 7 }],
                "anthropic": [{ "id": "claude", "name": "Claude", "envKey": "ANTHROPIC_KEY" }]
            },
            "env": { "DEMO_KEY": "secret", "UNRELATED": "keep" },
            "ui": { "theme": "dark" }
        });
        mutate_provider(&mut config, "upsert", &json!({
            "entityId": "demo",
            "payload": { "id": "demo", "name": "Demo", "protocol": "openai", "baseUrl": "https://example.com", "apiKeyEnvironmentVariable": "DEMO_KEY" }
        })).unwrap();
        assert_eq!(config["modelProviders"]["openai"][0]["unknown"], 7);
        assert_eq!(config["modelProviders"]["anthropic"][0]["id"], "claude");
        assert_eq!(config["env"]["DEMO_KEY"], "secret");
        assert_eq!(config["env"]["UNRELATED"], "keep");
        assert_eq!(config["ui"]["theme"], "dark");
    }

    #[test]
    fn mcp_update_preserves_qwen_specific_fields() {
        let mut config = json!({ "mcpServers": { "demo": { "command": "old", "trust": true, "timeout": 3000 } } });
        mutate_mcp(&mut config, "upsert", &json!({
            "entityId": "demo", "payload": { "id": "demo", "transport": "stdio", "command": "new", "args": [], "env": {} }
        })).unwrap();
        assert_eq!(config["mcpServers"]["demo"]["trust"], true);
        assert_eq!(config["mcpServers"]["demo"]["timeout"], 3000);
    }
}
