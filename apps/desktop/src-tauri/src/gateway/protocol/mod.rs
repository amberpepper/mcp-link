mod anthropic;
mod openai;
mod sse;
mod stream;
mod stream_anthropic;
mod stream_openai;
mod stream_openai_events;
mod usage;

use serde_json::{json, Value};

use anthropic::*;
use openai::*;
#[cfg(test)]
pub(super) use stream::convert_stream_body;
pub(in crate::gateway) use stream::GatewayStreamConverter;
pub(in crate::gateway) use usage::{usage_from_json, StreamUsageTracker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GatewayFormat {
    OpenAiCompatible,
    OpenAiResponses,
    Anthropic,
}

impl GatewayFormat {
    pub(super) fn for_operation(operation: &str) -> Result<Self, String> {
        match operation {
            "chat" => Ok(Self::OpenAiCompatible),
            "responses" => Ok(Self::OpenAiResponses),
            "messages" => Ok(Self::Anthropic),
            _ => Err(format!("Unsupported gateway operation: {operation}")),
        }
    }

    pub(super) fn for_provider(protocol: &str) -> Result<Self, String> {
        match protocol {
            "openai" | "openai-compatible" => Ok(Self::OpenAiCompatible),
            "openai-responses" => Ok(Self::OpenAiResponses),
            "anthropic" => Ok(Self::Anthropic),
            _ => Err(format!("Unsupported provider protocol: {protocol}")),
        }
    }

    pub(super) fn upstream_path(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "/v1/chat/completions",
            Self::OpenAiResponses => "/v1/responses",
            Self::Anthropic => "/v1/messages",
        }
    }

    pub(super) fn protocol_name(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai-compatible",
            Self::OpenAiResponses => "openai-responses",
            Self::Anthropic => "anthropic",
        }
    }
}

pub(super) fn convert_error_body(
    body: &[u8],
    _from: GatewayFormat,
    to: GatewayFormat,
) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(body).unwrap_or_else(
        |_| json!({ "error": { "message": String::from_utf8_lossy(body).to_string() } }),
    );
    let error = value.get("error").unwrap_or(&value);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Upstream request failed");
    let kind = error
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("upstream_error");
    let converted = if to == GatewayFormat::Anthropic {
        json!({ "type": "error", "error": { "type": kind, "message": message } })
    } else {
        json!({
            "error": {
                "type": kind,
                "message": message,
                "code": error.get("code").cloned().unwrap_or(Value::Null),
                "param": error.get("param").cloned().unwrap_or(Value::Null)
            }
        })
    };
    serde_json::to_vec(&converted).map_err(|error| error.to_string())
}

pub(super) fn convert_request_payload(
    payload: Value,
    from: GatewayFormat,
    to: GatewayFormat,
) -> Result<Value, String> {
    if from == to {
        return Ok(payload);
    }
    match (from, to) {
        (GatewayFormat::OpenAiCompatible, GatewayFormat::OpenAiResponses) => {
            openai_chat_to_responses(payload)
        }
        (GatewayFormat::OpenAiResponses, GatewayFormat::OpenAiCompatible) => {
            openai_responses_to_chat(payload)
        }
        (GatewayFormat::OpenAiCompatible, GatewayFormat::Anthropic) => {
            openai_to_anthropic_messages(payload)
        }
        (GatewayFormat::OpenAiResponses, GatewayFormat::Anthropic) => {
            openai_to_anthropic_messages(openai_responses_to_chat(payload)?)
        }
        (GatewayFormat::Anthropic, GatewayFormat::OpenAiCompatible) => {
            anthropic_to_openai_chat(payload)
        }
        (GatewayFormat::Anthropic, GatewayFormat::OpenAiResponses) => {
            openai_chat_to_responses(anthropic_to_openai_chat(payload)?)
        }
        _ => Err("Unsupported gateway request conversion".to_string()),
    }
}

pub(super) fn portable_gateway_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(portable_gateway_text)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .map(portable_gateway_text)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

pub(super) fn convert_json_body(
    body: &[u8],
    from: GatewayFormat,
    to: GatewayFormat,
) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let converted = convert_response_payload(value, from, to)?;
    serde_json::to_vec(&converted).map_err(|error| error.to_string())
}

fn convert_response_payload(
    payload: Value,
    from: GatewayFormat,
    to: GatewayFormat,
) -> Result<Value, String> {
    if from == to {
        return Ok(payload);
    }
    Ok(match (from, to) {
        (GatewayFormat::OpenAiCompatible, GatewayFormat::OpenAiResponses) => {
            openai_chat_response_to_responses(&payload)
        }
        (GatewayFormat::OpenAiResponses, GatewayFormat::OpenAiCompatible) => {
            openai_responses_response_to_chat(&payload)
        }
        (GatewayFormat::OpenAiCompatible, GatewayFormat::Anthropic) => {
            openai_response_to_anthropic(&payload)
        }
        (GatewayFormat::OpenAiResponses, GatewayFormat::Anthropic) => {
            openai_response_to_anthropic(&openai_responses_response_to_chat(&payload))
        }
        (GatewayFormat::Anthropic, GatewayFormat::OpenAiCompatible) => {
            anthropic_response_to_openai(&payload)
        }
        (GatewayFormat::Anthropic, GatewayFormat::OpenAiResponses) => {
            anthropic_response_to_openai_response(&payload)
        }
        _ => return Err("Unsupported gateway response conversion".to_string()),
    })
}

#[cfg(test)]
mod tests;
