use std::collections::HashMap;

use chrono::DateTime;
use mcp_link_agent_wasm_sdk::{export_plugin, scan_jsonl_reverse, AgentPlugin, Host};
use serde_json::{json, Value};

mod management;
mod providers;

struct Codex;

const DEFAULT_MESSAGE_PAGE_SIZE: usize = 50;
const MAX_MESSAGE_PAGE_SIZE: usize = 200;
const SESSION_PAGE_BYTES: usize = 8 * 1024 * 1024;

impl AgentPlugin for Codex {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(),
            "loadSession" => load_session(params),
            "loadSessionStats" => load_session_stats(params),
            "resumeCommand" => resume_command(params),
            "duplicateSession" => duplicate_session(params),
            "deleteSession" => delete_session(params),
            "exportNative" => export_native(params),
            "loadAttachment" => load_attachment(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            _ => Err(format!("Unsupported Codex method: {method}")),
        }
    }
}

export_plugin!(Codex);

fn list_sessions() -> Result<Value, String> {
    let mut files = Vec::new();
    collect_jsonl("", &mut files)?;
    let mut sessions = files
        .into_iter()
        .filter_map(|path| parse_session(&path, false).ok())
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        right
            .get("updatedAt")
            .and_then(Value::as_i64)
            .cmp(&left.get("updatedAt").and_then(Value::as_i64))
    });
    Ok(Value::Array(sessions))
}

fn collect_jsonl(path: &str, output: &mut Vec<String>) -> Result<(), String> {
    let entries = Host::file_list("sessions", path)?;
    for entry in entries.as_array().into_iter().flatten() {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let child = if path.is_empty() {
            name.to_string()
        } else {
            format!("{path}/{name}")
        };
        if entry
            .get("directory")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            collect_jsonl(&child, output)?;
        } else if name.ends_with(".jsonl") {
            output.push(child);
        }
    }
    Ok(())
}

fn load_session(params: &Value) -> Result<Value, String> {
    let path = required_native_id(params)?;
    if params.get("limit").and_then(Value::as_u64).is_some() {
        parse_session_page(path, params)
    } else {
        parse_session(path, true)
    }
}

fn load_session_stats(params: &Value) -> Result<Value, String> {
    let path = required_native_id(params)?;
    let mut stats = None;
    scan_jsonl_reverse("sessions", path, SESSION_PAGE_BYTES, |value| {
        stats = token_count_stats(value);
        stats.is_none()
    })?;
    Ok(stats.unwrap_or(Value::Null))
}

fn token_count_stats(value: &Value) -> Option<Value> {
    let payload = value.get("payload").unwrap_or(value);
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return None;
    }
    let info = payload.get("info")?;
    let usage = info.get("total_token_usage")?;
    let input = usage.get("input_tokens").and_then(Value::as_u64)?;
    let output = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let cached = usage
        .get("cached_input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let reasoning = usage
        .get("reasoning_output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let total = usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| input.saturating_add(output));
    Some(json!({
        "inputTokens": input,
        "outputTokens": output,
        "cachedInputTokens": cached,
        "reasoningTokens": reasoning,
        "totalTokens": total,
        "contextWindow": info.get("model_context_window").and_then(Value::as_u64),
        "source": "reported",
    }))
}

fn parse_session_page(path: &str, params: &Value) -> Result<Value, String> {
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_MESSAGE_PAGE_SIZE as u64)
        .clamp(1, MAX_MESSAGE_PAGE_SIZE as u64) as usize;
    let before = params.get("before").and_then(Value::as_u64);
    let head = Host::file_read_head("sessions", path, 512 * 1024)?;
    let summary = parse_session_content_at(path, &head, false, 0);
    let window = Host::file_read_before("sessions", path, before, SESSION_PAGE_BYTES)?;
    let content = window
        .get("content")
        .and_then(Value::as_str)
        .ok_or("Codex session page returned no content")?;
    let start = window
        .get("start")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let mut page = parse_session_content_at(path, content, true, start);
    let object = page
        .as_object_mut()
        .ok_or("Codex session page is invalid")?;
    let mut messages = object
        .remove("messages")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    if messages.len() > limit {
        messages.drain(..messages.len() - limit);
    }
    let cursor = messages
        .first()
        .and_then(|message| message.get("id"))
        .and_then(Value::as_str)
        .and_then(|id| id.rsplit(':').next())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(start);
    for key in [
        "id",
        "agentId",
        "nativeId",
        "nativeSessionId",
        "sourceRef",
        "title",
        "cwd",
        "repository",
        "createdAt",
        "messageCount",
        "parentNativeId",
        "rawMetadata",
    ] {
        if let Some(value) = summary.get(key) {
            object.insert(key.to_string(), value.clone());
        }
    }
    if object.get("model").is_none_or(Value::is_null) {
        if let Some(value) = summary.get("model") {
            object.insert("model".to_string(), value.clone());
        }
    }
    object.insert("messages".to_string(), Value::Array(messages));
    object.insert("messageCursor".to_string(), json!(cursor));
    object.insert("hasMoreMessages".to_string(), json!(cursor > 0));
    Ok(page)
}

fn parse_session(path: &str, include_messages: bool) -> Result<Value, String> {
    // Listing must stay cheap even when a rollout JSONL is tens of megabytes.
    // The beginning contains session metadata and the first user prompt, which
    // is sufficient for the sidebar summary. Full content is read only when
    // the user opens the session.
    let content = if include_messages {
        Host::file_read("sessions", path)?
    } else {
        Host::file_read_head("sessions", path, 512 * 1024)?
    };
    Ok(parse_session_content_at(
        path,
        &content,
        include_messages,
        0,
    ))
}

#[cfg(test)]
fn parse_session_content(path: &str, content: &str, include_messages: bool) -> Value {
    parse_session_content_at(path, content, include_messages, 0)
}

fn parse_session_content_at(
    path: &str,
    content: &str,
    include_messages: bool,
    base_offset: u64,
) -> Value {
    let mut native_id = path.to_string();
    let mut title = None;
    let mut cwd = None;
    let mut model = None;
    let mut created_at = None;
    let mut updated_at = None;
    let mut messages = Vec::new();
    let mut tool_names = HashMap::<String, String>::new();
    let line_limit = if include_messages { usize::MAX } else { 128 };
    let mut local_offset = 0_u64;
    for segment in content.split_inclusive('\n').take(line_limit) {
        let index = base_offset.saturating_add(local_offset);
        local_offset = local_offset.saturating_add(segment.len() as u64);
        let line = segment.trim_end_matches(['\r', '\n']);
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = value.get("payload").unwrap_or(&value);
        let timestamp =
            timestamp_millis(value.get("timestamp").or_else(|| payload.get("timestamp")));
        created_at = min_time(created_at, timestamp);
        updated_at = max_time(updated_at, timestamp);
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if kind == "session_meta" {
            if let Some(id) = payload.get("id").and_then(Value::as_str) {
                native_id = id.to_string();
            }
            cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(value) = payload.get("model").and_then(Value::as_str) {
                model = Some(value.to_string());
            }
            continue;
        }
        if kind == "turn_context" {
            if let Some(value) = payload.get("model").and_then(Value::as_str) {
                model = Some(value.to_string());
            }
            if let Some(value) = payload.get("cwd").and_then(Value::as_str) {
                cwd = Some(value.to_string());
            }
            continue;
        }
        if kind == "response_item" {
            let response_type = payload
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match response_type {
                "message" => {
                    let role = payload
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("assistant");
                    let text = message_text(payload).filter(|value| !value.trim().is_empty());
                    if role == "user" && text.as_deref().is_some_and(is_internal_prompt) {
                        continue;
                    }
                    if title.is_none() && role == "user" {
                        if let Some(value) = text.as_deref() {
                            title = Some(truncate(value.trim(), 120));
                        }
                    }
                    let attachments = attachments(payload, "content", index);
                    if include_messages && (text.is_some() || !attachments.is_empty()) {
                        messages.push(message(
                            &native_id,
                            index,
                            role,
                            "text",
                            text,
                            model.as_deref(),
                            timestamp,
                            response_type,
                            attachments,
                        ));
                    }
                }
                "reasoning" => {
                    if include_messages {
                        if let Some(text) =
                            reasoning_text(payload).filter(|value| !value.trim().is_empty())
                        {
                            messages.push(message(
                                &native_id,
                                index,
                                "assistant",
                                "reasoning",
                                Some(text),
                                model.as_deref(),
                                timestamp,
                                response_type,
                                Vec::new(),
                            ));
                        }
                    }
                }
                value if is_tool_call_type(value) => {
                    let call_id = tool_call_id(payload);
                    let tool_name = payload
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| tool_name_from_type(value));
                    if let Some(call_id) = call_id.as_deref() {
                        tool_names.insert(call_id.to_string(), tool_name.clone());
                    }
                    if include_messages {
                        messages.push(json!({
                            "id": format!("{native_id}:{index}"),
                            "role": "assistant",
                            "kind": "tool-call",
                            "text": null,
                            "toolName": tool_name,
                            "toolInput": normalized_tool_value(
                                payload
                                    .get("arguments")
                                    .or_else(|| payload.get("input"))
                                    .or_else(|| payload.get("action")),
                            ),
                            "toolOutput": null,
                            "toolCallId": call_id,
                            "model": model,
                            "timestamp": timestamp,
                            "rawType": response_type,
                            "attachments": attachments(payload, "input", index),
                        }));
                    }
                }
                value if is_tool_output_type(value) => {
                    let call_id = tool_call_id(payload);
                    let tool_name = call_id
                        .as_deref()
                        .and_then(|value| tool_names.get(value))
                        .cloned();
                    if include_messages {
                        messages.push(json!({
                            "id": format!("{native_id}:{index}"),
                            "role": "tool",
                            "kind": "tool-result",
                            "text": null,
                            "toolName": tool_name,
                            "toolInput": null,
                            "toolOutput": normalized_tool_value(payload.get("output")),
                            "toolCallId": call_id,
                            "model": model,
                            "timestamp": timestamp,
                            "rawType": response_type,
                            "attachments": attachments(payload, "output", index),
                        }));
                    }
                }
                _ => {}
            }
            continue;
        }

        // Preserve text-based records from older Codex rollout formats. Current
        // event_msg records mirror response_item records and are intentionally
        // skipped here to avoid rendering every user/assistant message twice.
        if kind != "event_msg" {
            let role = payload
                .get("role")
                .and_then(Value::as_str)
                .or_else(|| value.get("role").and_then(Value::as_str));
            let text = message_text(payload).or_else(|| message_text(&value));
            if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                if role == Some("user") && is_internal_prompt(&text) {
                    continue;
                }
                if title.is_none() && role == Some("user") {
                    title = Some(truncate(text.trim(), 120));
                }
                if include_messages {
                    messages.push(message(
                        &native_id,
                        index,
                        role.unwrap_or("assistant"),
                        if kind.contains("reason") {
                            "reasoning"
                        } else {
                            "text"
                        },
                        Some(text),
                        model.as_deref(),
                        timestamp,
                        kind,
                        attachments(payload, "content", index),
                    ));
                }
            }
        }
    }
    json!({
        "id": format!("codex:{native_id}"),
        "agentId": "codex",
        "nativeId": path,
        "nativeSessionId": path,
        "sourceRef": path,
        "title": title.unwrap_or_else(|| "Untitled Codex session".to_string()),
        "cwd": cwd,
        "repository": cwd,
        "model": model,
        "createdAt": created_at,
        "updatedAt": updated_at,
        "messageCount": if include_messages { messages.len() } else { content.lines().count() },
        "parentNativeId": null,
        "messages": messages,
        "rawMetadata": { "sessionId": native_id }
    })
}

fn message(
    native_id: &str,
    index: u64,
    role: &str,
    kind: &str,
    text: Option<String>,
    model: Option<&str>,
    timestamp: Option<i64>,
    raw_type: &str,
    attachments: Vec<Value>,
) -> Value {
    json!({
        "id": format!("{native_id}:{index}"),
        "role": role,
        "kind": kind,
        "text": text,
        "toolName": null,
        "toolInput": null,
        "toolOutput": null,
        "toolCallId": null,
        "model": model,
        "timestamp": timestamp,
        "rawType": raw_type,
        "attachments": attachments,
    })
}

fn is_tool_call_type(value: &str) -> bool {
    value.ends_with("_call") || matches!(value, "tool_call" | "mcp_tool_call")
}

fn is_tool_output_type(value: &str) -> bool {
    value.ends_with("_call_output") || matches!(value, "tool_result" | "mcp_tool_result")
}

fn tool_name_from_type(value: &str) -> String {
    value
        .strip_suffix("_call")
        .unwrap_or(value)
        .replace('_', "-")
}

fn tool_call_id(payload: &Value) -> Option<String> {
    payload
        .get("call_id")
        .or_else(|| payload.get("tool_call_id"))
        .or_else(|| payload.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn normalized_tool_value(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    if let Some(text) = value.as_str() {
        return serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()));
    }
    if let Some(items) = value.as_array() {
        let texts = items
            .iter()
            .map(|item| {
                item.get("text").and_then(Value::as_str).filter(|_| {
                    matches!(
                        item.get("type").and_then(Value::as_str),
                        Some("input_text" | "output_text" | "text")
                    )
                })
            })
            .collect::<Option<Vec<_>>>();
        if let Some(texts) = texts {
            return Value::String(texts.join("\n"));
        }
    }
    value.clone()
}

fn reasoning_text(value: &Value) -> Option<String> {
    text_from_items(value.get("summary"))
        .or_else(|| text_from_items(value.get("content")))
        .or_else(|| value.get("text").and_then(Value::as_str).map(str::to_owned))
}

fn text_from_items(value: Option<&Value>) -> Option<String> {
    let text = value?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn attachments(payload: &Value, field: &str, line_index: u64) -> Vec<Value> {
    payload
        .get(field)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(item_index, item)| attachment(item, field, line_index, item_index))
        .collect()
}

fn attachment(item: &Value, field: &str, line_index: u64, item_index: usize) -> Option<Value> {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
    if !item_type.contains("image") || !has_supported_image_data(item) {
        return None;
    }
    let mime_type = image_mime_type(item);
    let extension = match mime_type.as_deref() {
        Some("image/jpeg") => "jpg",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        Some("image/svg+xml") => "svg",
        _ => "png",
    };
    Some(json!({
        "id": format!("image-{line_index}-{field}-{item_index}"),
        "kind": "image",
        "name": format!("codex-image-{line_index}-{item_index}.{extension}"),
        "mimeType": mime_type,
        "size": image_size(item),
        "reference": format!("jsonl-byte:{line_index}:{field}:{item_index}"),
    }))
}

fn has_supported_image_data(item: &Value) -> bool {
    image_url(item).is_some()
        || (item.get("data").and_then(Value::as_str).is_some() && image_mime_type(item).is_some())
}

fn image_url(item: &Value) -> Option<&str> {
    ["image_url", "data_url", "url", "source"]
        .iter()
        .find_map(|key| item.get(*key).and_then(Value::as_str))
        .filter(|value| {
            value.starts_with("data:")
                || value.starts_with("http://")
                || value.starts_with("https://")
        })
}

fn image_mime_type(item: &Value) -> Option<String> {
    item.get("mime_type")
        .or_else(|| item.get("mimeType"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            image_url(item)
                .and_then(|value| value.strip_prefix("data:"))
                .and_then(|value| value.split([';', ',']).next())
                .filter(|value| value.starts_with("image/"))
                .map(str::to_owned)
        })
}

fn image_size(item: &Value) -> Option<usize> {
    let value = image_url(item).or_else(|| item.get("data").and_then(Value::as_str))?;
    Some(value.len())
}

fn load_attachment(params: &Value) -> Result<Value, String> {
    let path = required_native_id(params)?;
    let attachment = params
        .get("attachment")
        .ok_or("Codex attachment is required")?;
    let reference = attachment
        .get("reference")
        .and_then(Value::as_str)
        .ok_or("Codex attachment reference is required")?;
    let (content, content_offset) = if let Some(offset) = reference_byte_offset(reference) {
        let window = Host::file_read_range("sessions", path, offset, SESSION_PAGE_BYTES)?;
        (
            window
                .get("content")
                .and_then(Value::as_str)
                .ok_or("Codex attachment page returned no content")?
                .to_string(),
            offset,
        )
    } else {
        (Host::file_read("sessions", path)?, 0)
    };
    let data_url = attachment_data_url_at(&content, reference, content_offset)?;
    Ok(json!({
        "id": attachment.get("id"),
        "name": attachment.get("name"),
        "mimeType": attachment.get("mimeType"),
        "dataUrl": data_url,
    }))
}

#[cfg(test)]
fn attachment_data_url(content: &str, reference: &str) -> Result<String, String> {
    attachment_data_url_at(content, reference, 0)
}

fn attachment_data_url_at(
    content: &str,
    reference: &str,
    content_offset: u64,
) -> Result<String, String> {
    let mut parts = reference.split(':');
    let reference_kind = parts
        .next()
        .ok_or("Unsupported Codex attachment reference")?;
    let location = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or("Invalid Codex attachment line")?;
    let field = parts.next().ok_or("Invalid Codex attachment field")?;
    if !matches!(field, "content" | "input" | "output") {
        return Err("Invalid Codex attachment field".to_string());
    }
    let item_index = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or("Invalid Codex attachment index")?;
    if parts.next().is_some() {
        return Err("Invalid Codex attachment reference".to_string());
    }
    let line = match reference_kind {
        "jsonl-byte" => {
            let local = location
                .checked_sub(content_offset)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or("Codex attachment offset is invalid")?;
            content
                .get(local..)
                .and_then(|value| value.lines().next())
                .ok_or("Codex attachment line was not found")?
        }
        "jsonl" => content
            .lines()
            .nth(location as usize)
            .ok_or("Codex attachment line was not found")?,
        _ => return Err("Unsupported Codex attachment reference".to_string()),
    };
    let value: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
    let payload = value.get("payload").unwrap_or(&value);
    let item = payload
        .get(field)
        .and_then(Value::as_array)
        .and_then(|items| items.get(item_index))
        .ok_or("Codex attachment item was not found")?;
    if let Some(value) = image_url(item) {
        return Ok(value.to_string());
    }
    let data = item
        .get("data")
        .and_then(Value::as_str)
        .ok_or("Codex attachment has no image data")?;
    let mime_type = image_mime_type(item).ok_or("Codex attachment has no MIME type")?;
    Ok(format!("data:{mime_type};base64,{data}"))
}

fn reference_byte_offset(reference: &str) -> Option<u64> {
    let mut parts = reference.split(':');
    (parts.next() == Some("jsonl-byte"))
        .then(|| parts.next()?.parse::<u64>().ok())
        .flatten()
}

fn resume_command(params: &Value) -> Result<Value, String> {
    let id = required_native_id(params)?;
    Ok(json!({"command":"codex","args":["resume",id]}))
}

fn export_native(params: &Value) -> Result<Value, String> {
    let path = required_native_id(params)?;
    let content = Host::file_read("sessions", path)?;
    Ok(
        json!({"fileName":path.rsplit('/').next().unwrap_or("codex-session.jsonl"),"encoding":"utf8","content":content}),
    )
}

fn duplicate_session(params: &Value) -> Result<Value, String> {
    let path = required_native_id(params)?;
    let content = Host::file_read("sessions", path)?;
    let target = format!("{path}.copy.jsonl");
    Host::file_write("sessions", &target, &content)?;
    Ok(operation(&target, Some(path)))
}

fn delete_session(params: &Value) -> Result<Value, String> {
    let path = required_native_id(params)?;
    Host::file_remove("sessions", path)?;
    Ok(operation(path, None))
}

fn operation(native_id: &str, source_native_id: Option<&str>) -> Value {
    json!({"ok":true,"agentId":"codex","nativeId":native_id,"command":null,"sourceNativeId":source_native_id,"warnings":[],"backupPath":null})
}

fn required_native_id(params: &Value) -> Result<&str, String> {
    params
        .get("nativeId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Codex nativeId is required".to_string())
}

fn message_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    let content = value.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let text = content
        .as_array()?
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn is_internal_prompt(value: &str) -> bool {
    let value = value.trim_start();
    [
        "<environment_context",
        "<codex_internal_context",
        "<permissions",
        "<collaboration_mode",
        "<multi_agent_mode",
        "<model_switch",
    ]
    .iter()
    .any(|prefix| value.starts_with(prefix))
}

fn timestamp_millis(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(value) = value.as_i64() {
        return Some(if value < 10_000_000_000 {
            value * 1000
        } else {
            value
        });
    }
    let value = value.as_str()?;
    if let Ok(value) = value.parse::<i64>() {
        return Some(if value < 10_000_000_000 {
            value * 1000
        } else {
            value
        });
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.timestamp_millis())
}

fn min_time(current: Option<i64>, next: Option<i64>) -> Option<i64> {
    match (current, next) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (value, None) | (None, value) => value,
    }
}
fn max_time(current: Option<i64>, next: Option<i64>) -> Option<i64> {
    match (current, next) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (value, None) | (None, value) => value,
    }
}
fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let text = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() {
        format!("{text}…")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Native unit tests do not run inside the WASM host. Supplying the imported
    // symbol lets the pure parser tests link without weakening production code.
    #[no_mangle]
    extern "C" fn host_call(_: i32, _: i32, _: i32, _: i32) -> i32 {
        -1
    }

    const IMAGE: &str = "data:image/png;base64,aGVsbG8=";

    fn fixture() -> String {
        [
            r#"{"timestamp":"2026-07-16T10:00:00Z","type":"session_meta","payload":{"id":"session-1","cwd":"/workspace"}}"#.to_string(),
            r#"{"timestamp":"2026-07-16T10:00:01.250Z","type":"turn_context","payload":{"model":"gpt-5.4"}}"#.to_string(),
            format!(r#"{{"timestamp":"2026-07-16T10:00:02Z","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"show the image"}},{{"type":"input_image","image_url":"{IMAGE}"}}]}}}}"#),
            r#"{"timestamp":"2026-07-16T10:00:03Z","type":"response_item","payload":{"type":"function_call","name":"read_file","arguments":"{\"path\":\"README.md\"}","call_id":"call-1"}}"#.to_string(),
            r#"{"timestamp":"2026-07-16T10:00:04Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":[{"type":"input_text","text":"file body"}]}}"#.to_string(),
            r#"{"timestamp":"2026-07-16T10:00:05Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"raw javascript","call_id":"call-2"}}"#.to_string(),
            r#"{"timestamp":"2026-07-16T10:00:06Z","type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call-2","output":"done"}}"#.to_string(),
            r#"{"timestamp":"2026-07-16T10:00:07Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"finished"}]}}"#.to_string(),
        ]
        .join("\n")
    }

    #[test]
    fn parses_metadata_tools_timestamps_and_images() {
        let session = parse_session_content("rollout.jsonl", &fixture(), true);
        assert_eq!(
            session.get("model").and_then(Value::as_str),
            Some("gpt-5.4")
        );
        assert_eq!(
            session.get("createdAt").and_then(Value::as_i64),
            Some(1_784_196_000_000)
        );
        assert_eq!(
            session.get("updatedAt").and_then(Value::as_i64),
            Some(1_784_196_007_000)
        );
        assert_eq!(
            session.get("title").and_then(Value::as_str),
            Some("show the image")
        );

        let messages = session.get("messages").and_then(Value::as_array).unwrap();
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[0]["attachments"].as_array().unwrap().len(), 1);
        assert_eq!(messages[0]["timestamp"].as_i64(), Some(1_784_196_002_000));
        assert_eq!(messages[1]["kind"].as_str(), Some("tool-call"));
        assert_eq!(messages[1]["toolName"].as_str(), Some("read_file"));
        assert_eq!(messages[1]["toolInput"]["path"].as_str(), Some("README.md"));
        assert_eq!(messages[2]["kind"].as_str(), Some("tool-result"));
        assert_eq!(messages[2]["toolName"].as_str(), Some("read_file"));
        assert_eq!(messages[2]["toolOutput"].as_str(), Some("file body"));
        assert_eq!(messages[3]["toolName"].as_str(), Some("exec"));
        assert_eq!(messages[4]["toolName"].as_str(), Some("exec"));
        assert_eq!(messages[5]["model"].as_str(), Some("gpt-5.4"));
    }

    #[test]
    fn loads_image_data_from_a_reference_without_putting_it_in_the_session() {
        let content = fixture();
        let session = parse_session_content("rollout.jsonl", &content, true);
        let attachment = &session["messages"][0]["attachments"][0];
        assert!(attachment.get("dataUrl").is_none());
        let reference = attachment.get("reference").and_then(Value::as_str).unwrap();
        assert_eq!(attachment_data_url(&content, reference).unwrap(), IMAGE);
    }

    #[test]
    fn accepts_numeric_and_rfc3339_timestamps() {
        assert_eq!(timestamp_millis(Some(&json!(123))), Some(123_000));
        assert_eq!(timestamp_millis(Some(&json!("123"))), Some(123_000));
        assert_eq!(
            timestamp_millis(Some(&json!("2026-07-16T10:00:00Z"))),
            Some(1_784_196_000_000),
        );
    }

    #[test]
    fn reads_reported_cumulative_token_stats() {
        let stats = token_count_stats(&json!({
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": 15656,
                        "cached_input_tokens": 12800,
                        "output_tokens": 798,
                        "reasoning_output_tokens": 317,
                        "total_tokens": 16454
                    },
                    "model_context_window": 258400
                }
            }
        }))
        .unwrap();
        assert_eq!(stats["inputTokens"], 15656);
        assert_eq!(stats["cachedInputTokens"], 12800);
        assert_eq!(stats["reasoningTokens"], 317);
        assert_eq!(stats["totalTokens"], 16454);
        assert_eq!(stats["contextWindow"], 258400);
    }
}
