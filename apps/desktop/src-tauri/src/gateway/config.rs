use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

use crate::{
    state::{save_store, DesktopState, StoreState},
    util::{json::required_string, time::now_iso},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayProvider {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) protocol: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    #[serde(default)]
    pub(crate) models: Vec<String>,
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayRoute {
    pub(crate) id: String,
    pub(crate) alias: String,
    pub(crate) protocol: String,
    pub(crate) provider_id: String,
    pub(crate) upstream_model: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

fn default_true() -> bool {
    true
}

const ACTIVE_PROVIDER_SETTING: &str = "modelGatewayActiveProviderId";

pub(crate) fn list_providers(store: &StoreState) -> Vec<GatewayProvider> {
    store
        .gateway_providers
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

pub(crate) fn list_routes(store: &StoreState) -> Vec<GatewayRoute> {
    store
        .gateway_routes
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

pub(crate) fn create_provider(
    store: &mut StoreState,
    input: Option<&Value>,
) -> Result<Value, String> {
    let input = input
        .and_then(Value::as_object)
        .ok_or_else(|| "Gateway provider input is required".to_string())?;
    let had_active_provider = active_provider_id(store).is_some();
    let now = now_iso();
    let provider = GatewayProvider {
        id: Uuid::new_v4().to_string(),
        name: required_field(input, "name")?,
        protocol: required_protocol(input)?,
        base_url: normalize_base_url(&required_field(input, "baseUrl")?)?,
        api_key: optional_field(input, "apiKey").unwrap_or_default(),
        models: string_list_field(input, "models")?,
        enabled: input
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
    };
    ensure_provider_name_unique(store, &provider.name, None)?;
    let value = serde_json::to_value(&provider).map_err(|error| error.to_string())?;
    store.gateway_providers.push(value.clone());
    if !had_active_provider && provider.enabled {
        store
            .settings
            .insert(ACTIVE_PROVIDER_SETTING.to_string(), json!(provider.id));
    }
    Ok(value)
}

pub(crate) fn update_provider(store: &mut StoreState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let updates = args
        .get(1)
        .and_then(Value::as_object)
        .ok_or_else(|| "Gateway provider updates are required".to_string())?;
    let current = list_providers(store)
        .into_iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("Gateway provider not found: {id}"))?;
    let name = optional_field(updates, "name").unwrap_or(current.name);
    ensure_provider_name_unique(store, &name, Some(&id))?;
    let protocol = if updates.contains_key("protocol") {
        required_protocol(updates)?
    } else {
        current.protocol.clone()
    };
    if protocol != current.protocol
        && list_routes(store)
            .iter()
            .any(|route| route.provider_id == id)
    {
        return Err(
            "Remove this provider's model mappings before changing its protocol".to_string(),
        );
    }
    let enabled = updates
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(current.enabled);
    if !enabled && active_provider_id(store).as_deref() == Some(id.as_str()) {
        return Err("Choose another current provider before disabling this one".to_string());
    }
    let provider = GatewayProvider {
        id: id.clone(),
        name,
        protocol,
        base_url: match optional_field(updates, "baseUrl") {
            Some(value) => normalize_base_url(&value)?,
            None => current.base_url,
        },
        api_key: if updates.contains_key("apiKey") {
            updates
                .get("apiKey")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        } else {
            current.api_key
        },
        models: if updates.contains_key("models") {
            string_list_field(updates, "models")?
        } else {
            current.models
        },
        enabled,
        created_at: current.created_at,
        updated_at: now_iso(),
    };
    let value = serde_json::to_value(&provider).map_err(|error| error.to_string())?;
    replace_by_id(&mut store.gateway_providers, &id, value.clone())?;
    Ok(value)
}

pub(crate) async fn fetch_provider_models(input: Option<&Value>) -> Result<Value, String> {
    let input = input
        .and_then(Value::as_object)
        .ok_or_else(|| "Gateway provider input is required".to_string())?;
    let protocol = required_protocol(input)?;
    let base_url = normalize_base_url(&required_field(input, "baseUrl")?)?;
    let api_key = optional_field(input, "apiKey").unwrap_or_default();
    let models_url = provider_models_url(&base_url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("mcp-link/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?;
    let mut request = client.get(models_url).header("accept", "application/json");
    if protocol == "anthropic" {
        request = request.header("anthropic-version", "2023-06-01");
        if !api_key.is_empty() {
            request = request.header("x-api-key", &api_key);
        }
    } else if !api_key.is_empty() {
        request = request.bearer_auth(&api_key);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        let detail = body.chars().take(300).collect::<String>();
        return Err(format!(
            "Upstream models request failed ({}): {detail}",
            status.as_u16()
        ));
    }
    let payload: Value =
        serde_json::from_str(&body).map_err(|error| format!("Invalid models response: {error}"))?;
    let mut models = model_ids_from_response(&payload);
    models.sort_by_key(|model| model.to_ascii_lowercase());
    models.dedup();
    if models.is_empty() {
        return Err("The upstream /v1/models response did not contain model IDs".to_string());
    }
    Ok(json!(models))
}

fn provider_models_url(base_url: &str) -> Result<Url, String> {
    let mut url = Url::parse(base_url).map_err(|error| error.to_string())?;
    let path = url.path().trim_end_matches('/');
    let path = if path.ends_with("/models") {
        path.to_string()
    } else if path.ends_with("/v1") {
        format!("{path}/models")
    } else {
        format!("{path}/v1/models")
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn model_ids_from_response(payload: &Value) -> Vec<String> {
    payload
        .as_array()
        .or_else(|| payload.get("data").and_then(Value::as_array))
        .or_else(|| payload.get("models").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.as_str()
                .or_else(|| item.get("id").and_then(Value::as_str))
                .or_else(|| item.get("name").and_then(Value::as_str))
                .or_else(|| item.get("model").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .collect()
}

pub(crate) fn remove_provider(store: &mut StoreState, id: &str) -> Result<(), String> {
    let removing_active = active_provider_id(store).as_deref() == Some(id);
    let before = store.gateway_providers.len();
    store
        .gateway_providers
        .retain(|value| value.get("id").and_then(Value::as_str) != Some(id));
    store
        .gateway_routes
        .retain(|value| value.get("providerId").and_then(Value::as_str) != Some(id));
    if before == store.gateway_providers.len() {
        return Err(format!("Gateway provider not found: {id}"));
    }
    if removing_active {
        if let Some(next) = list_providers(store)
            .into_iter()
            .find(|provider| provider.enabled)
        {
            store
                .settings
                .insert(ACTIVE_PROVIDER_SETTING.to_string(), json!(next.id));
        } else {
            store.settings.remove(ACTIVE_PROVIDER_SETTING);
        }
    }
    Ok(())
}

pub(crate) fn active_provider_id(store: &StoreState) -> Option<String> {
    let configured = store
        .settings
        .get(ACTIVE_PROVIDER_SETTING)
        .and_then(Value::as_str);
    list_providers(store)
        .into_iter()
        .find(|provider| {
            provider.enabled
                && configured.is_some_and(|configured| configured == provider.id.as_str())
        })
        .or_else(|| {
            list_providers(store)
                .into_iter()
                .find(|provider| provider.enabled)
        })
        .map(|provider| provider.id)
}

pub(crate) fn set_active_provider(store: &mut StoreState, id: &str) -> Result<Value, String> {
    let provider = list_providers(store)
        .into_iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("Gateway provider not found: {id}"))?;
    if !provider.enabled {
        return Err("Enable the provider before making it current".to_string());
    }
    store
        .settings
        .insert(ACTIVE_PROVIDER_SETTING.to_string(), json!(id));
    Ok(Value::String(id.to_string()))
}

pub(crate) fn create_route(store: &mut StoreState, input: Option<&Value>) -> Result<Value, String> {
    let input = input
        .and_then(Value::as_object)
        .ok_or_else(|| "Gateway model mapping input is required".to_string())?;
    let provider_id = required_field(input, "providerId")?;
    let provider = list_providers(store)
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("Gateway provider not found: {provider_id}"))?;
    let alias = required_field(input, "alias")?;
    ensure_route_alias_unique(store, &alias, &provider_id, None)?;
    let now = now_iso();
    let route = GatewayRoute {
        id: Uuid::new_v4().to_string(),
        alias,
        protocol: provider.protocol,
        provider_id,
        upstream_model: required_field(input, "upstreamModel")?,
        created_at: now.clone(),
        updated_at: now,
    };
    let value = serde_json::to_value(&route).map_err(|error| error.to_string())?;
    store.gateway_routes.push(value.clone());
    Ok(value)
}

pub(crate) fn update_route(store: &mut StoreState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let updates = args
        .get(1)
        .and_then(Value::as_object)
        .ok_or_else(|| "Gateway model mapping updates are required".to_string())?;
    let current = list_routes(store)
        .into_iter()
        .find(|route| route.id == id)
        .ok_or_else(|| format!("Gateway model mapping not found: {id}"))?;
    let provider_id = optional_field(updates, "providerId").unwrap_or(current.provider_id);
    let provider = list_providers(store)
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("Gateway provider not found: {provider_id}"))?;
    let alias = optional_field(updates, "alias").unwrap_or(current.alias);
    ensure_route_alias_unique(store, &alias, &provider_id, Some(&id))?;
    let route = GatewayRoute {
        id: id.clone(),
        alias,
        protocol: provider.protocol,
        provider_id,
        upstream_model: optional_field(updates, "upstreamModel").unwrap_or(current.upstream_model),
        created_at: current.created_at,
        updated_at: now_iso(),
    };
    let value = serde_json::to_value(&route).map_err(|error| error.to_string())?;
    replace_by_id(&mut store.gateway_routes, &id, value.clone())?;
    Ok(value)
}

pub(crate) fn remove_route(store: &mut StoreState, id: &str) -> Result<(), String> {
    let before = store.gateway_routes.len();
    store
        .gateway_routes
        .retain(|value| value.get("id").and_then(Value::as_str) != Some(id));
    if before == store.gateway_routes.len() {
        return Err(format!("Gateway model mapping not found: {id}"));
    }
    Ok(())
}

pub(crate) fn gateway_settings(state: &DesktopState) -> Value {
    state
        .store
        .lock()
        .map(|store| {
            json!({
                "listenHost": store.settings.get("modelGatewayListenHost").and_then(Value::as_str).unwrap_or("127.0.0.1"),
                "listenPort": store.settings.get("modelGatewayListenPort").and_then(Value::as_u64).unwrap_or(3285),
                "accessKey": store.settings.get("modelGatewayAccessKey").and_then(Value::as_str).unwrap_or_default(),
                "activeProviderId": active_provider_id(&store),
                "endpoint": current_endpoint(state),
                "listenerError": state.model_gateway_listener_error.lock().ok().and_then(|error| error.clone()),
            })
        })
        .unwrap_or_else(|_| json!({}))
}

pub(crate) fn update_gateway_settings(
    store: &mut StoreState,
    input: Option<&Value>,
) -> Result<Value, String> {
    let input = input
        .and_then(Value::as_object)
        .ok_or_else(|| "Gateway settings input is required".to_string())?;
    let host = input
        .get("listenHost")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("127.0.0.1");
    host.parse::<std::net::IpAddr>()
        .map_err(|_| "Gateway listen host must be an IP address".to_string())?;
    let port = input
        .get("listenPort")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "Gateway listen port must be between 1 and 65535".to_string())?;
    store
        .settings
        .insert("modelGatewayListenHost".to_string(), json!(host));
    store
        .settings
        .insert("modelGatewayListenPort".to_string(), json!(port));
    Ok(json!({
        "listenHost": host,
        "listenPort": port,
        "accessKey": store.settings.get("modelGatewayAccessKey").and_then(Value::as_str).unwrap_or_default(),
    }))
}

pub(crate) fn regenerate_access_key(state: &DesktopState) -> Result<Value, String> {
    let key = format!(
        "mcpg_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    store
        .settings
        .insert("modelGatewayAccessKey".to_string(), json!(key));
    save_store(&state.store_path, &store)?;
    Ok(store
        .settings
        .get("modelGatewayAccessKey")
        .cloned()
        .unwrap_or(Value::Null))
}

fn current_endpoint(state: &DesktopState) -> Option<String> {
    state
        .model_gateway_endpoint
        .lock()
        .ok()
        .and_then(|value| value.clone())
}

fn required_field(input: &serde_json::Map<String, Value>, key: &str) -> Result<String, String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{key} is required"))
}

fn optional_field(input: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn string_list_field(
    input: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, String> {
    let Some(value) = input.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("{key} must be an array"))?;
    let mut result = values
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    result.sort_by_key(|value| value.to_ascii_lowercase());
    result.dedup();
    Ok(result)
}

fn required_protocol(input: &serde_json::Map<String, Value>) -> Result<String, String> {
    let protocol = required_field(input, "protocol")?;
    match protocol.as_str() {
        "openai" | "openai-compatible" => Ok("openai-compatible".to_string()),
        "openai-responses" | "anthropic" => Ok(protocol),
        _ => Err(
            "Gateway protocol must be openai-compatible, openai-responses, or anthropic"
                .to_string(),
        ),
    }
}

fn normalize_base_url(value: &str) -> Result<String, String> {
    let url = Url::parse(value).map_err(|error| format!("Invalid provider Base URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Provider Base URL must use HTTP or HTTPS".to_string());
    }
    Ok(value.trim_end_matches('/').to_string())
}

fn ensure_provider_name_unique(
    store: &StoreState,
    name: &str,
    except: Option<&str>,
) -> Result<(), String> {
    if list_providers(store).iter().any(|provider| {
        provider.id != except.unwrap_or_default() && provider.name.eq_ignore_ascii_case(name)
    }) {
        return Err(format!("Gateway provider name already exists: {name}"));
    }
    Ok(())
}

fn ensure_route_alias_unique(
    store: &StoreState,
    alias: &str,
    provider_id: &str,
    except: Option<&str>,
) -> Result<(), String> {
    if list_routes(store).iter().any(|route| {
        route.id != except.unwrap_or_default()
            && route.provider_id == provider_id
            && route.alias == alias
    }) {
        return Err(format!("Gateway model alias already exists: {alias}"));
    }
    Ok(())
}

fn replace_by_id(values: &mut [Value], id: &str, replacement: Value) -> Result<(), String> {
    let target = values
        .iter_mut()
        .find(|value| value.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| format!("Gateway item not found: {id}"))?;
    *target = replacement;
    Ok(())
}
