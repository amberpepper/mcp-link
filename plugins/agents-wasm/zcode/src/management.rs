use mcp_link_agent_wasm_sdk::{
    management_section, management_section_descriptor, masked_secret, read_json_config,
    write_json_config,
};
use serde_json::{json, Map, Value};

const CONFIG: &str = "config";
const CLI_CONFIG: &str = "cli-config";
const SETTINGS: &str = "settings";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params.get("instance").ok_or("ZCode instance is required")?;
    Ok(json!({
        "schemaVersion": 1, "agentId": "zcode", "instanceId": instance.get("id"),
        "sections": [
            section("overview", true), section("skills", false), section("mcp", false),
            section("providers", false),
            section("models", true), section("environment", true),
            section("raw-config", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let id = params
        .get("section")
        .and_then(Value::as_str)
        .ok_or("ZCode management section is required")?;
    let (config, config_revision) = read_json_config(CONFIG, "")?;
    let (cli_config, cli_config_revision) = read_json_config(CLI_CONFIG, "")?;
    let (settings, settings_revision) = read_json_config(SETTINGS, "")?;
    let (revision, data) = match id {
        "overview" => (&config_revision, overview(params, &config, &cli_config)),
        "mcp" => (
            &cli_config_revision,
            json!({ "servers": mcp_servers(&cli_config) }),
        ),
        "providers" => (&config_revision, json!({ "providers": providers(&config) })),
        "models" => (&config_revision, models(&config)),
        "environment" => (
            &settings_revision,
            environment(params, &config, &cli_config, &settings),
        ),
        _ => return Err(format!("Unsupported ZCode management section: {id}")),
    };
    Ok(management_section(id, revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("ZCode management mutation is required")?;
    let section_id = mutation
        .get("section")
        .and_then(Value::as_str)
        .ok_or("ZCode mutation section is required")?;
    let action = mutation
        .get("action")
        .and_then(Value::as_str)
        .ok_or("ZCode mutation action is required")?;
    let expected = mutation
        .get("expectedRevision")
        .and_then(Value::as_str)
        .ok_or("ZCode expectedRevision is required")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let resource = match section_id {
        "mcp" => CLI_CONFIG,
        "providers" => CONFIG,
        _ => return Err(format!("ZCode section is read-only: {section_id}")),
    };
    let (mut config, revision) = read_json_config(resource, "")?;
    if revision != expected {
        return Err(format!("CONFIG_CONFLICT: configuration changed on disk (expected {expected}, found {revision})"));
    }
    match section_id {
        "mcp" => mutate_mcp(&mut config, action, mutation)?,
        "providers" => mutate_provider(&mut config, action, mutation)?,
        _ => unreachable!(),
    }
    let next_revision = if dry_run {
        revision
    } else {
        write_json_config(resource, "", &config, expected)?
    };
    let changed_resource = if section_id == "mcp" {
        "cli/config.json"
    } else {
        "v2/config.json"
    };
    Ok(json!({
        "section": section_id, "revision": next_revision, "changed": true,
        "changedResources": [changed_resource], "restartRequired": true,
        "warnings": []
    }))
}

fn section(id: &str, read_only: bool) -> Value {
    let source = if matches!(id, "skills" | "raw-config") {
        "host"
    } else {
        "plugin"
    };
    management_section_descriptor(id, id, source, read_only)
}

fn overview(params: &Value, config: &Value, cli_config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "ZCode Desktop", "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"), "skillRoot": instance.get("skillRoot"),
        "defaultModel": Value::Null, "defaultProvider": Value::Null,
        "mcpServerCount": mcp_servers(cli_config).len(), "providerCount": providers(config).len(),
        "skillTargetCount": 2, "warnings": []
    })
}

fn mcp_servers(config: &Value) -> Vec<Value> {
    config
        .pointer("/mcp/servers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|servers| servers.iter())
        .map(|(id, server)| {
            let native_type = server
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let transport = match native_type.as_str() {
                "sse" => "sse",
                "http" | "streamable-http" | "streamable_http" => "http",
                _ if server.get("url").and_then(Value::as_str).is_some() => "http",
                _ => "stdio",
            };
            json!({
                "id": id,
                "name": id,
                "transport": transport,
                "command": server.get("command"),
                "args": server.get("args").cloned().unwrap_or_else(|| json!([])),
                "url": server.get("url"),
                "env": server.get("env").cloned().unwrap_or_else(|| json!({})),
                "headers": server.get("headers").cloned().unwrap_or_else(|| json!({})),
                "enabled": server.get("enable").and_then(Value::as_bool).unwrap_or(true),
                "scope": "global"
            })
        })
        .collect()
}

fn providers(config: &Value) -> Vec<Value> {
    config.get("provider").and_then(Value::as_object).into_iter().flat_map(|items| items.iter()).map(|(id, provider)| {
        let options = provider.get("options").and_then(Value::as_object);
        let models = provider.get("models").and_then(Value::as_object).map(|items| items.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
        json!({
            "id": id, "name": provider.get("name").and_then(Value::as_str).unwrap_or(id),
            "protocol": protocol(provider.get("kind").and_then(Value::as_str)),
            "baseUrl": options.and_then(|items| items.get("baseURL").or_else(|| items.get("baseUrl"))),
            "apiKey": masked_secret(options.and_then(|items| items.get("apiKey").or_else(|| items.get("api_key"))).and_then(Value::as_str)),
            "defaultModel": models.first(), "models": models,
            "enabled": provider.get("enabled").and_then(Value::as_bool).unwrap_or(true)
        })
    }).collect()
}

fn models(config: &Value) -> Value {
    let available = config
        .get("provider")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|items| items.iter())
        .flat_map(|(provider, value)| {
            value
                .get("models")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(move |models| {
                    models
                        .keys()
                        .map(move |model| format!("{provider}/{model}"))
                })
        })
        .collect::<Vec<_>>();
    json!({ "defaultModel": Value::Null, "smallModel": Value::Null, "reasoningModel": Value::Null, "reasoningEffort": Value::Null, "availableModels": available, "aliases": {} })
}

fn environment(params: &Value, config: &Value, cli_config: &Value, settings: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "configFiles": [
            { "id": "config", "label": "config.json", "path": "~/.zcode/v2/config.json", "exists": !config.as_object().is_none_or(Map::is_empty) },
            { "id": "cli-config", "label": "CLI config.json", "path": "~/.zcode/cli/config.json", "exists": !cli_config.as_object().is_none_or(Map::is_empty) },
            { "id": "settings", "label": "setting.json", "path": "~/.zcode/v2/setting.json", "exists": !settings.as_object().is_none_or(Map::is_empty) }
        ],
        "variables": [], "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"), "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| mutation.pointer("/payload/id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or("MCP server id is required")?;
    let root = config
        .as_object_mut()
        .ok_or("ZCode CLI configuration must be an object")?;
    if !root.get("mcp").is_some_and(Value::is_object) {
        root.insert("mcp".to_string(), json!({}));
    }
    let mcp = root
        .get_mut("mcp")
        .and_then(Value::as_object_mut)
        .ok_or("ZCode mcp must be an object")?;
    if !mcp.get("servers").is_some_and(Value::is_object) {
        mcp.insert("servers".to_string(), json!({}));
    }
    let servers = mcp
        .get_mut("servers")
        .and_then(Value::as_object_mut)
        .ok_or("ZCode mcp.servers must be an object")?;
    if action == "remove" {
        servers.remove(id);
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported ZCode MCP action: {action}"));
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
    server.insert(
        "enable".to_string(),
        json!(payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)),
    );
    if transport == "stdio" {
        server.remove("type");
        server.insert(
            "command".to_string(),
            json!(payload
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()),
        );
        server.insert(
            "args".to_string(),
            payload.get("args").cloned().unwrap_or_else(|| json!([])),
        );
        server.insert(
            "env".to_string(),
            payload.get("env").cloned().unwrap_or_else(|| json!({})),
        );
        server.remove("url");
        server.remove("headers");
    } else {
        server.insert("type".to_string(), json!(transport));
        server.insert(
            "url".to_string(),
            payload.get("url").cloned().unwrap_or(Value::Null),
        );
        server.insert(
            "headers".to_string(),
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
    let id = mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| {
            mutation
                .get("payload")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
        })
        .filter(|id| !id.is_empty())
        .ok_or("Provider id is required")?;
    let providers = object_field(config, "provider")?;
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
    set_provider_protocol(&mut provider, payload.get("protocol"));
    provider
        .entry("source".to_string())
        .or_insert_with(|| json!("user"));
    provider.insert(
        "enabled".to_string(),
        json!(payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)),
    );
    let mut options = provider
        .remove("options")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    set_optional(&mut options, "baseURL", payload.get("baseUrl"));
    if let Some(key) = payload
        .get("apiKeyValue")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        options.insert("apiKey".to_string(), json!(key));
        options
            .entry("apiKeyRequired".to_string())
            .or_insert_with(|| json!(true));
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
                                .unwrap_or_else(|| json!({})),
                        )
                    })
                    .collect(),
            ),
        );
    }
    providers.insert(id.to_string(), Value::Object(provider));
    Ok(())
}

fn protocol(kind: Option<&str>) -> &str {
    match kind.unwrap_or_default().to_ascii_lowercase().as_str() {
        "anthropic" => "anthropic",
        "gemini" => "gemini",
        "openai" | "openai-compatible" | "openai_compatible" => "openai",
        _ => "custom",
    }
}

fn set_provider_protocol(provider: &mut Map<String, Value>, input: Option<&Value>) {
    let requested = input.and_then(Value::as_str).unwrap_or("openai");
    let previous = provider.get("kind").and_then(Value::as_str);
    if previous.is_some_and(|kind| protocol(Some(kind)) == requested) {
        return;
    }
    let kind = match requested {
        "anthropic" | "gemini" | "openai" => requested,
        "custom" => previous.unwrap_or("openai"),
        _ => "openai",
    };
    provider.insert("kind".to_string(), json!(kind));
}

fn object_field<'a>(
    config: &'a mut Value,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let root = config
        .as_object_mut()
        .ok_or("ZCode configuration must be an object")?;
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), json!({}));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("ZCode {key} must be an object"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_read_masks_keys_and_classifies_openai_compatible() {
        let config = json!({ "provider": { "custom": {
            "name": "Custom", "kind": "openai-compatible",
            "options": { "apiKey": "test-secret", "baseURL": "https://example.com/v1" },
            "models": {}
        } } });
        let values = providers(&config);
        assert_eq!(values[0]["protocol"], "openai");
        assert_eq!(values[0]["apiKey"]["configured"], true);
        assert_eq!(values[0]["apiKey"]["source"], "inline");
        assert!(!serde_json::to_string(&values)
            .unwrap()
            .contains("test-secret"));
    }

    #[test]
    fn provider_update_writes_key_protocol_and_required_flag() {
        let mut config = json!({ "provider": {} });
        mutate_provider(
            &mut config,
            "upsert",
            &json!({ "payload": {
                "id": "custom", "name": "Custom", "protocol": "anthropic",
                "apiKeyValue": "new-secret", "baseUrl": "https://example.com"
            } }),
        )
        .unwrap();
        assert_eq!(config["provider"]["custom"]["kind"], "anthropic");
        assert_eq!(
            config["provider"]["custom"]["options"]["apiKey"],
            "new-secret"
        );
        assert_eq!(
            config["provider"]["custom"]["options"]["apiKeyRequired"],
            true
        );
    }

    #[test]
    fn provider_update_preserves_model_metadata_and_secret() {
        let mut config = json!({ "provider": { "custom": { "kind": "openai-compatible", "options": { "apiKey": "secret" }, "models": { "m": { "limit": { "context": 10 } } }, "unknown": 1 } } });
        mutate_provider(&mut config, "upsert", &json!({ "entityId": "custom", "payload": { "id": "custom", "name": "Custom", "protocol": "openai", "models": ["m"], "apiKeyValue": "" } })).unwrap();
        assert_eq!(config["provider"]["custom"]["options"]["apiKey"], "secret");
        assert_eq!(config["provider"]["custom"]["kind"], "openai-compatible");
        assert_eq!(
            config["provider"]["custom"]["models"]["m"]["limit"]["context"],
            10
        );
        assert_eq!(config["provider"]["custom"]["unknown"], 1);
    }

    #[test]
    fn mcp_reads_native_servers_and_enable_flag() {
        let config = json!({ "mcp": { "servers": {
            "local": { "command": "npx", "args": ["server"] },
            "remote": { "type": "sse", "url": "https://example.com/sse", "enable": false }
        } } });
        let servers = mcp_servers(&config);
        assert_eq!(servers[0]["transport"], "stdio");
        assert_eq!(servers[0]["enabled"], true);
        assert_eq!(servers[1]["transport"], "sse");
        assert_eq!(servers[1]["enabled"], false);
    }

    #[test]
    fn mcp_update_preserves_unknown_native_fields() {
        let mut config = json!({
            "unknownRoot": true,
            "mcp": { "unknownMcp": 1, "servers": { "demo": {
                "command": "old", "timeout": 30, "unknown": { "keep": true }
            } } }
        });
        mutate_mcp(
            &mut config,
            "upsert",
            &json!({ "entityId": "demo", "payload": {
                "id": "demo", "transport": "http", "url": "https://example.com/mcp",
                "headers": { "Authorization": "Bearer token" }, "enabled": false
            } }),
        )
        .unwrap();
        assert_eq!(config["unknownRoot"], true);
        assert_eq!(config["mcp"]["unknownMcp"], 1);
        assert_eq!(config["mcp"]["servers"]["demo"]["timeout"], 30);
        assert_eq!(config["mcp"]["servers"]["demo"]["unknown"]["keep"], true);
        assert_eq!(config["mcp"]["servers"]["demo"]["type"], "http");
        assert_eq!(config["mcp"]["servers"]["demo"]["enable"], false);
        assert!(config["mcp"]["servers"]["demo"].get("command").is_none());
    }
}
