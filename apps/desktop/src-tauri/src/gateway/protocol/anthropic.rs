use serde_json::{json, Value};

use super::{openai::openai_chat_response_to_responses, portable_gateway_text};

pub(super) fn anthropic_to_openai_chat(payload: Value) -> Result<Value, String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "Anthropic request body must be a JSON object".to_string())?;
    let mut messages = Vec::new();
    if let Some(system) = object.get("system") {
        let text = portable_gateway_text(system);
        if !text.is_empty() {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }
    for message in object
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let blocks = message.get("content").unwrap_or(&Value::Null);
        if let Some(text) = blocks.as_str() {
            messages.push(json!({ "role": role, "content": text }));
            continue;
        }
        let mut content = Vec::new();
        let mut tool_calls = Vec::new();
        let mut tool_result_calls = Vec::new();
        for block in blocks.as_array().into_iter().flatten() {
            match block.get("type").and_then(Value::as_str).unwrap_or_default() {
                "text" => content.push(json!({
                    "type": "text",
                    "text": block.get("text").and_then(Value::as_str).unwrap_or_default()
                })),
                "image" => {
                    if let Some(source) = block.get("source") {
                        let image_url = if source.get("type").and_then(Value::as_str) == Some("base64") {
                            let media = source.get("media_type").and_then(Value::as_str).unwrap_or("image/png");
                            let data = source.get("data").and_then(Value::as_str).unwrap_or_default();
                            format!("data:{media};base64,{data}")
                        } else {
                            source.get("url").and_then(Value::as_str).unwrap_or_default().to_string()
                        };
                        content.push(json!({ "type": "image_url", "image_url": { "url": image_url } }));
                    }
                }
                "tool_use" => tool_calls.push(json!({
                    "id": block.get("id").and_then(Value::as_str).unwrap_or_default(),
                    "type": "function",
                    "function": {
                        "name": block.get("name").and_then(Value::as_str).unwrap_or("tool"),
                        "arguments": serde_json::to_string(block.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".to_string())
                    }
                })),
                "tool_result" => tool_result_calls.push(json!({
                    "role": "tool",
                    "tool_call_id": block.get("tool_use_id").and_then(Value::as_str).unwrap_or_default(),
                    "content": portable_gateway_text(block.get("content").unwrap_or(&Value::Null))
                })),
                _ => {}
            }
        }
        if !content.is_empty() || !tool_calls.is_empty() {
            let mut converted = json!({ "role": role, "content": content });
            if !tool_calls.is_empty() {
                converted["tool_calls"] = Value::Array(tool_calls);
                converted["content"] = Value::Null;
            }
            messages.push(converted);
        }
        messages.extend(tool_result_calls);
    }
    let mut converted = json!({
        "model": object.get("model").cloned().unwrap_or(Value::Null),
        "messages": messages,
        "stream": object.get("stream").cloned().unwrap_or(Value::Bool(false)),
        "max_tokens": object.get("max_tokens").cloned().unwrap_or(json!(4096))
    });
    for key in ["temperature", "top_p", "tools"] {
        if let Some(value) = object.get(key) {
            converted[key] = if key == "tools" {
                anthropic_tools_to_openai(value)
            } else {
                value.clone()
            };
        }
    }
    if let Some(stop) = object.get("stop_sequences") {
        converted["stop"] = stop.clone();
    }
    if let Some(choice) = object.get("tool_choice") {
        converted["tool_choice"] = anthropic_tool_choice_to_openai(choice);
    }
    Ok(converted)
}

pub(super) fn openai_to_anthropic_messages(payload: Value) -> Result<Value, String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "OpenAI request body must be a JSON object".to_string())?;
    let mut messages = Vec::new();
    let mut system = Vec::new();
    if let Some(instructions) = object.get("instructions").and_then(Value::as_str) {
        if !instructions.is_empty() {
            system.push(instructions.to_string());
        }
    }
    let source_messages = object
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            object
                .get("input")
                .map(|input| vec![json!({ "role": "user", "content": input })])
                .unwrap_or_default()
        });
    for message in source_messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let content = message.get("content").unwrap_or(&Value::Null);
        if role == "system" {
            let text = portable_gateway_text(content);
            if !text.is_empty() {
                system.push(text);
            }
            continue;
        }
        if role == "tool" {
            messages.push(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": message.get("tool_call_id").and_then(Value::as_str).unwrap_or_default(),
                    "content": portable_gateway_text(content)
                }]
            }));
            continue;
        }
        let mut blocks = openai_content_to_anthropic(content);
        if role == "assistant" {
            blocks.extend(openai_tool_calls_to_anthropic(
                message.get("tool_calls").unwrap_or(&Value::Null),
            ));
        }
        messages.push(json!({ "role": if role == "assistant" { "assistant" } else { "user" }, "content": blocks }));
    }
    let mut converted = json!({
        "model": object.get("model").cloned().unwrap_or(Value::Null),
        "messages": messages,
        "max_tokens": object
            .get("max_tokens")
            .or_else(|| object.get("max_completion_tokens"))
            .cloned()
            .unwrap_or(json!(4096)),
        "stream": object.get("stream").cloned().unwrap_or(Value::Bool(false))
    });
    if !system.is_empty() {
        converted["system"] = Value::String(system.join("\n\n"));
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = object.get(key) {
            converted[key] = value.clone();
        }
    }
    if let Some(stop) = object.get("stop") {
        converted["stop_sequences"] = stop.clone();
    }
    if let Some(tools) = object.get("tools") {
        converted["tools"] = openai_tools_to_anthropic(tools);
    }
    if let Some(choice) = object.get("tool_choice") {
        converted["tool_choice"] = openai_tool_choice_to_anthropic(choice);
    }
    Ok(converted)
}

fn openai_content_to_anthropic(content: &Value) -> Vec<Value> {
    if let Some(text) = content.as_str() {
        return vec![json!({ "type": "text", "text": text })];
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(
            |part| match part.get("type").and_then(Value::as_str).unwrap_or_default() {
                "text" | "input_text" => Some(json!({
                    "type": "text",
                    "text": part.get("text").and_then(Value::as_str).unwrap_or_default()
                })),
                "image_url" => {
                    let url = part
                        .get("image_url")
                        .and_then(|value| value.get("url"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    Some(json!({ "type": "image", "source": image_url_to_anthropic(url) }))
                }
                "input_image" => part
                    .get("image_url")
                    .and_then(Value::as_str)
                    .map(|url| json!({ "type": "image", "source": image_url_to_anthropic(url) })),
                _ => None,
            },
        )
        .collect()
}

fn image_url_to_anthropic(url: &str) -> Value {
    if let Some((metadata, data)) = url
        .strip_prefix("data:")
        .and_then(|value| value.split_once(','))
    {
        json!({
            "type": "base64",
            "media_type": metadata.split(';').next().unwrap_or("image/png"),
            "data": data
        })
    } else {
        json!({ "type": "url", "url": url })
    }
}

fn anthropic_tools_to_openai(value: &Value) -> Value {
    Value::Array(
        value
            .as_array()
            .into_iter()
            .flatten()
            .map(|tool| json!({
                "type": "function",
                "function": {
                    "name": tool.get("name").cloned().unwrap_or(Value::String("tool".to_string())),
                    "description": tool.get("description").cloned().unwrap_or(Value::Null),
                    "parameters": tool.get("input_schema").cloned().unwrap_or_else(|| json!({}))
                }
            }))
            .collect(),
    )
}

fn openai_tools_to_anthropic(value: &Value) -> Value {
    Value::Array(
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| {
                let function = tool.get("function").unwrap_or(tool);
                function.get("name").and_then(Value::as_str).map(|name| {
                    json!({
                        "name": name,
                        "description": function.get("description").cloned().unwrap_or(Value::Null),
                        "input_schema": function.get("parameters").cloned().unwrap_or_else(|| json!({}))
                    })
                })
            })
            .collect(),
    )
}

fn anthropic_tool_choice_to_openai(value: &Value) -> Value {
    match value.get("type").and_then(Value::as_str) {
        Some("any") => json!("required"),
        Some("tool") => json!({
            "type": "function",
            "function": { "name": value.get("name").cloned().unwrap_or_else(|| json!("tool")) }
        }),
        Some("none") => json!("none"),
        _ => json!("auto"),
    }
}

fn openai_tool_choice_to_anthropic(value: &Value) -> Value {
    if let Some(choice) = value.as_str() {
        return match choice {
            "required" => json!({ "type": "any" }),
            "none" => json!({ "type": "none" }),
            _ => json!({ "type": "auto" }),
        };
    }
    value
        .get("function")
        .and_then(|function| function.get("name"))
        .map(|name| json!({ "type": "tool", "name": name }))
        .unwrap_or_else(|| json!({ "type": "auto" }))
}

pub(super) fn anthropic_response_to_openai_response(value: &Value) -> Value {
    openai_chat_response_to_responses(&anthropic_response_to_openai(value))
}

pub(super) fn openai_response_to_anthropic(value: &Value) -> Value {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .unwrap_or(&Value::Null);
    let message = choice.get("message").unwrap_or(&Value::Null);
    let text = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            value
                .get("output_text")
                .and_then(Value::as_str)
                .unwrap_or_default()
        });
    let mut content = Vec::new();
    if !text.is_empty() {
        content.push(json!({ "type": "text", "text": text }));
    }
    content.extend(openai_tool_calls_to_anthropic(
        message.get("tool_calls").unwrap_or(&Value::Null),
    ));
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .map(|reason| match reason {
            "tool_calls" => "tool_use",
            "length" => "max_tokens",
            _ => "end_turn",
        });
    let usage = value.get("usage").unwrap_or(&Value::Null);
    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| json!("msg_mcp_link")),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": value.get("model").cloned().unwrap_or(Value::Null),
        "stop_reason": finish_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": usage.get("prompt_tokens").cloned().unwrap_or(json!(0)),
            "output_tokens": usage.get("completion_tokens").cloned().unwrap_or(json!(0)),
            "cache_read_input_tokens": usage.get("prompt_tokens_details").and_then(|details| details.get("cached_tokens")).cloned().unwrap_or(json!(0))
        }
    })
}

pub(super) fn anthropic_response_to_openai(value: &Value) -> Value {
    let blocks = value
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let text = blocks
        .iter()
        .find_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| item.get("text").and_then(Value::as_str).unwrap_or_default())
        })
        .unwrap_or_default();
    let tool_calls = blocks.iter().filter_map(|item| {
        (item.get("type").and_then(Value::as_str) == Some("tool_use")).then(|| json!({
            "id": item.get("id").cloned().unwrap_or(Value::String("tool_mcp_link".to_string())),
            "type": "function",
            "function": {
                "name": item.get("name").cloned().unwrap_or(Value::String("tool".to_string())),
                "arguments": serde_json::to_string(item.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".to_string())
            }
        }))
    }).collect::<Vec<_>>();
    let mut message = json!({ "role": "assistant", "content": text });
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
        message["content"] = Value::Null;
    }
    let usage = value.get("usage").unwrap_or(&Value::Null);
    let input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let finish_reason = match value.get("stop_reason").and_then(Value::as_str) {
        Some("tool_use") => "tool_calls",
        Some("max_tokens") => "length",
        _ => "stop",
    };
    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| json!("chatcmpl_mcp_link")),
        "object": "chat.completion",
        "created": 0,
        "model": value.get("model").cloned().unwrap_or(Value::Null),
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason
        }],
        "usage": {
            "prompt_tokens": input_tokens,
            "completion_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens,
            "prompt_tokens_details": {
                "cached_tokens": usage.get("cache_read_input_tokens").cloned().unwrap_or(json!(0))
            }
        }
    })
}

fn openai_tool_calls_to_anthropic(value: &Value) -> Vec<Value> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|call| {
            let function = call.get("function")?;
            let arguments = function
                .get("arguments")
                .and_then(Value::as_str)
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .unwrap_or_else(|| json!({}));
            Some(json!({
                "type": "tool_use",
                "id": call.get("id").cloned().unwrap_or(Value::String("tool_mcp_link".to_string())),
                "name": function.get("name").cloned().unwrap_or(Value::String("tool".to_string())),
                "input": arguments
            }))
        })
        .collect()
}
