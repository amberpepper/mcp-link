use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use super::sse::{push_data, push_event};

pub(super) fn push_response_event(
    output: &mut String,
    event: &str,
    mut value: Value,
    sequence: &mut u64,
) {
    if let Some(object) = value.as_object_mut() {
        object.insert("sequence_number".to_string(), json!(*sequence));
    }
    *sequence += 1;
    push_event(output, event, &value);
}

pub(super) fn push_responses_started(
    output: &mut String,
    response_id: &str,
    model: &Value,
    sequence: &mut u64,
) {
    push_response_event(
        output,
        "response.created",
        json!({
            "type": "response.created",
            "response": { "id": response_id, "object": "response", "status": "in_progress", "model": model, "output": [] }
        }),
        sequence,
    );
    push_response_event(
        output,
        "response.output_item.added",
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": { "id": "msg_mcp_link", "type": "message", "role": "assistant", "status": "in_progress", "content": [] }
        }),
        sequence,
    );
    push_response_event(
        output,
        "response.content_part.added",
        json!({
            "type": "response.content_part.added",
            "item_id": "msg_mcp_link",
            "output_index": 0,
            "content_index": 0,
            "part": { "type": "output_text", "text": "", "annotations": [] }
        }),
        sequence,
    );
}

pub(super) fn push_responses_completed(
    output: &mut String,
    response_id: &str,
    model: &Value,
    text: &str,
    tools: &BTreeMap<u64, (String, String, String)>,
    usage: Option<&Value>,
    sequence: &mut u64,
) {
    push_response_event(
        output,
        "response.output_text.done",
        json!({ "type": "response.output_text.done", "item_id": "msg_mcp_link", "output_index": 0, "content_index": 0, "text": text }),
        sequence,
    );
    push_response_event(
        output,
        "response.content_part.done",
        json!({
            "type": "response.content_part.done", "item_id": "msg_mcp_link", "output_index": 0, "content_index": 0,
            "part": { "type": "output_text", "text": text, "annotations": [] }
        }),
        sequence,
    );
    let message = json!({
        "id": "msg_mcp_link", "type": "message", "status": "completed", "role": "assistant",
        "content": [{ "type": "output_text", "text": text, "annotations": [] }]
    });
    push_response_event(
        output,
        "response.output_item.done",
        json!({ "type": "response.output_item.done", "output_index": 0, "item": message }),
        sequence,
    );
    let mut response_output = vec![message];
    for (index, (call_id, name, arguments)) in tools {
        let item = json!({
            "id": format!("fc_mcp_link_{index}"), "type": "function_call", "status": "completed",
            "call_id": call_id, "name": name, "arguments": arguments
        });
        push_response_event(
            output,
            "response.function_call_arguments.done",
            json!({
                "type": "response.function_call_arguments.done", "item_id": format!("fc_mcp_link_{index}"),
                "output_index": index + 1, "name": name, "arguments": arguments
            }),
            sequence,
        );
        push_response_event(
            output,
            "response.output_item.done",
            json!({ "type": "response.output_item.done", "output_index": index + 1, "item": item }),
            sequence,
        );
        response_output.push(item);
    }
    let usage = usage
        .map(responses_usage)
        .unwrap_or_else(|| json!({ "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 }));
    push_response_event(
        output,
        "response.completed",
        json!({
            "type": "response.completed",
            "response": {
                "id": response_id, "object": "response", "status": "completed", "model": model,
                "output": response_output, "output_text": text, "usage": usage
            }
        }),
        sequence,
    );
}

pub(super) fn push_chat_chunk(
    output: &mut String,
    id: &str,
    model: &Value,
    delta: Value,
    finish_reason: Value,
    usage: Option<&Value>,
) {
    let mut chunk = Map::from_iter([
        ("id".to_string(), json!(id)),
        ("object".to_string(), json!("chat.completion.chunk")),
        ("created".to_string(), json!(0)),
        ("model".to_string(), model.clone()),
        (
            "choices".to_string(),
            json!([{ "index": 0, "delta": delta, "finish_reason": finish_reason }]),
        ),
    ]);
    if let Some(usage) = usage {
        chunk.insert("usage".to_string(), chat_usage(usage));
    }
    push_data(output, &Value::Object(chunk));
}

fn responses_usage(usage: &Value) -> Value {
    let input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({ "input_tokens": input, "output_tokens": output, "total_tokens": input + output })
}

fn chat_usage(usage: &Value) -> Value {
    let input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({ "prompt_tokens": input, "completion_tokens": output, "total_tokens": input + output })
}
