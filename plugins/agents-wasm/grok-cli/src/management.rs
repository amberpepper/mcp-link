use mcp_link_agent_wasm_sdk::{
    finish_json_management_mutation, management_section, management_section_descriptor,
    read_json_document,
};
use serde_json::{json, Map, Value};

const SETTINGS: &str = "settings";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params
        .get("instance")
        .ok_or("Grok CLI instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "grok-cli",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", "overview", "plugin", true),
            section("mcp", "mcp", "plugin", false),
            section("skills", "skills", "host", false),
            section("prompts", "prompts", "host", false),
            api_section(),
            section("models", "models", "plugin", false),
            section("environment", "environment", "plugin", true),
            section("raw-config", "raw-config", "host", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let id = required_string(params, "section")?;
    let (document, config) = read_json_document(SETTINGS, "", "Grok user-settings.json")?;
    let data = match id {
        "overview" => overview(params, &config),
        "mcp" => json!({ "servers": mcp_servers(&config) }),
        "api" => api_settings(&config),
        "models" => model_settings(&config),
        "environment" => environment(params, &config),
        _ => return Err(format!("Unsupported Grok CLI management section: {id}")),
    };
    Ok(management_section(id, &document.revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Grok CLI management mutation is required")?;
    let section = required_string(mutation, "section")?;
    let action = required_string(mutation, "action")?;
    let expected = required_string(mutation, "expectedRevision")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (document, mut config) = read_json_document(SETTINGS, "", "Grok user-settings.json")?;
    ensure_revision(expected, &document.revision)?;
    match section {
        "mcp" => mutate_mcp(&mut config, action, mutation)?,
        "api" => mutate_api(&mut config, mutation)?,
        "models" => mutate_models(&mut config, mutation)?,
        _ => return Err(format!("Grok CLI section is read-only: {section}")),
    }
    finish_json_management_mutation(
        SETTINGS,
        "",
        section,
        "user-settings.json",
        &config,
        document,
        dry_run,
        section == "mcp",
    )
}

fn section(id: &str, renderer: &str, source: &str, read_only: bool) -> Value {
    management_section_descriptor(id, renderer, source, read_only)
}

fn api_section() -> Value {
    let mut value = management_section_descriptor("api", "form", "plugin", false);
    value["label"] = json!({ "zh": "API 设置", "en": "API settings", "ja": "API 設定" });
    value["description"] = json!({
        "zh": "管理 Grok CLI 原生 API Key。Base URL 由 GROK_BASE_URL 环境变量控制。",
        "en": "Manage Grok CLI's native API key. Base URL is controlled by GROK_BASE_URL.",
        "ja": "Grok CLI の API キーを管理します。Base URL は GROK_BASE_URL で指定します。"
    });
    value
}

fn overview(params: &Value, config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "Grok CLI",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "defaultModel": config.get("defaultModel"),
        "defaultProvider": "xai",
        "mcpServerCount": mcp_servers(config).len(),
        "providerCount": 1,
        "skillTargetCount": 2,
        "warnings": []
    })
}

fn mcp_servers(config: &Value) -> Vec<Value> {
    config
        .pointer("/mcp/servers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|server| {
            let id = server.get("id").and_then(Value::as_str)?;
            Some(json!({
                "id": id,
                "name": server.get("label").and_then(Value::as_str).unwrap_or(id),
                "transport": server.get("transport").and_then(Value::as_str).unwrap_or("stdio"),
                "command": server.get("command"),
                "args": server.get("args").cloned().unwrap_or_else(|| json!([])),
                "url": server.get("url"),
                "env": server.get("env").cloned().unwrap_or_else(|| json!({})),
                "headers": server.get("headers").cloned().unwrap_or_else(|| json!({})),
                "enabled": server.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                "scope": "global"
            }))
        })
        .collect()
}

fn api_settings(config: &Value) -> Value {
    let configured = config
        .get("apiKey")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    json!({
        "schemaVersion": 1,
        "groups": [{
            "id": "credentials",
            "title": { "zh": "xAI 凭据", "en": "xAI credentials", "ja": "xAI 認証情報" },
            "description": { "zh": "留空会保留已经保存的密钥。", "en": "Leave blank to keep the saved key.", "ja": "空欄の場合は保存済みキーを維持します。" },
            "fields": [{
                "key": "apiKey",
                "control": "password",
                "label": "API Key",
                "placeholder": if configured { "••••••••" } else { "xai-..." },
                "mono": true
            }]
        }],
        "values": { "apiKey": "" }
    })
}

fn model_settings(config: &Value) -> Value {
    let model = config.get("defaultModel").and_then(Value::as_str);
    json!({
        "defaultModel": model,
        "smallModel": Value::Null,
        "reasoningModel": Value::Null,
        "reasoningEffort": model.and_then(|id| config.get("reasoningEffortByModel").and_then(|map| map.get(id))).and_then(Value::as_str),
        "availableModels": [],
        "aliases": {}
    })
}

fn environment(params: &Value, config: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "configFiles": [{
            "id": "settings", "label": "user-settings.json", "path": "~/.grok/user-settings.json",
            "exists": !config.as_object().is_none_or(Map::is_empty)
        }],
        "variables": [
            { "name": "GROK_API_KEY", "secret": true, "source": "environment" },
            { "name": "GROK_BASE_URL", "secret": false, "source": "environment" },
            { "name": "GROK_MODEL", "secret": false, "source": "environment" }
        ],
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "MCP server")?;
    let servers = mcp_server_array(config)?;
    let index = servers
        .iter()
        .position(|server| server.get("id").and_then(Value::as_str) == Some(id));
    if action == "remove" {
        if let Some(index) = index {
            servers.remove(index);
        }
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported Grok MCP action: {action}"));
    }
    let payload = mutation
        .get("payload")
        .ok_or("MCP server payload is required")?;
    let transport = payload
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    let mut server = index
        .and_then(|index| servers.get(index))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    server.insert("id".into(), json!(id));
    server.insert(
        "label".into(),
        json!(non_empty(payload.get("name")).unwrap_or(id)),
    );
    server.insert(
        "enabled".into(),
        json!(payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)),
    );
    server.insert("transport".into(), json!(transport));
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
        server.remove("headers");
    } else {
        server.insert(
            "url".into(),
            json!(non_empty(payload.get("url")).unwrap_or_default()),
        );
        server.insert(
            "headers".into(),
            payload.get("headers").cloned().unwrap_or_else(|| json!({})),
        );
        server.remove("command");
        server.remove("args");
        server.remove("env");
    }
    let value = Value::Object(server);
    match index {
        Some(index) => servers[index] = value,
        None => servers.push(value),
    }
    Ok(())
}

fn mutate_api(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let key = mutation
        .pointer("/payload/values/apiKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(key) = key {
        config
            .as_object_mut()
            .ok_or("Grok settings must be an object")?
            .insert("apiKey".into(), json!(key));
    }
    Ok(())
}

fn mutate_models(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Grok model settings payload is required")?;
    let root = config
        .as_object_mut()
        .ok_or("Grok settings must be an object")?;
    let model = non_empty(payload.get("defaultModel"));
    match model {
        Some(model) => {
            root.insert("defaultModel".into(), json!(model));
        }
        None => {
            root.remove("defaultModel");
        }
    }
    if let (Some(model), Some(effort)) = (model, non_empty(payload.get("reasoningEffort"))) {
        let efforts = root
            .entry("reasoningEffortByModel")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("Grok reasoningEffortByModel must be an object")?;
        efforts.insert(model.to_string(), json!(effort));
    }
    Ok(())
}

fn mcp_server_array(config: &mut Value) -> Result<&mut Vec<Value>, String> {
    let root = config
        .as_object_mut()
        .ok_or("Grok settings must be an object")?;
    let mcp = root
        .entry("mcp")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Grok mcp must be an object")?;
    mcp.entry("servers")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| "Grok mcp.servers must be an array".to_string())
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Grok CLI {key} is required"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_update_preserves_unknown_server_and_settings_fields() {
        let mut config = json!({
            "mcp": {
                "servers": [{
                    "id": "demo", "label": "Old", "transport": "stdio",
                    "command": "old", "cwd": "C:/work", "unknown": 3
                }],
                "unknownMcp": true
            },
            "sandbox": { "network": false }
        });
        mutate_mcp(
            &mut config,
            "upsert",
            &json!({
                "entityId": "demo",
                "payload": {
                    "id": "demo", "name": "Demo", "transport": "stdio",
                    "command": "new", "args": [], "env": {}, "enabled": true
                }
            }),
        )
        .unwrap();
        assert_eq!(config["mcp"]["servers"][0]["cwd"], "C:/work");
        assert_eq!(config["mcp"]["servers"][0]["unknown"], 3);
        assert_eq!(config["mcp"]["unknownMcp"], true);
        assert_eq!(config["sandbox"]["network"], false);
    }

    #[test]
    fn blank_api_key_keeps_saved_secret() {
        let mut config = json!({ "apiKey": "secret", "unknown": true });
        mutate_api(
            &mut config,
            &json!({ "payload": { "values": { "apiKey": "" } } }),
        )
        .unwrap();
        assert_eq!(config["apiKey"], "secret");
        assert_eq!(config["unknown"], true);
    }
}
