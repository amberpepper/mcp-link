use mcp_link_agent_wasm_sdk::{
    management_section, management_section_descriptor, masked_secret, ConfigDocument, Host,
};
use serde_json::{json, Map, Value};

const SETTINGS: &str = "settings";
const STATE: &str = "state";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params
        .get("instance")
        .ok_or("Claude Code instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "claude-code",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", true), section("mcp", false),
            section("skills", false), section("prompts", false), section("providers", false),
            section("models", false), section("permissions", false),
            section("environment", true), section("raw-config", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let section_id = params
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Claude Code management section is required")?;
    let settings_doc = Host::config_read(SETTINGS, "")?;
    let state_doc = Host::config_read(STATE, "")?;
    let settings = parse(&settings_doc, "settings.json")?;
    let state = parse(&state_doc, ".claude.json")?;
    let (revision, data) = match section_id {
        "overview" => (&settings_doc.revision, overview(params, &settings, &state)),
        "mcp" => (
            &state_doc.revision,
            json!({ "servers": mcp_servers(&state) }),
        ),
        "providers" => (
            &settings_doc.revision,
            json!({ "providers": providers(&settings) }),
        ),
        "models" => (&settings_doc.revision, models(&settings)),
        "permissions" => (&settings_doc.revision, permissions(&settings)),
        "environment" => (
            &settings_doc.revision,
            environment(params, &settings, &state),
        ),
        _ => {
            return Err(format!(
                "Unsupported Claude Code management section: {section_id}"
            ))
        }
    };
    Ok(management_section(section_id, revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Claude Code management mutation is required")?;
    let section_id = mutation
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Claude Code mutation section is required")?;
    let action = mutation
        .get("action")
        .and_then(Value::as_str)
        .ok_or("Claude Code mutation action is required")?;
    let expected = mutation
        .get("expectedRevision")
        .and_then(Value::as_str)
        .ok_or("Claude Code expectedRevision is required")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (resource, label) = if section_id == "mcp" {
        (STATE, ".claude.json")
    } else {
        (SETTINGS, "settings.json")
    };
    let source = Host::config_read(resource, "")?;
    if source.revision != expected {
        return Err(format!(
            "CONFIG_CONFLICT: configuration changed on disk (expected {expected}, found {})",
            source.revision
        ));
    }
    let mut config = parse(&source, label)?;
    match section_id {
        "mcp" => mutate_mcp(&mut config, action, mutation)?,
        "providers" => mutate_provider(&mut config, mutation)?,
        "models" => mutate_models(&mut config, mutation)?,
        "permissions" => mutate_permissions(&mut config, mutation)?,
        _ => return Err(format!("Claude Code section is read-only: {section_id}")),
    }
    let mut content = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;
    content.push('\n');
    let revision = if dry_run {
        source.revision
    } else {
        Host::config_write_atomic(resource, "", &content, expected)?
            .get("revision")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or("Host file.writeAtomic returned no revision")?
    };
    Ok(json!({
        "section": section_id, "revision": revision,
        "changed": content != source.content,
        "changedResources": [label], "restartRequired": false,
        "warnings": []
    }))
}

fn parse(document: &ConfigDocument, label: &str) -> Result<Value, String> {
    if document.content.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&document.content).map_err(|error| format!("Invalid {label}: {error}"))
}

fn section(id: &str, read_only: bool) -> Value {
    let source = if matches!(id, "skills" | "prompts" | "raw-config") {
        "host"
    } else {
        "plugin"
    };
    management_section_descriptor(id, id, source, read_only)
}

fn overview(params: &Value, settings: &Value, state: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "Claude Code",
        "configRoot": instance.get("cliRoot"), "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"), "defaultModel": settings.get("model"),
        "defaultProvider": "anthropic", "mcpServerCount": mcp_servers(state).len(),
        "providerCount": providers(settings).len(), "skillTargetCount": 1, "warnings": []
    })
}

fn mcp_servers(state: &Value) -> Vec<Value> {
    state.get("mcpServers").and_then(Value::as_object).into_iter().flat_map(|items| items.iter()).map(|(id, server)| {
        let url = server.get("url").and_then(Value::as_str);
        json!({
            "id": id, "name": id, "transport": if url.is_some() { "http" } else { "stdio" },
            "command": server.get("command"), "args": server.get("args").cloned().unwrap_or_else(|| json!([])),
            "url": url, "env": server.get("env").cloned().unwrap_or_else(|| json!({})),
            "headers": server.get("headers").cloned().unwrap_or_else(|| json!({})),
            "enabled": !server.get("disabled").and_then(Value::as_bool).unwrap_or(false), "scope": "global"
        })
    }).collect()
}

fn providers(settings: &Value) -> Vec<Value> {
    let env = settings.get("env").and_then(Value::as_object);
    let injected = env
        .and_then(|items| items.get("MCP_LINK_GATEWAY_INJECTED"))
        .and_then(Value::as_str)
        == Some("true");
    let active_token = env
        .and_then(|items| {
            items
                .get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| items.get("ANTHROPIC_API_KEY"))
        })
        .and_then(Value::as_str);
    let original_token = if injected {
        env.and_then(|items| {
            items
                .get("MCP_LINK_PREVIOUS_ANTHROPIC_AUTH_TOKEN")
                .or_else(|| items.get("ANTHROPIC_API_KEY"))
        })
        .and_then(Value::as_str)
    } else {
        active_token
    };
    let original_base_url = env.and_then(|items| {
        items.get(if injected {
            "MCP_LINK_PREVIOUS_ANTHROPIC_BASE_URL"
        } else {
            "ANTHROPIC_BASE_URL"
        })
    });
    let mut result = vec![json!({
        "id": "anthropic", "name": "Anthropic", "protocol": "anthropic",
        "baseUrl": original_base_url, "apiKey": masked_secret(original_token),
        "defaultModel": settings.get("model"),
        "models": [], "enabled": true
    })];
    if injected {
        result.push(json!({
            "id": "mcp-link", "name": "MCP Link Gateway", "protocol": "anthropic",
            "baseUrl": env.and_then(|items| items.get("ANTHROPIC_BASE_URL")),
            "apiKey": masked_secret(active_token), "defaultModel": settings.get("model"),
            "models": [], "enabled": true
        }));
    }
    result
}

fn models(settings: &Value) -> Value {
    let env = settings.get("env").and_then(Value::as_object);
    json!({
        "defaultModel": settings.get("model"),
        "smallModel": env.and_then(|items| items.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")),
        "reasoningModel": Value::Null, "reasoningEffort": settings.get("effortLevel"),
        "availableModels": [], "aliases": {}
    })
}

fn permissions(settings: &Value) -> Value {
    let permissions = settings.get("permissions").and_then(Value::as_object);
    let mut rules = Vec::new();
    for (key, decision) in [("allow", "allow"), ("ask", "ask"), ("deny", "deny")] {
        for target in permissions
            .and_then(|items| items.get(key))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            rules.push(json!({ "id": format!("{decision}:{target}"), "decision": decision, "target": target, "kind": "tool" }));
        }
    }
    json!({ "approvalMode": Value::Null, "sandboxMode": Value::Null, "rules": rules })
}

fn environment(params: &Value, settings: &Value, state: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    let variables = settings.get("env").and_then(Value::as_object).into_iter().flat_map(|items| items.iter()).map(|(name, value)| {
        let secret = is_secret_name(name);
        json!({ "name": name, "value": if secret { Some("••••••••") } else { value.as_str() }, "secret": secret, "source": "settings.json" })
    }).collect::<Vec<_>>();
    json!({
        "configFiles": [
            { "id": "settings", "label": "settings.json", "path": "~/.claude/settings.json", "exists": !settings.as_object().is_none_or(Map::is_empty) },
            { "id": "state", "label": ".claude.json", "path": "~/.claude.json", "exists": !state.as_object().is_none_or(Map::is_empty) }
        ],
        "variables": variables, "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"), "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "MCP server")?;
    let servers = object_field(config, "mcpServers")?;
    if action == "remove" {
        servers.remove(id);
        return Ok(());
    }
    let payload = mutation
        .get("payload")
        .ok_or("MCP server payload is required")?;
    let previous = servers.get(id).cloned().unwrap_or_else(|| json!({}));
    let mut server = previous.as_object().cloned().unwrap_or_default();
    let transport = payload
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    if transport == "stdio" {
        set_optional(&mut server, "command", payload.get("command"));
        server.insert(
            "args".to_string(),
            payload.get("args").cloned().unwrap_or_else(|| json!([])),
        );
        server.remove("url");
    } else {
        set_optional(&mut server, "url", payload.get("url"));
        server.remove("command");
        server.remove("args");
    }
    server.insert(
        "env".to_string(),
        payload.get("env").cloned().unwrap_or_else(|| json!({})),
    );
    server.insert(
        "headers".to_string(),
        payload.get("headers").cloned().unwrap_or_else(|| json!({})),
    );
    server.insert(
        "disabled".to_string(),
        json!(!payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)),
    );
    servers.insert(id.to_string(), Value::Object(server));
    Ok(())
}

fn mutate_provider(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let env = object_field(config, "env")?;
    let provider_id = mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| payload.get("id").and_then(Value::as_str));
    let is_gateway = provider_id == Some("mcp-link");
    if mutation.get("action").and_then(Value::as_str) == Some("remove") {
        if !is_gateway {
            return Err("Claude Code's built-in Anthropic provider cannot be removed".to_string());
        }
        restore_env_value(
            env,
            "ANTHROPIC_BASE_URL",
            "MCP_LINK_PREVIOUS_ANTHROPIC_BASE_URL",
        );
        restore_env_value(
            env,
            "ANTHROPIC_AUTH_TOKEN",
            "MCP_LINK_PREVIOUS_ANTHROPIC_AUTH_TOKEN",
        );
        env.remove("MCP_LINK_GATEWAY_INJECTED");
        return Ok(());
    }
    let already_injected =
        env.get("MCP_LINK_GATEWAY_INJECTED").and_then(Value::as_str) == Some("true");
    if !is_gateway && already_injected {
        set_optional(
            env,
            "MCP_LINK_PREVIOUS_ANTHROPIC_BASE_URL",
            payload.get("baseUrl"),
        );
        if let Some(key) = payload
            .get("apiKeyValue")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            env.insert(
                "MCP_LINK_PREVIOUS_ANTHROPIC_AUTH_TOKEN".to_string(),
                json!(key),
            );
        }
        return Ok(());
    }
    if is_gateway && !already_injected {
        backup_env_value(
            env,
            "ANTHROPIC_BASE_URL",
            "MCP_LINK_PREVIOUS_ANTHROPIC_BASE_URL",
        );
        backup_env_value(
            env,
            "ANTHROPIC_AUTH_TOKEN",
            "MCP_LINK_PREVIOUS_ANTHROPIC_AUTH_TOKEN",
        );
    }
    set_optional(env, "ANTHROPIC_BASE_URL", payload.get("baseUrl"));
    if is_gateway {
        env.insert("MCP_LINK_GATEWAY_INJECTED".to_string(), json!("true"));
    }
    if let Some(key) = payload
        .get("apiKeyValue")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), json!(key));
    }
    Ok(())
}

fn backup_env_value(env: &mut Map<String, Value>, key: &str, backup: &str) {
    if let Some(previous) = env.get(key).cloned() {
        env.insert(backup.to_string(), previous);
    } else {
        env.remove(backup);
    }
}

fn restore_env_value(env: &mut Map<String, Value>, key: &str, backup: &str) {
    if let Some(previous) = env.remove(backup) {
        env.insert(key.to_string(), previous);
    } else {
        env.remove(key);
    }
}

fn mutate_models(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Model settings payload is required")?;
    let root = config
        .as_object_mut()
        .ok_or("Claude settings must be an object")?;
    set_optional(root, "model", payload.get("defaultModel"));
    set_optional(root, "effortLevel", payload.get("reasoningEffort"));
    let env = object_field(config, "env")?;
    set_optional(
        env,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        payload.get("smallModel"),
    );
    Ok(())
}

fn mutate_permissions(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let rules = mutation
        .get("payload")
        .and_then(|value| value.get("rules"))
        .and_then(Value::as_array)
        .ok_or("Permission rules are required")?;
    let permissions = object_field(config, "permissions")?;
    for decision in ["allow", "ask", "deny"] {
        permissions.insert(
            decision.to_string(),
            Value::Array(
                rules
                    .iter()
                    .filter(|rule| rule.get("decision").and_then(Value::as_str) == Some(decision))
                    .filter_map(|rule| {
                        rule.get("target")
                            .and_then(Value::as_str)
                            .map(|target| json!(target))
                    })
                    .collect(),
            ),
        );
    }
    Ok(())
}

fn object_field<'a>(
    config: &'a mut Value,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let root = config
        .as_object_mut()
        .ok_or("Claude configuration must be an object")?;
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), json!({}));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("Claude {key} must be an object"))
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

fn entity_id<'a>(mutation: &'a Value, label: &str) -> Result<&'a str, String> {
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
        .ok_or_else(|| format!("{label} id is required"))
}

fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD"]
        .iter()
        .any(|marker| upper.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_read_masks_key_without_returning_plaintext() {
        let settings = json!({ "env": {
            "ANTHROPIC_AUTH_TOKEN": "test-secret",
            "ANTHROPIC_BASE_URL": "https://example.com"
        } });
        let values = providers(&settings);
        assert_eq!(values[0]["apiKey"]["configured"], true);
        assert!(!serde_json::to_string(&values)
            .unwrap()
            .contains("test-secret"));
    }

    #[test]
    fn provider_update_writes_key_and_blank_preserves_it() {
        let mut settings = json!({ "env": {} });
        mutate_provider(
            &mut settings,
            &json!({ "payload": { "apiKeyValue": "  new-secret  " } }),
        )
        .unwrap();
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], "new-secret");
        mutate_provider(&mut settings, &json!({ "payload": { "apiKeyValue": "" } })).unwrap();
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], "new-secret");
    }

    #[test]
    fn gateway_injection_restores_previous_provider_settings() {
        let mut settings = json!({ "env": {
            "ANTHROPIC_AUTH_TOKEN": "original-key",
            "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
        } });
        mutate_provider(
            &mut settings,
            &json!({ "action": "upsert", "entityId": "mcp-link", "payload": {
                "apiKeyValue": "gateway-key",
                "baseUrl": "http://127.0.0.1:3285/anthropic"
            } }),
        )
        .unwrap();
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], "gateway-key");
        let values = providers(&settings);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["id"], "anthropic");
        assert_eq!(values[0]["baseUrl"], "https://api.anthropic.com");
        assert_eq!(values[1]["id"], "mcp-link");
        assert_eq!(values[1]["baseUrl"], "http://127.0.0.1:3285/anthropic");
        mutate_provider(
            &mut settings,
            &json!({ "action": "remove", "entityId": "mcp-link" }),
        )
        .unwrap();
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], "original-key");
        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            "https://api.anthropic.com"
        );
        assert!(settings["env"].get("MCP_LINK_GATEWAY_INJECTED").is_none());
        assert_eq!(providers(&settings).len(), 1);
    }

    #[test]
    fn normal_provider_edit_does_not_create_gateway_backup_state() {
        let mut settings = json!({ "env": {
            "ANTHROPIC_AUTH_TOKEN": "original-key",
            "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
        } });
        mutate_provider(
            &mut settings,
            &json!({ "action": "upsert", "entityId": "anthropic", "payload": {
                "apiKeyValue": "updated-key",
                "baseUrl": "https://api.example.com"
            } }),
        )
        .unwrap();
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], "updated-key");
        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            "https://api.example.com"
        );
        assert!(settings["env"].get("MCP_LINK_GATEWAY_INJECTED").is_none());
        assert!(settings["env"]
            .get("MCP_LINK_PREVIOUS_ANTHROPIC_BASE_URL")
            .is_none());
        assert!(settings["env"]
            .get("MCP_LINK_PREVIOUS_ANTHROPIC_AUTH_TOKEN")
            .is_none());
    }

    #[test]
    fn original_provider_can_be_edited_while_gateway_is_active() {
        let mut settings = json!({ "env": {
            "ANTHROPIC_AUTH_TOKEN": "original-key",
            "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
        } });
        mutate_provider(
            &mut settings,
            &json!({ "action": "upsert", "entityId": "mcp-link", "payload": {
                "apiKeyValue": "gateway-key",
                "baseUrl": "http://127.0.0.1:3285/anthropic"
            } }),
        )
        .unwrap();
        mutate_provider(
            &mut settings,
            &json!({ "action": "upsert", "entityId": "anthropic", "payload": {
                "apiKeyValue": "updated-original-key",
                "baseUrl": "https://api.example.com"
            } }),
        )
        .unwrap();
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], "gateway-key");
        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            "http://127.0.0.1:3285/anthropic"
        );
        assert_eq!(
            settings["env"]["MCP_LINK_PREVIOUS_ANTHROPIC_AUTH_TOKEN"],
            "updated-original-key"
        );
        assert_eq!(
            settings["env"]["MCP_LINK_PREVIOUS_ANTHROPIC_BASE_URL"],
            "https://api.example.com"
        );

        mutate_provider(
            &mut settings,
            &json!({ "action": "remove", "entityId": "mcp-link" }),
        )
        .unwrap();
        assert_eq!(
            settings["env"]["ANTHROPIC_AUTH_TOKEN"],
            "updated-original-key"
        );
        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            "https://api.example.com"
        );
    }

    #[test]
    fn masks_secret_environment_values() {
        assert!(is_secret_name("ANTHROPIC_API_KEY"));
        assert!(!is_secret_name("ANTHROPIC_BASE_URL"));
    }

    #[test]
    fn permission_mutation_preserves_other_settings() {
        let mut settings = json!({ "unknown": 42, "permissions": { "allow": ["Read"] } });
        mutate_permissions(
            &mut settings,
            &json!({ "payload": { "rules": [{ "decision": "deny", "target": "Bash" }] } }),
        )
        .unwrap();
        assert_eq!(settings["unknown"], 42);
        assert_eq!(settings["permissions"]["deny"][0], "Bash");
    }
}
