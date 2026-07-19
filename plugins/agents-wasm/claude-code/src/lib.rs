use std::collections::HashSet;

use chrono::DateTime;
use mcp_link_agent_wasm_sdk::{
    export_plugin, jsonl_records, paginate_sourced_messages, scan_jsonl_reverse, set_message_page,
    AgentPlugin, Host, MessagePageRequest, DEFAULT_SESSION_PAGE_BYTES,
};
use serde_json::{json, Value};

mod management;

struct ClaudeCode;

impl AgentPlugin for ClaudeCode {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(),
            "loadSession" => load_session(params),
            "loadSessionStats" => load_session_stats(params),
            "resumeCommand" => resume_command(params),
            "duplicateSession" => duplicate_session(params),
            "deleteSession" => delete_session(params),
            "exportNative" => export_native(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            _ => Err(format!("Unsupported ClaudeCode method: {method}")),
        }
    }
}

export_plugin!(ClaudeCode);

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
    if MessagePageRequest::requested(params) {
        parse_session_page(path, params)
    } else {
        parse_session(path, true)
    }
}

#[derive(Default)]
struct UsageTotals {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    found: bool,
}

fn load_session_stats(params: &Value) -> Result<Value, String> {
    let path = required_native_id(params)?;
    let mut seen = HashSet::new();
    let mut totals = UsageTotals::default();
    scan_jsonl_reverse("sessions", path, DEFAULT_SESSION_PAGE_BYTES, |value| {
        add_usage(value, &mut seen, &mut totals);
        true
    })?;
    Ok(usage_stats(&totals).unwrap_or(Value::Null))
}

fn add_usage(value: &Value, seen: &mut HashSet<String>, totals: &mut UsageTotals) {
    let Some(message) = value.get("message") else {
        return;
    };
    let Some(id) = message
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    else {
        return;
    };
    let Some(usage) = message.get("usage") else {
        return;
    };
    if !seen.insert(id.to_string()) {
        return;
    }
    totals.found = true;
    totals.input = totals
        .input
        .saturating_add(token_value(usage, "input_tokens"));
    totals.output = totals
        .output
        .saturating_add(token_value(usage, "output_tokens"));
    totals.cache_read = totals
        .cache_read
        .saturating_add(token_value(usage, "cache_read_input_tokens"));
    totals.cache_write = totals
        .cache_write
        .saturating_add(token_value(usage, "cache_creation_input_tokens"));
}

fn token_value(usage: &Value, key: &str) -> u64 {
    usage.get(key).and_then(Value::as_u64).unwrap_or_default()
}

fn usage_stats(totals: &UsageTotals) -> Option<Value> {
    totals.found.then(|| {
        let total = totals
            .input
            .saturating_add(totals.output)
            .saturating_add(totals.cache_read)
            .saturating_add(totals.cache_write);
        json!({
            "inputTokens": totals.input,
            "outputTokens": totals.output,
            "cachedInputTokens": totals.cache_read,
            "cacheWriteTokens": totals.cache_write,
            "totalTokens": total,
            "source": "reported",
        })
    })
}

fn parse_session(path: &str, include_messages: bool) -> Result<Value, String> {
    let content = if include_messages {
        Host::file_read("sessions", path)?
    } else {
        Host::file_read_head("sessions", path, 512 * 1024)?
    };
    Ok(parse_session_content(path, &content, include_messages, 0))
}

fn parse_session_page(path: &str, params: &Value) -> Result<Value, String> {
    let request = MessagePageRequest::from_params(params);
    let mut session = parse_session(path, false)?;
    let window =
        Host::file_read_window("sessions", path, request.before, DEFAULT_SESSION_PAGE_BYTES)?;
    let page_session = parse_session_content(path, &window.content, true, window.start);
    for key in ["model", "updatedAt"] {
        if let Some(value) = page_session.get(key).filter(|value| !value.is_null()) {
            session[key] = value.clone();
        }
    }
    let sourced = page_session
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| {
            let cursor = message
                .get("id")
                .and_then(Value::as_str)?
                .rsplit(':')
                .next()?
                .parse::<u64>()
                .ok()?;
            Some((cursor, message.clone()))
        })
        .collect();
    let (messages, cursor, has_more) =
        paginate_sourced_messages(sourced, request.limit, window.start);
    set_message_page(&mut session, messages, cursor, has_more)?;
    Ok(session)
}

fn parse_session_content(
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
    let line_limit = if include_messages { usize::MAX } else { 128 };
    for (index, value) in jsonl_records(content, base_offset)
        .into_iter()
        .take(line_limit)
    {
        let timestamp = timestamp_millis(value.get("timestamp"));
        created_at = min_time(created_at, timestamp);
        updated_at = max_time(updated_at, timestamp);
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
            native_id = id.to_string();
        }
        if let Some(value_cwd) = value.get("cwd").and_then(Value::as_str) {
            cwd = Some(value_cwd.to_string());
        }
        let message = value.get("message").unwrap_or(&value);
        let payload = value.get("payload").unwrap_or(message);
        if let Some(value_model) = message
            .get("model")
            .and_then(Value::as_str)
            .or_else(|| value.get("model").and_then(Value::as_str))
        {
            model = Some(value_model.to_string());
        }
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .or_else(|| payload.get("role").and_then(Value::as_str))
            .or_else(|| value.get("role").and_then(Value::as_str));
        let text = message_text(message)
            .or_else(|| message_text(payload))
            .or_else(|| message_text(&value));
        if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
            if is_internal_prompt(&text) {
                continue;
            }
            if title.is_none() && role == Some("user") && !is_internal_prompt(&text) {
                title = Some(truncate(text.trim(), 120));
            }
            if include_messages {
                messages.push(json!({
                    "id": format!("claude-code-message:{path}:{index}"),
                    "role": role.unwrap_or("assistant"),
                    "kind": if kind.contains("reason") { "reasoning" } else { "text" },
                    "text": text,
                    "toolName": null,
                    "toolInput": null,
                    "toolOutput": null,
                    "toolCallId": null,
                    "model": model,
                    "timestamp": timestamp,
                    "rawType": kind,
                    "attachments": []
                }));
            }
        }
    }
    json!({
        "id": format!("claude-code:{native_id}"),
        "agentId": "claude-code",
        "nativeId": path,
        "nativeSessionId": path,
        "sourceRef": path,
        "title": title.unwrap_or_else(|| "Untitled ClaudeCode session".to_string()),
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

fn resume_command(params: &Value) -> Result<Value, String> {
    let id = required_native_id(params)?;
    Ok(json!({"command":"claude","args":["--resume",id]}))
}

fn export_native(params: &Value) -> Result<Value, String> {
    let path = required_native_id(params)?;
    let content = Host::file_read("sessions", path)?;
    Ok(
        json!({"fileName":path.rsplit('/').next().unwrap_or("claude-code-session.jsonl"),"encoding":"utf8","content":content}),
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
    json!({"ok":true,"agentId":"claude-code","nativeId":native_id,"command":null,"sourceNativeId":source_native_id,"warnings":[],"backupPath":null})
}

fn required_native_id(params: &Value) -> Result<&str, String> {
    params
        .get("nativeId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "ClaudeCode nativeId is required".to_string())
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
        "<local-command-caveat",
        "<command-name>",
        "<command-message>",
        "<local-command-stdout",
        "<environment_context",
        "<system-reminder",
        "<codex_internal_context",
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

    #[no_mangle]
    extern "C" fn host_call(_: i32, _: i32, _: i32, _: i32) -> i32 {
        -1
    }

    #[test]
    fn deduplicates_usage_by_api_message_id() {
        let first = json!({
            "message": {
                "id": "msg-1",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 20,
                    "cache_creation_input_tokens": 30,
                    "cache_read_input_tokens": 40
                }
            }
        });
        let second = json!({
            "message": {
                "id": "msg-2",
                "usage": {
                    "input_tokens": 50,
                    "output_tokens": 10,
                    "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": 5
                }
            }
        });
        let mut seen = HashSet::new();
        let mut totals = UsageTotals::default();
        add_usage(&first, &mut seen, &mut totals);
        add_usage(&first, &mut seen, &mut totals);
        add_usage(&second, &mut seen, &mut totals);
        let stats = usage_stats(&totals).unwrap();
        assert_eq!(stats["inputTokens"], 150);
        assert_eq!(stats["outputTokens"], 30);
        assert_eq!(stats["cachedInputTokens"], 45);
        assert_eq!(stats["cacheWriteTokens"], 30);
        assert_eq!(stats["totalTokens"], 255);
    }

    #[test]
    fn accepts_numeric_and_rfc3339_timestamps() {
        assert_eq!(timestamp_millis(Some(&json!(123))), Some(123_000));
        assert_eq!(timestamp_millis(Some(&json!("123"))), Some(123_000));
        assert_eq!(
            timestamp_millis(Some(&json!("2026-07-08T14:10:32.044Z"))),
            Some(1_783_519_832_044),
        );
    }
}
