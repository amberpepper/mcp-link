use mcp_link_agent_wasm_sdk::{
    finish_json_management_mutation, management_section, management_section_descriptor,
    read_json_document,
};
use serde_json::{json, Map, Value};

const SETTINGS: &str = "settings";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params
        .get("instance")
        .ok_or("Gemini CLI instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "gemini-cli",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", "overview", "plugin", true),
            section("mcp", "mcp", "plugin", false),
            section("skills", "skills", "host", false),
            section("prompts", "prompts", "host", false),
            section("models", "models", "plugin", false),
            section("environment", "environment", "plugin", true),
            section("raw-config", "raw-config", "host", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let id = params
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Gemini CLI management section is required")?;
    let (document, config) = read_json_document(SETTINGS, "", "Gemini settings.json")?;
    let data = match id {
        "overview" => overview(params, &config),
        "mcp" => json!({ "servers": mcp_servers(&config), "canDisable": false }),
        "models" => models(&config),
        "environment" => environment(params, &config),
        _ => return Err(format!("Unsupported Gemini CLI management section: {id}")),
    };
    Ok(management_section(id, &document.revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Gemini CLI management mutation is required")?;
    let section = required_string(mutation, "section")?;
    let action = required_string(mutation, "action")?;
    let expected = required_string(mutation, "expectedRevision")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (document, mut config) = read_json_document(SETTINGS, "", "Gemini settings.json")?;
    ensure_revision(expected, &document.revision)?;
    match section {
        "mcp" => mutate_mcp(&mut config, action, mutation)?,
        "models" => mutate_models(&mut config, mutation)?,
        _ => return Err(format!("Gemini CLI section is read-only: {section}")),
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
        "cliName": "Gemini CLI",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "defaultModel": config.pointer("/model/name"),
        "defaultProvider": "google",
        "mcpServerCount": mcp_servers(config).len(),
        "providerCount": 1,
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

fn models(config: &Value) -> Value {
    json!({
        "defaultModel": config.pointer("/model/name"),
        "smallModel": Value::Null,
        "reasoningModel": Value::Null,
        "reasoningEffort": Value::Null,
        "availableModels": [],
        "aliases": {}
    })
}

fn environment(params: &Value, config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "configFiles": [{
            "id": "settings",
            "label": "settings.json",
            "path": "~/.gemini/settings.json",
            "exists": !config.as_object().is_none_or(Map::is_empty)
        }],
        "variables": [
            { "name": "GEMINI_API_KEY", "secret": true, "source": "environment" },
            { "name": "GOOGLE_API_KEY", "secret": true, "source": "environment" },
            { "name": "GOOGLE_CLOUD_PROJECT", "secret": false, "source": "environment" }
        ],
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "MCP server")?;
    let servers = object_field(config, "mcpServers", "Gemini settings")?;
    if action == "remove" {
        servers.remove(id);
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported Gemini MCP action: {action}"));
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

fn mutate_models(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Gemini model settings payload is required")?;
    let model = object_field(config, "model", "Gemini settings")?;
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

fn object_field<'a>(
    config: &'a mut Value,
    key: &str,
    label: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let root = config
        .as_object_mut()
        .ok_or_else(|| format!("{label} must be an object"))?;
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), json!({}));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("{label}.{key} must be an object"))
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

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Gemini CLI {key} is required"))
}

fn non_empty(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_update_preserves_native_gemini_fields() {
        let mut config = json!({
            "mcpServers": { "demo": {
                "command": "old", "timeout": 42, "trust": true,
                "includeTools": ["search"]
            }},
            "general": { "vimMode": true }
        });
        mutate_mcp(&mut config, "upsert", &json!({
            "entityId": "demo",
            "payload": { "id": "demo", "name": "Demo", "transport": "stdio", "command": "npx", "args": ["demo"], "env": {} }
        })).unwrap();
        assert_eq!(config["mcpServers"]["demo"]["timeout"], 42);
        assert_eq!(config["mcpServers"]["demo"]["trust"], true);
        assert_eq!(config["mcpServers"]["demo"]["includeTools"][0], "search");
        assert_eq!(config["general"]["vimMode"], true);
    }

    #[test]
    fn model_update_preserves_other_model_settings() {
        let mut config = json!({ "model": { "name": "old", "maxSessionTurns": 12 } });
        mutate_models(
            &mut config,
            &json!({ "payload": { "defaultModel": "gemini-2.5-pro" } }),
        )
        .unwrap();
        assert_eq!(config["model"]["name"], "gemini-2.5-pro");
        assert_eq!(config["model"]["maxSessionTurns"], 12);
    }
}
