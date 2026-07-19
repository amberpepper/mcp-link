use chrono::DateTime;
use mcp_link_agent_wasm_sdk::{
    export_plugin, jsonl_records, paginate_sourced_messages, set_message_page, AgentPlugin, Host,
    MessagePageRequest, DEFAULT_SESSION_PAGE_BYTES,
};
use serde_json::{json, Value};

mod management;

const AGENT_ID: &str = "kimi-code";
const INDEX_FILE: &str = "session_index.jsonl";
const SUMMARY_BYTES: usize = 512 * 1024;

struct KimiCodePlugin;

impl AgentPlugin for KimiCodePlugin {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(),
            "loadSession" => load_session(params),
            "loadSessionStats" => load_session_stats(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            "exportNative" => export_native(params),
            _ => Err(format!("Unsupported Kimi Code plugin method: {method}")),
        }
    }
}

export_plugin!(KimiCodePlugin);

fn list_sessions() -> Result<Value, String> {
    let sessions = session_entries()?
        .iter()
        .filter_map(|entry| parse_session_summary(entry).ok())
        .collect::<Vec<_>>();
    Ok(Value::Array(sessions))
}

fn load_session(params: &Value) -> Result<Value, String> {
    let entry = find_session(required_native_id(params)?)?;
    if MessagePageRequest::requested(params) {
        parse_session_page(&entry, params)
    } else {
        parse_full_session(&entry)
    }
}

fn load_session_stats(params: &Value) -> Result<Value, String> {
    let entry = find_session(required_native_id(params)?)?;
    let wire = Host::file_read("data", &wire_path(&entry)?)?;
    let mut input_other = 0_u64;
    let mut output = 0_u64;
    let mut cache_read = 0_u64;
    let mut cache_creation = 0_u64;
    let mut reported = false;
    for record in read_json_lines(&wire) {
        let event = record.get("event").unwrap_or(&record);
        if event.get("type").and_then(Value::as_str) != Some("turn.step.completed") {
            continue;
        }
        let Some(usage) = event.get("usage") else {
            continue;
        };
        reported = true;
        input_other = input_other.saturating_add(number(usage.get("inputOther")));
        output = output.saturating_add(number(usage.get("output")));
        cache_read = cache_read.saturating_add(number(usage.get("inputCacheRead")));
        cache_creation = cache_creation.saturating_add(number(usage.get("inputCacheCreation")));
    }
    if !reported {
        return Ok(Value::Null);
    }
    Ok(json!({
        "inputTokens": input_other.saturating_add(cache_read).saturating_add(cache_creation),
        "outputTokens": output,
        "cachedInputTokens": cache_read,
        "cacheWriteTokens": cache_creation,
        "totalTokens": input_other
            .saturating_add(cache_read)
            .saturating_add(cache_creation)
            .saturating_add(output),
        "source": "reported",
    }))
}

fn export_native(params: &Value) -> Result<Value, String> {
    let native_id = required_native_id(params)?;
    let entry = find_session(native_id)?;
    let state = read_state(&entry);
    let wire = Host::file_read("data", &wire_path(&entry)?)?;
    let content = serde_json::to_string_pretty(&json!({
        "sessionId": native_id,
        "workDir": entry.get("workDir").cloned().unwrap_or(Value::Null),
        "state": state,
        "wire": wire,
    }))
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "fileName": format!("kimi-code-{native_id}.json"),
        "content": content,
        "encoding": "utf8",
    }))
}

fn required_native_id(params: &Value) -> Result<&str, String> {
    params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Kimi Code nativeId is required".to_string())
}

fn session_entries() -> Result<Vec<Value>, String> {
    let content = Host::file_read("data", INDEX_FILE).unwrap_or_default();
    let mut entries = content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(normalize_index_entry)
        .collect::<Vec<_>>();
    if entries.is_empty() {
        collect_session_directories("sessions", 0, &mut entries)?;
    }
    Ok(entries)
}

fn normalize_index_entry(entry: Value) -> Option<Value> {
    let native_id = entry.get("sessionId")?.as_str()?.trim();
    if native_id.is_empty() {
        return None;
    }
    let relative_dir = relative_session_dir(
        entry
            .get("sessionDir")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .unwrap_or_else(|| format!("sessions/{native_id}"));
    Some(json!({
        "nativeId": native_id,
        "relativeDir": relative_dir,
        "workDir": entry.get("workDir").cloned().unwrap_or(Value::Null),
    }))
}

fn collect_session_directories(
    path: &str,
    depth: usize,
    output: &mut Vec<Value>,
) -> Result<(), String> {
    if depth > 3 {
        return Ok(());
    }
    let entries = match Host::file_list("data", path) {
        Ok(Value::Array(entries)) => entries,
        _ => return Ok(()),
    };
    let has_state = entries.iter().any(|entry| {
        entry.get("name").and_then(Value::as_str) == Some("state.json")
            && entry.get("directory").and_then(Value::as_bool) != Some(true)
    });
    if has_state {
        let native_id = path.rsplit('/').next().unwrap_or(path);
        output.push(json!({
            "nativeId": native_id,
            "relativeDir": path,
            "workDir": Value::Null,
        }));
        return Ok(());
    }
    for entry in entries {
        if entry.get("directory").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(name) = entry.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        collect_session_directories(&format!("{path}/{name}"), depth + 1, output)?;
    }
    Ok(())
}

fn relative_session_dir(value: &str) -> Option<String> {
    let normalized = value.replace('\\', "/");
    if normalized.starts_with("sessions/") {
        return Some(normalized);
    }
    normalized
        .find("/sessions/")
        .map(|index| normalized[index + 1..].to_string())
}

fn find_session(native_id: &str) -> Result<Value, String> {
    session_entries()?
        .into_iter()
        .find(|entry| entry.get("nativeId").and_then(Value::as_str) == Some(native_id))
        .ok_or_else(|| format!("Kimi Code session not found: {native_id}"))
}

fn parse_session_summary(entry: &Value) -> Result<Value, String> {
    let wire = Host::file_read_head("data", &wire_path(entry)?, SUMMARY_BYTES).unwrap_or_default();
    base_session(entry, &read_json_lines(&wire), Vec::new())
}

fn parse_full_session(entry: &Value) -> Result<Value, String> {
    let wire = Host::file_read("data", &wire_path(entry)?)?;
    let records = jsonl_records(&wire, 0);
    let messages = records
        .iter()
        .flat_map(|(offset, record)| messages_from_record(record, *offset))
        .collect::<Vec<_>>();
    let values = records
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    base_session(entry, &values, messages)
}

fn parse_session_page(entry: &Value, params: &Value) -> Result<Value, String> {
    let wire_path = wire_path(entry)?;
    let head = Host::file_read_head("data", &wire_path, SUMMARY_BYTES).unwrap_or_default();
    let mut session = base_session(entry, &read_json_lines(&head), Vec::new())?;
    let request = MessagePageRequest::from_params(params);
    let window = Host::file_read_window(
        "data",
        &wire_path,
        request.before,
        DEFAULT_SESSION_PAGE_BYTES,
    )?;
    let sourced = jsonl_records(&window.content, window.start)
        .into_iter()
        .flat_map(|(offset, record)| {
            messages_from_record(&record, offset)
                .into_iter()
                .map(move |message| (offset, message))
        })
        .collect::<Vec<_>>();
    let (messages, cursor, has_more) =
        paginate_sourced_messages(sourced, request.limit, window.start);
    set_message_page(&mut session, messages, cursor, has_more)?;
    Ok(session)
}

fn base_session(entry: &Value, records: &[Value], messages: Vec<Value>) -> Result<Value, String> {
    let native_id = entry
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Kimi Code session index entry has no ID".to_string())?;
    let state = read_state(entry);
    let cwd = state
        .get("workDir")
        .or_else(|| state.get("cwd"))
        .filter(|value| value.is_string())
        .cloned()
        .or_else(|| entry.get("workDir").cloned())
        .unwrap_or(Value::Null);
    let title = state
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| state.get("lastPrompt").and_then(Value::as_str))
        .map(truncate_title)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Untitled Kimi Code session".to_string());
    let model = records.iter().rev().find_map(|record| {
        (record.get("type").and_then(Value::as_str) == Some("config.update"))
            .then(|| record.get("profileName").and_then(Value::as_str))
            .flatten()
    });
    let message_count = records
        .iter()
        .enumerate()
        .map(|(index, record)| messages_from_record(record, index as u64).len())
        .sum::<usize>();
    Ok(json!({
        "id": format!("{AGENT_ID}:{native_id}"),
        "agentId": AGENT_ID,
        "nativeId": native_id,
        "nativeSessionId": native_id,
        "sourceInstanceId": Value::Null,
        "sourceLabel": Value::Null,
        "title": title,
        "cwd": cwd,
        "repository": cwd,
        "model": model,
        "createdAt": timestamp(state.get("createdAt")),
        "updatedAt": timestamp(state.get("updatedAt")),
        "messageCount": if messages.is_empty() { message_count } else { messages.len() },
        "sourceRef": entry.get("relativeDir").cloned().unwrap_or(Value::Null),
        "parentNativeId": state.get("forkedFrom").cloned().unwrap_or(Value::Null),
        "active": false,
        "messages": messages,
        "rawMetadata": { "state": state },
    }))
}

fn read_state(entry: &Value) -> Value {
    let Some(directory) = entry.get("relativeDir").and_then(Value::as_str) else {
        return json!({});
    };
    Host::file_read("data", &format!("{directory}/state.json"))
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(|| json!({}))
}

fn wire_path(entry: &Value) -> Result<String, String> {
    entry
        .get("relativeDir")
        .and_then(Value::as_str)
        .map(|directory| format!("{directory}/agents/main/wire.jsonl"))
        .ok_or_else(|| "Kimi Code session directory is missing".to_string())
}

fn messages_from_record(record: &Value, offset: u64) -> Vec<Value> {
    if record.get("type").and_then(Value::as_str) != Some("context.append_message") {
        return Vec::new();
    }
    let Some(message) = record.get("message") else {
        return Vec::new();
    };
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("system");
    let content = message
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if role == "user" && is_internal_user_message(&content) {
        return Vec::new();
    }
    let model = message
        .get("model")
        .or_else(|| record.get("model"))
        .and_then(Value::as_str);
    let message_timestamp = timestamp(
        message
            .get("created_at")
            .or_else(|| record.get("time"))
            .or_else(|| record.get("timestamp")),
    );

    if role == "tool" {
        let output = content_text(&content, false);
        let mut result = base_message(
            &format!("kimi:{offset}:tool"),
            "tool",
            "tool-result",
            None,
            model,
            message_timestamp,
        );
        result["toolOutput"] = if output.is_empty() {
            Value::Array(content)
        } else {
            json!(output)
        };
        result["toolCallId"] = message
            .get("toolCallId")
            .or_else(|| message.get("tool_call_id"))
            .cloned()
            .unwrap_or(Value::Null);
        return vec![result];
    }

    let normalized_role = if matches!(role, "user" | "assistant" | "system") {
        role
    } else {
        "system"
    };
    let mut messages = Vec::new();
    for (index, part) in content.iter().enumerate() {
        let part_type = part.get("type").and_then(Value::as_str).unwrap_or_default();
        match part_type {
            "text" => {
                if let Some(text) = part
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                {
                    messages.push(base_message(
                        &format!("kimi:{offset}:{index}"),
                        normalized_role,
                        "text",
                        Some(text),
                        model,
                        message_timestamp,
                    ));
                }
            }
            "think" | "thinking" => {
                let text = part
                    .get("think")
                    .or_else(|| part.get("thinking"))
                    .and_then(Value::as_str);
                if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                    messages.push(base_message(
                        &format!("kimi:{offset}:{index}"),
                        "assistant",
                        "reasoning",
                        Some(text),
                        model,
                        message_timestamp,
                    ));
                }
            }
            "tool_use" => messages.push(tool_call_message(
                part,
                &format!("kimi:{offset}:{index}"),
                model,
                message_timestamp,
            )),
            "tool_result" => {
                let mut result = base_message(
                    &format!("kimi:{offset}:{index}"),
                    "tool",
                    "tool-result",
                    None,
                    model,
                    message_timestamp,
                );
                result["toolOutput"] = part.get("output").cloned().unwrap_or(Value::Null);
                result["toolCallId"] = part.get("tool_call_id").cloned().unwrap_or(Value::Null);
                messages.push(result);
            }
            "image" | "video" | "file" => messages.push(base_message(
                &format!("kimi:{offset}:{index}"),
                normalized_role,
                "text",
                Some(match part_type {
                    "image" => "[Image attachment]",
                    "video" => "[Video attachment]",
                    _ => "[File attachment]",
                }),
                model,
                message_timestamp,
            )),
            _ => {}
        }
    }
    if let Some(calls) = message.get("toolCalls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            let call_id = call.get("id").and_then(Value::as_str).unwrap_or_default();
            if messages.iter().any(|item| {
                item.get("toolCallId").and_then(Value::as_str) == Some(call_id)
                    && !call_id.is_empty()
            }) {
                continue;
            }
            messages.push(legacy_tool_call_message(
                call,
                &format!("kimi:{offset}:call:{index}"),
                model,
                message_timestamp,
            ));
        }
    }
    messages
}

fn tool_call_message(part: &Value, id: &str, model: Option<&str>, timestamp: Option<i64>) -> Value {
    let mut message = base_message(id, "assistant", "tool-call", None, model, timestamp);
    message["toolName"] = part.get("tool_name").cloned().unwrap_or(Value::Null);
    message["toolInput"] = part.get("input").cloned().unwrap_or(Value::Null);
    message["toolCallId"] = part.get("tool_call_id").cloned().unwrap_or(Value::Null);
    message
}

fn legacy_tool_call_message(
    call: &Value,
    id: &str,
    model: Option<&str>,
    timestamp: Option<i64>,
) -> Value {
    let function = call.get("function").unwrap_or(call);
    let input = function
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|value| serde_json::from_str(value).ok())
        .or_else(|| function.get("arguments").cloned())
        .or_else(|| call.get("input").cloned())
        .unwrap_or(Value::Null);
    let mut message = base_message(id, "assistant", "tool-call", None, model, timestamp);
    message["toolName"] = function.get("name").cloned().unwrap_or(Value::Null);
    message["toolInput"] = input;
    message["toolCallId"] = call.get("id").cloned().unwrap_or(Value::Null);
    message
}

fn base_message(
    id: &str,
    role: &str,
    kind: &str,
    text: Option<&str>,
    model: Option<&str>,
    timestamp: Option<i64>,
) -> Value {
    json!({
        "id": id,
        "role": role,
        "kind": kind,
        "text": text,
        "toolName": Value::Null,
        "toolInput": Value::Null,
        "toolOutput": Value::Null,
        "toolCallId": Value::Null,
        "model": model,
        "timestamp": timestamp,
        "rawType": kind,
        "attachments": [],
    })
}

fn is_internal_user_message(content: &[Value]) -> bool {
    let texts = content
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>();
    !texts.is_empty()
        && texts
            .iter()
            .all(|text| text.trim_start().starts_with("<system-reminder>"))
}

fn content_text(content: &[Value], reasoning: bool) -> String {
    content
        .iter()
        .filter_map(|part| {
            let part_type = part.get("type").and_then(Value::as_str)?;
            if reasoning && matches!(part_type, "think" | "thinking") {
                part.get("think")
                    .or_else(|| part.get("thinking"))
                    .and_then(Value::as_str)
            } else if !reasoning && part_type == "text" {
                part.get("text").and_then(Value::as_str)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_json_lines(content: &str) -> Vec<Value> {
    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn truncate_title(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}

fn timestamp(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(number) = value.as_i64() {
        return Some(if number < 10_000_000_000 {
            number.saturating_mul(1000)
        } else {
            number
        });
    }
    if let Some(number) = value.as_f64() {
        return Some(if number < 10_000_000_000.0 {
            (number * 1000.0) as i64
        } else {
            number as i64
        });
    }
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|date| date.timestamp_millis())
}

fn number(value: Option<&Value>) -> u64 {
    value
        .and_then(|value| value.as_u64().or_else(|| value.as_f64().map(|v| v as u64)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_windows_and_unix_session_directories() {
        assert_eq!(
            relative_session_dir(r"C:\Users\me\.kimi-code\sessions\wd_demo\session_1"),
            Some("sessions/wd_demo/session_1".to_string())
        );
        assert_eq!(
            relative_session_dir("/home/me/.kimi-code/sessions/wd_demo/session_1"),
            Some("sessions/wd_demo/session_1".to_string())
        );
    }

    #[test]
    fn parses_text_reasoning_and_tool_calls() {
        let record = json!({
            "type": "context.append_message",
            "message": {
                "role": "assistant",
                "content": [
                    { "type": "think", "think": "reason" },
                    { "type": "text", "text": "answer" }
                ],
                "toolCalls": [{
                    "id": "call-1",
                    "function": { "name": "ReadFile", "arguments": "{\"path\":\"a.rs\"}" }
                }]
            }
        });
        let messages = messages_from_record(&record, 10);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["kind"], "reasoning");
        assert_eq!(messages[1]["kind"], "text");
        assert_eq!(messages[2]["toolName"], "ReadFile");
        assert_eq!(messages[2]["toolInput"]["path"], "a.rs");
    }

    #[test]
    fn hides_injected_system_reminders() {
        let record = json!({
            "type": "context.append_message",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": "<system-reminder>internal</system-reminder>" }]
            }
        });
        assert!(messages_from_record(&record, 0).is_empty());
    }
}
