use mcp_link_agent_wasm_sdk::{
    management_section, management_section_descriptor, masked_secret, Host,
};
use serde_json::{json, Value};
use toml_edit::{value, DocumentMut, Item, Table};

const CONFIG: &str = "config";
const MCP: &str = "mcp";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params
        .get("instance")
        .ok_or("Kimi Code instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "kimi-code",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", "overview", "plugin", true),
            section("skills", "skills", "host", false),
            section("mcp", "mcp", "plugin", false),
            section("providers", "providers", "plugin", false),
            section("prompts", "prompts", "host", false),
            section("environment", "environment", "plugin", true),
            section("raw-config", "raw-config", "host", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let section_id = params
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Kimi Code management section is required")?;
    match section_id {
        "mcp" => load_mcp(),
        "providers" => load_providers(),
        "overview" | "environment" => {
            let config = Host::config_read(CONFIG, "")?;
            let data = if section_id == "overview" {
                overview(params, !config.content.trim().is_empty())
            } else {
                environment(params)?
            };
            Ok(management_section(section_id, &config.revision, data))
        }
        _ => Err(format!(
            "Unsupported Kimi Code management section: {section_id}"
        )),
    }
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Kimi Code management mutation is required")?;
    let section = mutation
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Kimi Code mutation section is required")?;
    let action = mutation
        .get("action")
        .and_then(Value::as_str)
        .ok_or("Kimi Code mutation action is required")?;
    let expected = mutation
        .get("expectedRevision")
        .and_then(Value::as_str)
        .ok_or("Kimi Code expectedRevision is required")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match section {
        "mcp" => mutate_mcp(mutation, action, expected, dry_run),
        "providers" => mutate_provider(mutation, action, expected, dry_run),
        _ => Err(format!("Kimi Code section is read-only: {section}")),
    }
}

fn load_mcp() -> Result<Value, String> {
    let document = Host::config_read(MCP, "")?;
    let config = parse_json(&document.content)?;
    let servers = config
        .get("mcpServers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|servers| servers.iter())
        .map(|(id, server)| {
            let url = server.get("url").and_then(Value::as_str);
            json!({
                "id": id,
                "name": id,
                "transport": if server.get("transport").and_then(Value::as_str) == Some("sse") { "sse" } else if url.is_some() { "http" } else { "stdio" },
                "command": server.get("command"),
                "args": server.get("args").cloned().unwrap_or_else(|| json!([])),
                "url": url,
                "env": server.get("env").cloned().unwrap_or_else(|| json!({})),
                "headers": server.get("headers").cloned().unwrap_or_else(|| json!({})),
                "enabled": server.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                "scope": "global"
            })
        })
        .collect::<Vec<_>>();
    Ok(management_section(
        "mcp",
        &document.revision,
        json!({ "servers": servers }),
    ))
}

fn load_providers() -> Result<Value, String> {
    let document = Host::config_read(CONFIG, "")?;
    let config = parse_toml(&document.content)?;
    let providers = config
        .get("providers")
        .and_then(Item::as_table_like)
        .into_iter()
        .flat_map(|providers| providers.iter())
        .filter_map(|(id, item)| {
            let provider = item.as_table_like()?;
            let models = config
                .get("models")
                .and_then(Item::as_table_like)
                .into_iter()
                .flat_map(|models| models.iter())
                .filter(|(_, model)| model.get("provider").and_then(Item::as_str) == Some(id))
                .filter_map(|(_, model)| model.get("model").and_then(Item::as_str))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            Some(json!({
                "id": id,
                "name": id,
                "protocol": provider.get("type").and_then(Item::as_str).unwrap_or("openai"),
                "baseUrl": provider.get("base_url").and_then(Item::as_str),
                "apiKey": masked_secret(provider.get("api_key").and_then(Item::as_str)),
                "models": models,
                "enabled": true
            }))
        })
        .collect::<Vec<_>>();
    Ok(management_section(
        "providers",
        &document.revision,
        json!({ "providers": providers, "secretInput": { "mode": "value" } }),
    ))
}

fn mutate_mcp(
    mutation: &Value,
    action: &str,
    expected: &str,
    dry_run: bool,
) -> Result<Value, String> {
    let document = Host::config_read(MCP, "")?;
    ensure_revision(expected, &document.revision)?;
    let mut config = parse_json(&document.content)?;
    apply_mcp_mutation(&mut config, action, mutation)?;
    let mut content = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;
    content.push('\n');
    finish_mutation(MCP, "mcp", "mcp.json", content, document, dry_run)
}

fn apply_mcp_mutation(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let root = config
        .as_object_mut()
        .ok_or("Kimi Code mcp.json root must be an object")?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Kimi Code mcpServers must be an object")?;
    let id = mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| {
            mutation
                .get("payload")
                .and_then(|p| p.get("id"))
                .and_then(Value::as_str)
        })
        .filter(|id| !id.is_empty())
        .ok_or("MCP server id is required")?;
    if action == "remove" {
        servers.remove(id);
    } else {
        let payload = mutation
            .get("payload")
            .ok_or("MCP server payload is required")?;
        let transport = payload
            .get("transport")
            .and_then(Value::as_str)
            .unwrap_or("stdio");
        let mut server = payload.as_object().cloned().unwrap_or_default();
        server.remove("id");
        server.remove("name");
        server.insert(
            "enabled".to_string(),
            json!(payload
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)),
        );
        if transport == "stdio" {
            server.remove("url");
            server.remove("transport");
        } else {
            server.remove("command");
            server.remove("args");
            if transport == "sse" {
                server.insert("transport".to_string(), json!("sse"));
            } else {
                server.remove("transport");
            }
        }
        servers.insert(id.to_string(), Value::Object(server));
    }
    Ok(())
}

fn mutate_provider(
    mutation: &Value,
    action: &str,
    expected: &str,
    dry_run: bool,
) -> Result<Value, String> {
    let document = Host::config_read(CONFIG, "")?;
    ensure_revision(expected, &document.revision)?;
    let mut config = parse_toml(&document.content)?;
    apply_provider_mutation(&mut config, action, mutation)?;
    finish_mutation(
        CONFIG,
        "providers",
        "config.toml",
        config.to_string(),
        document,
        dry_run,
    )
}

fn apply_provider_mutation(
    config: &mut DocumentMut,
    action: &str,
    mutation: &Value,
) -> Result<(), String> {
    let providers = ensure_table(config, "providers")?;
    let id = mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| {
            mutation
                .get("payload")
                .and_then(|p| p.get("id"))
                .and_then(Value::as_str)
        })
        .filter(|id| !id.is_empty())
        .ok_or("Provider id is required")?;
    if action == "remove" {
        providers.remove(id);
    } else {
        let payload = mutation
            .get("payload")
            .ok_or("Provider payload is required")?;
        let mut provider = providers
            .remove(id)
            .and_then(|item| item.into_table().ok())
            .unwrap_or_else(Table::new);
        provider.insert(
            "type",
            value(
                payload
                    .get("protocol")
                    .and_then(Value::as_str)
                    .unwrap_or("openai"),
            ),
        );
        if let Some(base_url) = payload.get("baseUrl").and_then(Value::as_str) {
            provider.insert("base_url", value(base_url));
        }
        if let Some(key) = payload
            .get("apiKeyValue")
            .and_then(Value::as_str)
            .filter(|key| !key.trim().is_empty())
        {
            provider.insert("api_key", value(key));
        }
        providers.insert(id, Item::Table(provider));
    }
    Ok(())
}

fn finish_mutation(
    resource: &str,
    section: &str,
    changed_resource: &str,
    content: String,
    document: mcp_link_agent_wasm_sdk::ConfigDocument,
    dry_run: bool,
) -> Result<Value, String> {
    let changed = content != document.content;
    let revision = if dry_run {
        document.revision
    } else {
        Host::config_write_atomic(resource, "", &content, &document.revision)?
            .get("revision")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or("Host file.writeAtomic returned no revision")?
    };
    Ok(json!({
        "section": section,
        "revision": revision,
        "changed": changed,
        "changedResources": [changed_resource],
        "restartRequired": true,
        "warnings": [],
    }))
}

fn section(id: &str, renderer: &str, source: &str, read_only: bool) -> Value {
    management_section_descriptor(id, renderer, source, read_only)
}

fn overview(params: &Value, configured: bool) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "Kimi Code",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "skillTargetCount": 2,
        "configured": configured,
        "warnings": []
    })
}

fn environment(params: &Value) -> Result<Value, String> {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    let files = [
        (CONFIG, "config.toml"),
        ("tui", "tui.toml"),
        (MCP, "mcp.json"),
    ]
    .into_iter()
    .map(|(id, label)| {
        Host::config_read(id, "").map(|document| {
            json!({
                "id": id,
                "label": label,
                "path": format!("~/.kimi-code/{label}"),
                "exists": !document.content.trim().is_empty()
            })
        })
    })
    .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "configFiles": files,
        "variables": [],
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    }))
}

fn parse_json(content: &str) -> Result<Value, String> {
    if content.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(content)
            .map_err(|error| format!("Invalid Kimi Code mcp.json: {error}"))
    }
}

fn parse_toml(content: &str) -> Result<DocumentMut, String> {
    if content.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        content
            .parse::<DocumentMut>()
            .map_err(|error| format!("Invalid Kimi Code config.toml: {error}"))
    }
}

fn ensure_table<'a>(document: &'a mut DocumentMut, key: &str) -> Result<&'a mut Table, String> {
    if document.get(key).is_none() {
        document.insert(key, Item::Table(Table::new()));
    }
    document
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| format!("Kimi Code {key} must be a table"))
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
    fn descriptor_exposes_shared_integrations() {
        let descriptor = describe(&json!({ "instance": { "id": "kimi" } })).unwrap();
        assert_eq!(descriptor["agentId"], "kimi-code");
        assert_eq!(descriptor["sections"].as_array().unwrap().len(), 7);
        assert_eq!(descriptor["sections"][1]["source"], "host");
        assert_eq!(descriptor["sections"][2]["renderer"], "mcp");
        assert_eq!(descriptor["sections"][3]["renderer"], "providers");
    }

    #[test]
    fn parses_empty_integration_documents() {
        assert_eq!(parse_json("").unwrap(), json!({}));
        assert!(parse_toml("").unwrap().is_empty());
    }

    #[test]
    fn mcp_link_mutations_preserve_existing_mcp_configuration() {
        let mut config = json!({
            "customRoot": true,
            "mcpServers": {
                "existing": {
                    "command": "npx",
                    "args": ["existing-server"],
                    "custom": 7
                }
            }
        });
        let existing = config["mcpServers"]["existing"].clone();

        apply_mcp_mutation(
            &mut config,
            "upsert",
            &json!({ "payload": {
                "id": "mcp-link",
                "name": "MCP Link",
                "transport": "http",
                "url": "http://192.168.1.10:3284/mcp",
                "headers": { "Authorization": "Bearer test-key" },
                "enabled": true
            }}),
        )
        .unwrap();
        assert_eq!(config["mcpServers"]["existing"], existing);
        assert_eq!(config["customRoot"], true);

        apply_mcp_mutation(&mut config, "remove", &json!({ "entityId": "mcp-link" })).unwrap();
        assert_eq!(config["mcpServers"]["existing"], existing);
        assert!(config["mcpServers"].get("mcp-link").is_none());
        assert_eq!(config["customRoot"], true);
    }

    #[test]
    fn mcp_link_mutations_preserve_existing_provider_and_models() {
        let mut config = parse_toml(
            r#"default_model = "existing/model"

[providers.existing]
type = "openai"
api_key = "existing-key"
base_url = "https://example.com/v1"
custom = 7

[models."existing/model"]
provider = "existing"
model = "model"
max_context_size = 100000
"#,
        )
        .unwrap();

        apply_provider_mutation(
            &mut config,
            "upsert",
            &json!({ "payload": {
                "id": "mcp-link",
                "name": "MCP Link Gateway",
                "protocol": "openai",
                "baseUrl": "http://192.168.1.10:3285/openai/v1",
                "apiKeyValue": "gateway-key"
            }}),
        )
        .unwrap();
        assert_eq!(
            config["providers"]["existing"]["api_key"].as_str(),
            Some("existing-key")
        );
        assert_eq!(
            config["providers"]["existing"]["custom"].as_integer(),
            Some(7)
        );
        assert_eq!(
            config["models"]["existing/model"]["provider"].as_str(),
            Some("existing")
        );
        assert_eq!(config["default_model"].as_str(), Some("existing/model"));

        apply_provider_mutation(&mut config, "remove", &json!({ "entityId": "mcp-link" })).unwrap();
        assert!(config["providers"].get("mcp-link").is_none());
        assert_eq!(
            config["providers"]["existing"]["api_key"].as_str(),
            Some("existing-key")
        );
        assert_eq!(
            config["models"]["existing/model"]["provider"].as_str(),
            Some("existing")
        );
        assert_eq!(config["default_model"].as_str(), Some("existing/model"));
    }
}
