use serde_json::{json, Value};

use super::portable_gateway_text;

pub(super) fn openai_chat_to_responses(payload: Value) -> Result<Value, String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "OpenAI Chat request body must be an object".to_string())?;
    let mut instructions = Vec::new();
    let mut input = Vec::new();
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
        let content = message.get("content").unwrap_or(&Value::Null);
        if matches!(role, "system" | "developer") {
            let text = portable_gateway_text(content);
            if !text.is_empty() {
                instructions.push(text);
            }
            continue;
        }
        if role == "tool" {
            input.push(json!({
                "type": "function_call_output",
                "call_id": message.get("tool_call_id").cloned().unwrap_or(Value::Null),
                "output": portable_gateway_text(content)
            }));
            continue;
        }
        let converted_content = chat_content_to_responses(content, role == "assistant");
        if !converted_content.is_empty() {
            input.push(json!({
                "type": "message",
                "role": if role == "assistant" { "assistant" } else { "user" },
                "content": converted_content
            }));
        }
        for call in message
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let function = call.get("function").unwrap_or(&Value::Null);
            input.push(json!({
                "type": "function_call",
                "id": call.get("id").cloned().unwrap_or(Value::Null),
                "call_id": call.get("id").cloned().unwrap_or(Value::Null),
                "name": function.get("name").cloned().unwrap_or_else(|| json!("tool")),
                "arguments": function.get("arguments").cloned().unwrap_or_else(|| json!("{}"))
            }));
        }
    }
    let mut converted = json!({
        "model": object.get("model").cloned().unwrap_or(Value::Null),
        "input": input,
        "stream": object.get("stream").cloned().unwrap_or(Value::Bool(false))
    });
    if !instructions.is_empty() {
        converted["instructions"] = Value::String(instructions.join("\n\n"));
    }
    copy_field(object, &mut converted, "temperature", "temperature");
    copy_field(object, &mut converted, "top_p", "top_p");
    copy_field(object, &mut converted, "max_tokens", "max_output_tokens");
    copy_field(
        object,
        &mut converted,
        "max_completion_tokens",
        "max_output_tokens",
    );
    copy_field(object, &mut converted, "metadata", "metadata");
    copy_field(object, &mut converted, "user", "user");
    copy_field(
        object,
        &mut converted,
        "parallel_tool_calls",
        "parallel_tool_calls",
    );
    copy_field(object, &mut converted, "service_tier", "service_tier");
    if let Some(response_format) = object.get("response_format") {
        converted["text"] = json!({ "format": response_format });
    }
    if let Some(tools) = object.get("tools") {
        converted["tools"] = chat_tools_to_responses(tools);
    }
    if let Some(choice) = object.get("tool_choice") {
        converted["tool_choice"] = chat_tool_choice_to_responses(choice);
    }
    if let Some(effort) = object.get("reasoning_effort") {
        converted["reasoning"] = json!({ "effort": effort });
    }
    Ok(converted)
}

pub(super) fn openai_responses_to_chat(payload: Value) -> Result<Value, String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "OpenAI Responses request body must be an object".to_string())?;
    if object
        .get("previous_response_id")
        .is_some_and(|value| !value.is_null())
    {
        return Err(
            "previous_response_id cannot be converted to a stateless Compatible request"
                .to_string(),
        );
    }
    let mut messages = Vec::new();
    if let Some(instructions) = object.get("instructions") {
        let text = portable_gateway_text(instructions);
        if !text.is_empty() {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }
    match object.get("input") {
        Some(Value::String(text)) => messages.push(json!({ "role": "user", "content": text })),
        Some(Value::Array(items)) => {
            for item in items {
                match item.get("type").and_then(Value::as_str).unwrap_or("message") {
                    "message" => {
                        let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
                        messages.push(json!({
                            "role": role,
                            "content": responses_content_to_chat(item.get("content").unwrap_or(&Value::Null))
                        }));
                    }
                    "function_call" => messages.push(json!({
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": item.get("call_id").or_else(|| item.get("id")).cloned().unwrap_or(Value::Null),
                            "type": "function",
                            "function": {
                                "name": item.get("name").cloned().unwrap_or_else(|| json!("tool")),
                                "arguments": item.get("arguments").cloned().unwrap_or_else(|| json!("{}"))
                            }
                        }]
                    })),
                    "function_call_output" => messages.push(json!({
                        "role": "tool",
                        "tool_call_id": item.get("call_id").cloned().unwrap_or(Value::Null),
                        "content": portable_gateway_text(item.get("output").unwrap_or(&Value::Null))
                    })),
                    _ => {}
                }
            }
        }
        Some(value) => messages.push(json!({
            "role": "user",
            "content": portable_gateway_text(value)
        })),
        None => {}
    }
    let mut converted = json!({
        "model": object.get("model").cloned().unwrap_or(Value::Null),
        "messages": messages,
        "stream": object.get("stream").cloned().unwrap_or(Value::Bool(false))
    });
    copy_field(object, &mut converted, "temperature", "temperature");
    copy_field(object, &mut converted, "top_p", "top_p");
    copy_field(
        object,
        &mut converted,
        "max_output_tokens",
        "max_completion_tokens",
    );
    copy_field(object, &mut converted, "metadata", "metadata");
    copy_field(object, &mut converted, "user", "user");
    copy_field(
        object,
        &mut converted,
        "parallel_tool_calls",
        "parallel_tool_calls",
    );
    copy_field(object, &mut converted, "service_tier", "service_tier");
    if let Some(format) = object.get("text").and_then(|text| text.get("format")) {
        converted["response_format"] = format.clone();
    }
    if let Some(tools) = object.get("tools") {
        converted["tools"] = responses_tools_to_chat(tools)?;
    }
    if let Some(choice) = object.get("tool_choice") {
        converted["tool_choice"] = responses_tool_choice_to_chat(choice)?;
    }
    if let Some(effort) = object
        .get("reasoning")
        .and_then(|reasoning| reasoning.get("effort"))
    {
        converted["reasoning_effort"] = effort.clone();
    }
    Ok(converted)
}

fn copy_field(
    source: &serde_json::Map<String, Value>,
    target: &mut Value,
    source_key: &str,
    target_key: &str,
) {
    if let Some(value) = source.get(source_key) {
        target[target_key] = value.clone();
    }
}

fn chat_content_to_responses(content: &Value, assistant: bool) -> Vec<Value> {
    if let Some(text) = content.as_str() {
        return vec![json!({
            "type": if assistant { "output_text" } else { "input_text" },
            "text": text
        })];
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| match part.get("type").and_then(Value::as_str) {
            Some("text" | "input_text" | "output_text") => Some(json!({
                "type": if assistant { "output_text" } else { "input_text" },
                "text": part.get("text").cloned().unwrap_or_else(|| json!(""))
            })),
            Some("image_url") => Some(json!({
                "type": "input_image",
                "image_url": part.get("image_url").and_then(|image| image.get("url")).cloned().unwrap_or(Value::Null)
            })),
            _ => None,
        })
        .collect()
}

fn responses_content_to_chat(content: &Value) -> Value {
    if let Some(text) = content.as_str() {
        return Value::String(text.to_string());
    }
    Value::Array(
        content
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("input_text" | "output_text" | "text") => Some(json!({
                    "type": "text",
                    "text": part.get("text").cloned().unwrap_or_else(|| json!(""))
                })),
                Some("input_image") => Some(json!({
                    "type": "image_url",
                    "image_url": { "url": part.get("image_url").cloned().unwrap_or(Value::Null) }
                })),
                _ => None,
            })
            .collect(),
    )
}

fn chat_tools_to_responses(tools: &Value) -> Value {
    Value::Array(
        tools
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| {
                let function = tool.get("function")?;
                Some(json!({
                    "type": "function",
                    "name": function.get("name").cloned().unwrap_or_else(|| json!("tool")),
                    "description": function.get("description").cloned().unwrap_or(Value::Null),
                    "parameters": function.get("parameters").cloned().unwrap_or_else(|| json!({})),
                    "strict": function.get("strict").cloned().unwrap_or(Value::Bool(false))
                }))
            })
            .collect(),
    )
}

fn responses_tools_to_chat(tools: &Value) -> Result<Value, String> {
    let mut converted = Vec::new();
    for tool in tools.as_array().into_iter().flatten() {
        let tool_type = tool.get("type").and_then(Value::as_str).unwrap_or_default();
        if tool_type != "function" {
            return Err(format!(
                "Responses tool type {tool_type} cannot be converted to OpenAI Compatible"
            ));
        }
        converted.push(json!({
            "type": "function",
            "function": {
                "name": tool.get("name").cloned().unwrap_or_else(|| json!("tool")),
                "description": tool.get("description").cloned().unwrap_or(Value::Null),
                "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({})),
                "strict": tool.get("strict").cloned().unwrap_or(Value::Bool(false))
            }
        }));
    }
    Ok(Value::Array(converted))
}

fn chat_tool_choice_to_responses(choice: &Value) -> Value {
    choice
        .get("function")
        .and_then(|function| function.get("name"))
        .map(|name| json!({ "type": "function", "name": name }))
        .unwrap_or_else(|| choice.clone())
}

fn responses_tool_choice_to_chat(choice: &Value) -> Result<Value, String> {
    if choice.get("type").and_then(Value::as_str) == Some("function") {
        return Ok(json!({
            "type": "function",
            "function": { "name": choice.get("name").cloned().unwrap_or_else(|| json!("tool")) }
        }));
    }
    if choice.is_string()
        || choice
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| matches!(kind, "auto" | "none" | "required"))
    {
        return Ok(choice.clone());
    }
    Err("Responses tool_choice cannot be converted to OpenAI Compatible".to_string())
}

pub(super) fn openai_chat_response_to_responses(value: &Value) -> Value {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .unwrap_or(&Value::Null);
    let message = choice.get("message").unwrap_or(&Value::Null);
    let mut output = Vec::new();
    let text = portable_gateway_text(message.get("content").unwrap_or(&Value::Null));
    if !text.is_empty() {
        output.push(json!({
            "id": "msg_mcp_link",
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": text, "annotations": [] }]
        }));
    }
    for call in message
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let function = call.get("function").unwrap_or(&Value::Null);
        output.push(json!({
            "id": call.get("id").cloned().unwrap_or_else(|| json!("fc_mcp_link")),
            "type": "function_call",
            "status": "completed",
            "call_id": call.get("id").cloned().unwrap_or_else(|| json!("call_mcp_link")),
            "name": function.get("name").cloned().unwrap_or_else(|| json!("tool")),
            "arguments": function.get("arguments").cloned().unwrap_or_else(|| json!("{}"))
        }));
    }
    let usage = value.get("usage").unwrap_or(&Value::Null);
    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| json!("resp_mcp_link")),
        "object": "response",
        "created_at": value.get("created").cloned().unwrap_or_else(|| json!(0)),
        "status": "completed",
        "model": value.get("model").cloned().unwrap_or(Value::Null),
        "output": output,
        "output_text": text,
        "usage": {
            "input_tokens": usage.get("prompt_tokens").cloned().unwrap_or_else(|| json!(0)),
            "output_tokens": usage.get("completion_tokens").cloned().unwrap_or_else(|| json!(0)),
            "total_tokens": usage.get("total_tokens").cloned().unwrap_or_else(|| json!(0)),
            "input_tokens_details": usage.get("prompt_tokens_details").cloned().unwrap_or_else(|| json!({})),
            "output_tokens_details": usage.get("completion_tokens_details").cloned().unwrap_or_else(|| json!({}))
        }
    })
}

pub(super) fn openai_responses_response_to_chat(value: &Value) -> Value {
    let mut text = value
        .get("output_text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut tool_calls = Vec::new();
    for item in value
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match item.get("type").and_then(Value::as_str).unwrap_or_default() {
            "message" if text.is_empty() => {
                text = portable_gateway_text(item.get("content").unwrap_or(&Value::Null));
            }
            "function_call" => tool_calls.push(json!({
                "id": item.get("call_id").or_else(|| item.get("id")).cloned().unwrap_or_else(|| json!("call_mcp_link")),
                "type": "function",
                "function": {
                    "name": item.get("name").cloned().unwrap_or_else(|| json!("tool")),
                    "arguments": item.get("arguments").cloned().unwrap_or_else(|| json!("{}"))
                }
            })),
            _ => {}
        }
    }
    let mut message = json!({ "role": "assistant", "content": text });
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
        if text.is_empty() {
            message["content"] = Value::Null;
        }
    }
    let usage = value.get("usage").unwrap_or(&Value::Null);
    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| json!("chatcmpl_mcp_link")),
        "object": "chat.completion",
        "created": value.get("created_at").cloned().unwrap_or_else(|| json!(0)),
        "model": value.get("model").cloned().unwrap_or(Value::Null),
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": if message.get("tool_calls").is_some() { "tool_calls" } else { "stop" }
        }],
        "usage": {
            "prompt_tokens": usage.get("input_tokens").cloned().unwrap_or_else(|| json!(0)),
            "completion_tokens": usage.get("output_tokens").cloned().unwrap_or_else(|| json!(0)),
            "total_tokens": usage.get("total_tokens").cloned().unwrap_or_else(|| json!(0)),
            "prompt_tokens_details": usage.get("input_tokens_details").cloned().unwrap_or_else(|| json!({})),
            "completion_tokens_details": usage.get("output_tokens_details").cloned().unwrap_or_else(|| json!({}))
        }
    })
}
