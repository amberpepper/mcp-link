use mcp_link_agent_wasm_sdk::masked_secret;
use serde_json::{json, Value};
use toml_edit::{value, DocumentMut, Item, Table};

pub(super) fn section_data(config: &DocumentMut) -> Value {
    json!({
        "providers": providers(config),
        "secretInput": {
            "mode": "environment-variable",
            "defaultEnvironmentVariable": "OPENAI_API_KEY"
        }
    })
}

pub(super) fn providers(config: &DocumentMut) -> Vec<Value> {
    config
        .get("model_providers")
        .and_then(Item::as_table_like)
        .into_iter()
        .flat_map(|items| items.iter())
        .filter_map(|(id, item)| {
            let provider = item.as_table_like()?;
            let env_key = provider.get("env_key").and_then(Item::as_str);
            let inline_key = provider
                .get("http_headers")
                .and_then(Item::as_table_like)
                .and_then(|headers| headers.get("Authorization"))
                .and_then(Item::as_str)
                .and_then(|header| header.strip_prefix("Bearer "));
            Some(json!({
                "id": id,
                "name": provider.get("name").and_then(Item::as_str).unwrap_or(id),
                "protocol": "openai",
                "baseUrl": provider.get("base_url").and_then(Item::as_str),
                "apiKey": inline_key
                    .map(|_| masked_secret(Some("inline")))
                    .or_else(|| env_key
                    .map(|key| masked_secret(Some(&format!("${key}"))))
                    ).unwrap_or_else(|| masked_secret(None)),
                "defaultModel": if root_str(config, "model_provider") == Some(id) {
                    root_str(config, "model")
                } else {
                    None
                },
                "models": [],
                "enabled": true
            }))
        })
        .collect()
}

pub(super) fn mutate(
    config: &mut DocumentMut,
    action: &str,
    mutation: &Value,
) -> Result<(), String> {
    let id = entity_id(mutation)?;
    let providers = ensure_providers(config)?;
    if action == "remove" {
        providers.remove(id);
        return Ok(());
    }
    let payload = mutation
        .get("payload")
        .ok_or("Provider payload is required")?;
    let is_new = !providers.contains_key(id);
    let previous = providers
        .remove(id)
        .unwrap_or_else(|| Item::Table(Table::new()));
    let mut table = previous.into_table().unwrap_or_else(|_| Table::new());
    set_string(&mut table, "name", payload.get("name"));
    set_string(&mut table, "base_url", payload.get("baseUrl"));
    if let Some(variable) = payload
        .get("apiKeyEnvironmentVariable")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        table["env_key"] = value(variable);
    }
    if let Some(key) = payload
        .get("apiKeyValue")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let mut headers = Table::new();
        headers.insert("Authorization", value(format!("Bearer {key}")));
        table["http_headers"] = Item::Table(headers);
        table.remove("env_key");
    }
    if is_new && !table.contains_key("wire_api") {
        table["wire_api"] = value("responses");
    }
    providers.insert(id, Item::Table(table));
    Ok(())
}

fn root_str<'a>(config: &'a DocumentMut, key: &str) -> Option<&'a str> {
    config.get(key).and_then(Item::as_str)
}

fn entity_id<'a>(mutation: &'a Value) -> Result<&'a str, String> {
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

fn ensure_providers(config: &mut DocumentMut) -> Result<&mut Table, String> {
    if !config.get("model_providers").is_some_and(Item::is_table) {
        config["model_providers"] = Item::Table(Table::new());
    }
    config
        .get_mut("model_providers")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| "Codex model_providers must be a table".to_string())
}

fn set_string(table: &mut Table, key: &str, input: Option<&Value>) {
    match input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(input) => table[key] = value(input),
        None => {
            table.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> DocumentMut {
        input.parse().unwrap()
    }

    #[test]
    fn reads_environment_key_as_masked_state() {
        let config = parse(
            "model = \"m\"\nmodel_provider = \"proxy\"\n[model_providers.proxy]\nname = \"Proxy\"\nbase_url = \"https://example.com/v1\"\nenv_key = \"PROXY_API_KEY\"\n",
        );
        let section = section_data(&config);
        assert_eq!(section["secretInput"]["mode"], "environment-variable");
        assert_eq!(section["providers"][0]["apiKey"]["configured"], true);
        assert_eq!(
            section["providers"][0]["apiKey"]["environmentVariable"],
            "PROXY_API_KEY"
        );
    }

    #[test]
    fn writes_environment_key_and_preserves_it_when_not_submitted() {
        let mut config = parse("");
        mutate(
            &mut config,
            "upsert",
            &json!({ "payload": {
                "id": "proxy", "name": "Proxy", "baseUrl": "https://example.com/v1",
                "apiKeyEnvironmentVariable": "PROXY_API_KEY"
            } }),
        )
        .unwrap();
        assert_eq!(
            config["model_providers"]["proxy"]["env_key"].as_str(),
            Some("PROXY_API_KEY")
        );
        assert_eq!(
            config["model_providers"]["proxy"]["wire_api"].as_str(),
            Some("responses")
        );
        mutate(
            &mut config,
            "upsert",
            &json!({ "entityId": "proxy", "payload": {
                "id": "proxy", "name": "Renamed", "baseUrl": "https://example.com/v1"
            } }),
        )
        .unwrap();
        assert_eq!(
            config["model_providers"]["proxy"]["env_key"].as_str(),
            Some("PROXY_API_KEY")
        );
    }
}
