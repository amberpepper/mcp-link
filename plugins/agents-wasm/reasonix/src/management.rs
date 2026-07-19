use mcp_link_agent_wasm_sdk::{
    management_section, management_section_descriptor, ConfigDocument, Host,
};
use serde_json::{json, Value};
use toml_edit::{value, Array, ArrayOfTables, DocumentMut, Item, Table};

const CONFIG: &str = "config";
const CREDENTIALS: &str = "credentials";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params
        .get("instance")
        .ok_or("Reasonix instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "reasonix",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", "overview", "plugin", true),
            section("mcp", "mcp", "plugin", false),
            section("skills", "skills", "host", false),
            section("prompts", "prompts", "host", false),
            section("providers", "providers", "plugin", false),
            section("models", "models", "plugin", false),
            section("permissions", "permissions", "plugin", false),
            section("environment", "environment", "plugin", true),
            section("raw-config", "raw-config", "host", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let id = required_string(params, "section")?;
    let document = Host::config_read(CONFIG, "")?;
    let config = parse_toml(&document.content)?;
    let credentials = Host::config_read(CREDENTIALS, "")?;
    let data = match id {
        "overview" => overview(params, &config),
        "mcp" => json!({ "servers": mcp_servers(&config) }),
        "providers" => json!({
            "providers": providers(&config, &credentials.content),
            "secretInput": {
                "mode": "environment-variable",
                "defaultEnvironmentVariable": "REASONIX_PROVIDER_API_KEY"
            }
        }),
        "models" => model_settings(&config),
        "permissions" => permission_settings(&config),
        "environment" => environment(params, &config, &credentials.content),
        _ => return Err(format!("Unsupported Reasonix management section: {id}")),
    };
    Ok(management_section(id, &document.revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Reasonix management mutation is required")?;
    let section = required_string(mutation, "section")?;
    let action = required_string(mutation, "action")?;
    let expected = required_string(mutation, "expectedRevision")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let document = Host::config_read(CONFIG, "")?;
    ensure_revision(expected, &document.revision)?;
    let mut config = parse_toml(&document.content)?;
    let credential_update = match section {
        "mcp" => {
            mutate_mcp(&mut config, action, mutation)?;
            None
        }
        "providers" => mutate_provider(&mut config, action, mutation)?,
        "models" => {
            mutate_models(&mut config, mutation)?;
            None
        }
        "permissions" => {
            mutate_permissions(&mut config, mutation)?;
            None
        }
        _ => return Err(format!("Reasonix section is read-only: {section}")),
    };
    finish_mutation(
        section,
        config.to_string(),
        document,
        credential_update,
        dry_run,
    )
}

fn section(id: &str, renderer: &str, source: &str, read_only: bool) -> Value {
    management_section_descriptor(id, renderer, source, read_only)
}

fn overview(params: &Value, config: &DocumentMut) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "Reasonix",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "defaultModel": config.get("default_model").and_then(Item::as_str),
        "defaultProvider": config.get("default_model").and_then(Item::as_str).and_then(|model| model.split_once('/').map(|(provider, _)| provider)),
        "mcpServerCount": mcp_servers(config).len(),
        "providerCount": provider_tables(config).map(Iterator::count).unwrap_or(0),
        "skillTargetCount": 2,
        "warnings": []
    })
}

fn mcp_servers(config: &DocumentMut) -> Vec<Value> {
    plugin_tables(config)
        .into_iter()
        .flatten()
        .filter_map(|plugin| {
            let id = plugin.get("name").and_then(Item::as_str)?;
            let transport = plugin.get("type").and_then(Item::as_str).unwrap_or("stdio");
            Some(json!({
                "id": id,
                "name": id,
                "transport": transport,
                "command": plugin.get("command").and_then(Item::as_str),
                "args": item_string_array(plugin.get("args")),
                "url": plugin.get("url").and_then(Item::as_str),
                "env": item_string_map(plugin.get("env")),
                "headers": item_string_map(plugin.get("headers")),
                "enabled": plugin.get("auto_start").and_then(Item::as_bool).unwrap_or(true),
                "scope": "global"
            }))
        })
        .collect()
}

fn providers(config: &DocumentMut, credentials: &str) -> Vec<Value> {
    provider_tables(config)
        .into_iter()
        .flatten()
        .filter_map(|provider| {
            let id = provider.get("name").and_then(Item::as_str)?;
            let key_env = provider.get("api_key_env").and_then(Item::as_str);
            let configured = key_env
                .and_then(|key| env_value(credentials, key))
                .is_some_and(|value| !value.is_empty());
            let api_key = key_env.map(|key| json!({
                "configured": configured,
                "source": "environment",
                "masked": if configured { Value::String("••••••••".into()) } else { Value::Null },
                "environmentVariable": key
            })).unwrap_or_else(|| json!({ "configured": false }));
            let models = if let Some(models) = provider.get("models") {
                item_string_array(Some(models))
            } else {
                provider.get("model").and_then(Item::as_str).map(|model| vec![model.to_string()]).unwrap_or_default()
            };
            Some(json!({
                "id": id,
                "name": id,
                "protocol": normalize_protocol(provider.get("kind").and_then(Item::as_str).unwrap_or("openai")),
                "baseUrl": provider.get("base_url").and_then(Item::as_str),
                "apiKey": api_key,
                "defaultModel": provider.get("default").and_then(Item::as_str).or_else(|| provider.get("model").and_then(Item::as_str)),
                "models": models,
                "enabled": true
            }))
        })
        .collect()
}

fn model_settings(config: &DocumentMut) -> Value {
    let available = provider_tables(config)
        .into_iter()
        .flatten()
        .flat_map(|provider| {
            let id = provider
                .get("name")
                .and_then(Item::as_str)
                .unwrap_or_default();
            let models = if let Some(models) = provider.get("models") {
                item_string_array(Some(models))
            } else {
                provider
                    .get("model")
                    .and_then(Item::as_str)
                    .map(|model| vec![model.to_string()])
                    .unwrap_or_default()
            };
            models.into_iter().map(move |model| format!("{id}/{model}"))
        })
        .collect::<Vec<_>>();
    json!({
        "defaultModel": config.get("default_model").and_then(Item::as_str),
        "smallModel": Value::Null,
        "reasoningModel": Value::Null,
        "reasoningEffort": Value::Null,
        "availableModels": available,
        "aliases": {}
    })
}

fn permission_settings(config: &DocumentMut) -> Value {
    let permissions = config.get("permissions").and_then(Item::as_table_like);
    let sandbox = config.get("sandbox").and_then(Item::as_table_like);
    let mut rules = Vec::new();
    for (decision, key) in [("allow", "allow"), ("ask", "ask"), ("deny", "deny")] {
        for target in item_string_array(permissions.and_then(|table| table.get(key))) {
            rules.push(
                json!({ "id": target, "decision": decision, "target": target, "kind": "tool" }),
            );
        }
    }
    json!({
        "approvalMode": permissions.and_then(|table| table.get("mode")).and_then(Item::as_str),
        "sandboxMode": sandbox.and_then(|table| table.get("bash")).and_then(Item::as_str),
        "rules": rules
    })
}

fn environment(params: &Value, config: &DocumentMut, credentials: &str) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    let variables = provider_tables(config)
        .into_iter()
        .flatten()
        .filter_map(|provider| provider.get("api_key_env").and_then(Item::as_str))
        .map(|name| json!({
            "name": name,
            "secret": true,
            "source": if env_value(credentials, name).is_some() { ".env" } else { "not configured" }
        }))
        .collect::<Vec<_>>();
    json!({
        "configFiles": [
            { "id": "config", "label": "config.toml", "path": "<Reasonix home>/config.toml", "exists": !config.is_empty() },
            { "id": "credentials", "label": ".env", "path": "<Reasonix home>/.env", "exists": !credentials.trim().is_empty() }
        ],
        "variables": variables,
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut DocumentMut, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "MCP server")?;
    let plugins = ensure_array_of_tables(config, "plugins")?;
    let index = plugins
        .iter()
        .position(|plugin| plugin.get("name").and_then(Item::as_str) == Some(id));
    if action == "remove" {
        if let Some(index) = index {
            plugins.remove(index);
        }
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported Reasonix MCP action: {action}"));
    }
    let payload = mutation
        .get("payload")
        .ok_or("MCP server payload is required")?;
    let transport = payload
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    let mut plugin = index
        .and_then(|index| plugins.get(index).cloned())
        .unwrap_or_else(Table::new);
    if let Some(index) = index {
        plugins.remove(index);
    }
    plugin.insert("name", value(id));
    plugin.insert("type", value(transport));
    plugin.insert(
        "auto_start",
        value(
            payload
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        ),
    );
    if transport == "stdio" {
        plugin.insert(
            "command",
            value(non_empty(payload.get("command")).unwrap_or_default()),
        );
        plugin.insert("args", Item::Value(string_array(payload.get("args"))));
        plugin.insert("env", string_map_item(payload.get("env")));
        plugin.remove("url");
        plugin.remove("headers");
    } else {
        plugin.insert(
            "url",
            value(non_empty(payload.get("url")).unwrap_or_default()),
        );
        plugin.insert("headers", string_map_item(payload.get("headers")));
        plugin.remove("command");
        plugin.remove("args");
        plugin.remove("env");
    }
    plugins.push(plugin);
    Ok(())
}

fn mutate_provider(
    config: &mut DocumentMut,
    action: &str,
    mutation: &Value,
) -> Result<Option<(String, String)>, String> {
    let id = entity_id(mutation, "Provider")?;
    let providers = ensure_array_of_tables(config, "providers")?;
    let index = providers
        .iter()
        .position(|provider| provider.get("name").and_then(Item::as_str) == Some(id));
    if action == "remove" {
        if let Some(index) = index {
            providers.remove(index);
        }
        return Ok(None);
    }
    if action != "upsert" {
        return Err(format!("Unsupported Reasonix provider action: {action}"));
    }
    let payload = mutation
        .get("payload")
        .ok_or("Provider payload is required")?;
    let mut provider = index
        .and_then(|index| providers.get(index).cloned())
        .unwrap_or_else(Table::new);
    if let Some(index) = index {
        providers.remove(index);
    }
    provider.insert("name", value(id));
    provider.insert(
        "kind",
        value(provider_kind(
            payload
                .get("protocol")
                .and_then(Value::as_str)
                .unwrap_or("openai"),
        )),
    );
    if let Some(base_url) = non_empty(payload.get("baseUrl")) {
        provider.insert("base_url", value(base_url));
    } else {
        provider.remove("base_url");
    }
    if let Some(models) = payload.get("models").and_then(Value::as_array) {
        let values = models.iter().filter_map(Value::as_str).collect::<Vec<_>>();
        if values.len() == 1 {
            provider.insert("model", value(values[0]));
            provider.remove("models");
        } else {
            let mut array = Array::new();
            for model in values {
                array.push(model);
            }
            provider.insert("models", Item::Value(array.into()));
            provider.remove("model");
        }
    }
    let env_key = non_empty(payload.get("apiKeyEnvironmentVariable"))
        .map(str::to_string)
        .or_else(|| {
            provider
                .get("api_key_env")
                .and_then(Item::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| generated_env_key(id));
    provider.insert("api_key_env", value(&env_key));
    let credential_update =
        non_empty(payload.get("apiKeyValue")).map(|secret| (env_key, secret.to_string()));
    providers.push(provider);
    Ok(credential_update)
}

fn mutate_models(config: &mut DocumentMut, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Reasonix model settings payload is required")?;
    match non_empty(payload.get("defaultModel")) {
        Some(model) => {
            config["default_model"] = value(model);
        }
        None => {
            config.remove("default_model");
        }
    }
    Ok(())
}

fn mutate_permissions(config: &mut DocumentMut, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Reasonix permissions payload is required")?;
    let permissions = ensure_table(config, "permissions")?;
    if let Some(mode) = non_empty(payload.get("approvalMode")) {
        permissions.insert("mode", value(mode));
    }
    let rules = payload
        .get("rules")
        .and_then(Value::as_array)
        .ok_or("Permission rules are required")?;
    for decision in ["allow", "ask", "deny"] {
        let mut array = Array::new();
        for target in rules
            .iter()
            .filter(|rule| rule.get("decision").and_then(Value::as_str) == Some(decision))
            .filter_map(|rule| rule.get("target").and_then(Value::as_str))
            .map(str::trim)
            .filter(|target| !target.is_empty())
        {
            array.push(target);
        }
        permissions.insert(decision, Item::Value(array.into()));
    }
    if let Some(mode) = non_empty(payload.get("sandboxMode")) {
        ensure_table(config, "sandbox")?.insert("bash", value(mode));
    }
    Ok(())
}

fn finish_mutation(
    section: &str,
    content: String,
    document: ConfigDocument,
    credential_update: Option<(String, String)>,
    dry_run: bool,
) -> Result<Value, String> {
    let changed = content != document.content;
    let mut changed_resources = vec![json!("config.toml")];
    if credential_update.is_some() {
        changed_resources.push(json!(".env"));
    }
    let revision = if dry_run {
        document.revision
    } else {
        let credential_write = if let Some((key, secret)) = credential_update {
            let credentials = Host::config_read(CREDENTIALS, "")?;
            let next = set_env_value(&credentials.content, &key, &secret)?;
            let written = Host::config_write_atomic(CREDENTIALS, "", &next, &credentials.revision)?;
            let revision = written
                .get("revision")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or("Host file.writeAtomic returned no revision")?;
            Some((credentials, revision))
        } else {
            None
        };

        let config_write = Host::config_write_atomic(CONFIG, "", &content, &document.revision);
        let config_write = match config_write {
            Ok(result) => result,
            Err(error) => {
                if let Some((credentials, written_revision)) = credential_write {
                    if let Err(rollback_error) = Host::config_write_atomic(
                        CREDENTIALS,
                        "",
                        &credentials.content,
                        &written_revision,
                    ) {
                        return Err(format!(
                            "{error}; failed to roll back Reasonix credentials: {rollback_error}"
                        ));
                    }
                }
                return Err(error);
            }
        };
        config_write
            .get("revision")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or("Host file.writeAtomic returned no revision")?
    };
    Ok(json!({
        "section": section,
        "revision": revision,
        "changed": changed || changed_resources.len() > 1,
        "changedResources": changed_resources,
        "restartRequired": true,
        "warnings": []
    }))
}

fn parse_toml(content: &str) -> Result<DocumentMut, String> {
    if content.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        content
            .parse()
            .map_err(|error| format!("Invalid Reasonix config.toml: {error}"))
    }
}

fn provider_tables(config: &DocumentMut) -> Option<impl Iterator<Item = &Table>> {
    config
        .get("providers")
        .and_then(Item::as_array_of_tables)
        .map(|tables| tables.iter())
}

fn plugin_tables(config: &DocumentMut) -> Option<impl Iterator<Item = &Table>> {
    config
        .get("plugins")
        .and_then(Item::as_array_of_tables)
        .map(|tables| tables.iter())
}

fn ensure_array_of_tables<'a>(
    config: &'a mut DocumentMut,
    key: &str,
) -> Result<&'a mut ArrayOfTables, String> {
    if config.get(key).is_none() {
        config.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    config
        .get_mut(key)
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| format!("Reasonix {key} must be an array of tables"))
}

fn ensure_table<'a>(config: &'a mut DocumentMut, key: &str) -> Result<&'a mut Table, String> {
    if config.get(key).is_none() {
        config.insert(key, Item::Table(Table::new()));
    }
    config
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| format!("Reasonix {key} must be a table"))
}

fn item_string_array(item: Option<&Item>) -> Vec<String> {
    item.and_then(Item::as_array)
        .into_iter()
        .flat_map(|values| values.iter())
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn item_string_map(item: Option<&Item>) -> Value {
    let Some(table) = item.and_then(Item::as_inline_table) else {
        return json!({});
    };
    Value::Object(
        table
            .iter()
            .filter_map(|(key, value)| Some((key.to_string(), json!(value.as_str()?))))
            .collect(),
    )
}

fn string_array(value: Option<&Value>) -> toml_edit::Value {
    let mut array = Array::new();
    for item in value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        array.push(item);
    }
    array.into()
}

fn string_map_item(value: Option<&Value>) -> Item {
    let mut table = toml_edit::InlineTable::new();
    if let Some(values) = value.and_then(Value::as_object) {
        for (key, value) in values {
            if let Some(value) = value.as_str() {
                table.insert(key, value.into());
            }
        }
    }
    Item::Value(table.into())
}

fn env_value<'a>(content: &'a str, target: &str) -> Option<&'a str> {
    content.lines().find_map(|line| {
        let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
        if line.starts_with('#') {
            return None;
        }
        let (key, value) = line.split_once('=')?;
        (key.trim() == target).then_some(value.trim().trim_matches(['\'', '"']))
    })
}

fn set_env_value(content: &str, key: &str, secret: &str) -> Result<String, String> {
    if !valid_env_key(key) {
        return Err("Reasonix api_key_env must be a valid environment variable name".into());
    }
    if secret.contains(['\r', '\n']) {
        return Err("Reasonix API key cannot contain a newline".into());
    }
    let assignment = format!(
        "{key}={}",
        serde_json::to_string(secret).map_err(|error| error.to_string())?
    );
    let mut found = false;
    let mut lines = content
        .lines()
        .map(|line| {
            let candidate = line.trim().strip_prefix("export ").unwrap_or(line.trim());
            let matches = !candidate.starts_with('#')
                && candidate
                    .split_once('=')
                    .is_some_and(|(name, _)| name.trim() == key);
            if matches {
                found = true;
                assignment.clone()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>();
    if !found {
        lines.push(assignment);
    }
    let mut output = lines.join("\n");
    output.push('\n');
    Ok(output)
}

fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn generated_env_key(id: &str) -> String {
    let normalized = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}_API_KEY", normalized.trim_matches('_'))
}

fn normalize_protocol(kind: &str) -> &str {
    match kind {
        "openai" => "openai",
        "anthropic" => "anthropic",
        "gemini" | "google" => "gemini",
        _ => "custom",
    }
}
fn provider_kind(protocol: &str) -> &str {
    match protocol {
        "anthropic" => "anthropic",
        "gemini" => "gemini",
        _ => "openai",
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Reasonix {key} is required"))
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
    fn provider_update_preserves_unknown_fields_and_other_entries() {
        let mut config = parse_toml(
            r#"
default_model = "old/model"
unknown_root = true

[[providers]]
name = "demo"
kind = "openai"
base_url = "https://old"
models = ["a"]
api_key_env = "DEMO_KEY"
unknown = 7

[[providers]]
name = "other"
kind = "anthropic"
model = "claude"
api_key_env = "OTHER_KEY"
"#,
        )
        .unwrap();
        let update = mutate_provider(&mut config, "upsert", &json!({
            "entityId": "demo",
            "payload": { "id": "demo", "protocol": "openai", "baseUrl": "https://new", "models": ["a"], "apiKeyEnvironmentVariable": "DEMO_KEY" }
        })).unwrap();
        assert!(update.is_none());
        let output = config.to_string();
        assert!(output.contains("unknown = 7"));
        assert!(output.contains("unknown_root = true"));
        assert!(output.contains("name = \"other\""));
        assert!(output.contains("base_url = \"https://new\""));
    }

    #[test]
    fn mcp_update_preserves_reasonix_policy_fields() {
        let mut config = parse_toml(
            r#"
[[plugins]]
name = "demo"
type = "stdio"
command = "old"
call_timeout_seconds = 20
trusted_read_only_tools = ["read"]
unknown = "keep"
"#,
        )
        .unwrap();
        mutate_mcp(&mut config, "upsert", &json!({
            "entityId": "demo",
            "payload": { "id": "demo", "transport": "stdio", "command": "new", "args": [], "env": {}, "enabled": true }
        })).unwrap();
        let output = config.to_string();
        assert!(output.contains("call_timeout_seconds = 20"));
        assert!(output.contains("trusted_read_only_tools = [\"read\"]"));
        assert!(output.contains("unknown = \"keep\""));
    }

    #[test]
    fn env_update_preserves_comments_and_other_keys() {
        let output = set_env_value("# keep\nOTHER=1\nDEMO=old\n", "DEMO", "new value").unwrap();
        assert!(output.contains("# keep"));
        assert!(output.contains("OTHER=1"));
        assert!(output.contains("DEMO=\"new value\""));
    }
}
