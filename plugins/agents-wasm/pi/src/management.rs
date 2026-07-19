use mcp_link_agent_wasm_sdk::{
    finish_json_management_mutation, management_section, management_section_descriptor,
    masked_secret, read_json_document,
};
use serde_json::{json, Map, Value};

const SETTINGS: &str = "settings";
const MODELS: &str = "models";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params.get("instance").ok_or("Pi instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "pi",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", "overview", "plugin", true),
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
    let (settings_doc, settings) = read_json_document(SETTINGS, "", "Pi settings.json")?;
    let (models_doc, custom_models) = read_json_document(MODELS, "", "Pi models.json")?;
    let (revision, data) = match id {
        "overview" => (
            &settings_doc.revision,
            overview(params, &settings, &custom_models),
        ),
        "providers" => (
            &models_doc.revision,
            json!({
                "providers": providers(&custom_models, &settings),
                "secretInput": {
                    "mode": "environment-variable",
                    "defaultEnvironmentVariable": "OPENAI_API_KEY"
                }
            }),
        ),
        "models" => (
            &settings_doc.revision,
            model_settings(&settings, &custom_models),
        ),
        "environment" => (
            &settings_doc.revision,
            environment(params, &settings, &custom_models),
        ),
        _ => return Err(format!("Unsupported Pi management section: {id}")),
    };
    Ok(management_section(id, revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Pi management mutation is required")?;
    let section = required_string(mutation, "section")?;
    let action = required_string(mutation, "action")?;
    let expected = required_string(mutation, "expectedRevision")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match section {
        "providers" => {
            let (document, mut config) = read_json_document(MODELS, "", "Pi models.json")?;
            ensure_revision(expected, &document.revision)?;
            mutate_provider(&mut config, action, mutation)?;
            finish_json_management_mutation(
                MODELS,
                "",
                section,
                "models.json",
                &config,
                document,
                dry_run,
                false,
            )
        }
        "models" => {
            let (document, mut config) = read_json_document(SETTINGS, "", "Pi settings.json")?;
            ensure_revision(expected, &document.revision)?;
            mutate_model_settings(&mut config, mutation)?;
            finish_json_management_mutation(
                SETTINGS,
                "",
                section,
                "settings.json",
                &config,
                document,
                dry_run,
                false,
            )
        }
        _ => Err(format!("Pi section is read-only: {section}")),
    }
}

fn section(id: &str, renderer: &str, source: &str, read_only: bool) -> Value {
    management_section_descriptor(id, renderer, source, read_only)
}

fn overview(params: &Value, settings: &Value, models: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "Pi",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "defaultModel": settings.get("defaultModel"),
        "defaultProvider": settings.get("defaultProvider"),
        "mcpServerCount": 0,
        "providerCount": providers(models, settings).len(),
        "skillTargetCount": 3,
        "warnings": []
    })
}

fn providers(models: &Value, settings: &Value) -> Vec<Value> {
    models
        .get("providers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|providers| providers.iter())
        .map(|(id, provider)| {
            let model_ids = provider
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|model| model.get("id").and_then(Value::as_str))
                .collect::<Vec<_>>();
            let api = provider.get("api").and_then(Value::as_str).unwrap_or("custom");
            json!({
                "id": id,
                "name": provider.get("name").and_then(Value::as_str).unwrap_or(id),
                "protocol": api_protocol(api),
                "baseUrl": provider.get("baseUrl"),
                "apiKey": masked_secret(provider.get("apiKey").and_then(Value::as_str)),
                "defaultModel": if settings.get("defaultProvider").and_then(Value::as_str) == Some(id) { settings.get("defaultModel") } else { None },
                "models": model_ids,
                "enabled": true
            })
        })
        .collect()
}

fn model_settings(settings: &Value, models: &Value) -> Value {
    let default_provider = settings.get("defaultProvider").and_then(Value::as_str);
    let default_model = settings.get("defaultModel").and_then(Value::as_str);
    let selected = match (default_provider, default_model) {
        (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
        (_, Some(model)) => Some(model.to_string()),
        _ => None,
    };
    let available = providers(models, settings)
        .iter()
        .flat_map(|provider| {
            let id = provider
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            provider
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(move |model| format!("{id}/{model}"))
        })
        .collect::<Vec<_>>();
    json!({
        "defaultModel": selected,
        "smallModel": Value::Null,
        "reasoningModel": Value::Null,
        "reasoningEffort": Value::Null,
        "availableModels": available,
        "aliases": {}
    })
}

fn environment(params: &Value, settings: &Value, models: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "configFiles": [
            { "id": "settings", "label": "settings.json", "path": "~/.pi/agent/settings.json", "exists": !settings.as_object().is_none_or(Map::is_empty) },
            { "id": "models", "label": "models.json", "path": "~/.pi/agent/models.json", "exists": !models.as_object().is_none_or(Map::is_empty) }
        ],
        "variables": [],
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_provider(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "Provider")?;
    let providers = object_field(config, "providers")?;
    if action == "remove" {
        providers.remove(id);
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported Pi provider action: {action}"));
    }
    let payload = mutation
        .get("payload")
        .ok_or("Provider payload is required")?;
    let mut provider = providers
        .get(id)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(name) = non_empty(payload.get("name")) {
        provider.insert("name".into(), json!(name));
    }
    if let Some(base_url) = non_empty(payload.get("baseUrl")) {
        provider.insert("baseUrl".into(), json!(base_url));
    } else {
        provider.remove("baseUrl");
    }
    let protocol = payload
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or("openai");
    if provider.get("api").is_none() || protocol != "custom" {
        provider.insert("api".into(), json!(protocol_api(protocol)));
    }
    if let Some(key) = non_empty(payload.get("apiKeyValue")) {
        provider.insert("apiKey".into(), json!(key));
    } else if let Some(variable) = non_empty(payload.get("apiKeyEnvironmentVariable")) {
        provider.insert("apiKey".into(), json!(format!("${variable}")));
    }
    if let Some(models) = payload.get("models").and_then(Value::as_array) {
        let previous = provider
            .get("models")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let values = models
            .iter()
            .filter_map(Value::as_str)
            .map(|id| {
                previous
                    .iter()
                    .find(|model| model.get("id").and_then(Value::as_str) == Some(id))
                    .cloned()
                    .unwrap_or_else(|| json!({ "id": id }))
            })
            .collect();
        provider.insert("models".into(), Value::Array(values));
    }
    providers.insert(id.to_string(), Value::Object(provider));
    Ok(())
}

fn mutate_model_settings(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Pi model settings payload is required")?;
    let root = config
        .as_object_mut()
        .ok_or("Pi settings must be an object")?;
    let Some(value) = non_empty(payload.get("defaultModel")) else {
        root.remove("defaultModel");
        return Ok(());
    };
    if let Some((provider, model)) = value.split_once('/') {
        root.insert("defaultProvider".into(), json!(provider));
        root.insert("defaultModel".into(), json!(model));
    } else {
        root.insert("defaultModel".into(), json!(value));
    }
    Ok(())
}

fn api_protocol(api: &str) -> &str {
    if api.starts_with("openai") {
        "openai"
    } else if api.starts_with("anthropic") {
        "anthropic"
    } else if api.starts_with("google") {
        "gemini"
    } else {
        "custom"
    }
}

fn protocol_api(protocol: &str) -> &str {
    match protocol {
        "anthropic" => "anthropic-messages",
        "gemini" => "google-generative-ai",
        _ => "openai-completions",
    }
}

fn object_field<'a>(
    config: &'a mut Value,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let root = config
        .as_object_mut()
        .ok_or("Pi configuration must be an object")?;
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), json!({}));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("Pi {key} must be an object"))
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Pi {key} is required"))
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
    fn provider_update_preserves_model_metadata_and_unknown_fields() {
        let mut config = json!({
            "providers": { "demo": {
                "baseUrl": "https://old", "api": "openai-responses", "apiKey": "$DEMO_KEY",
                "models": [{ "id": "model-a", "contextWindow": 1000 }], "unknown": true
            }},
            "rootUnknown": 9
        });
        mutate_provider(&mut config, "upsert", &json!({
            "entityId": "demo",
            "payload": { "id": "demo", "protocol": "custom", "baseUrl": "https://new", "models": ["model-a"], "apiKeyEnvironmentVariable": "" }
        })).unwrap();
        assert_eq!(config["providers"]["demo"]["api"], "openai-responses");
        assert_eq!(config["providers"]["demo"]["apiKey"], "$DEMO_KEY");
        assert_eq!(
            config["providers"]["demo"]["models"][0]["contextWindow"],
            1000
        );
        assert_eq!(config["providers"]["demo"]["unknown"], true);
        assert_eq!(config["rootUnknown"], 9);
    }

    #[test]
    fn model_update_keeps_unrelated_settings() {
        let mut config =
            json!({ "defaultProvider": "old", "defaultModel": "old", "theme": "dark" });
        mutate_model_settings(
            &mut config,
            &json!({ "payload": { "defaultModel": "demo/model-a" } }),
        )
        .unwrap();
        assert_eq!(config["defaultProvider"], "demo");
        assert_eq!(config["defaultModel"], "model-a");
        assert_eq!(config["theme"], "dark");
    }
}
