use std::collections::HashMap;

use chrono::DateTime;
use mcp_link_agent_wasm_sdk::{
    export_plugin, jsonl_records, paginate_sourced_messages, set_message_page, AgentPlugin, Host,
    MessagePageRequest, DEFAULT_SESSION_PAGE_BYTES,
};
use serde_json::{json, Value};

mod management;

struct QwenCodePlugin;

impl AgentPlugin for QwenCodePlugin {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => Ok(Value::Array(
                session_files()?
                    .iter()
                    .map(|file| parse_session(file, false))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            "loadSession" => load_session(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            "exportNative" => export_native(params),
            _ => Err(format!("Unsupported Qwen Code plugin method: {method}")),
        }
    }
}

export_plugin!(QwenCodePlugin);

fn load_session(params: &Value) -> Result<Value, String> {
    let native_id = required_native_id(params)?;
    let file = find_session(native_id)?;
    if MessagePageRequest::requested(params) {
        parse_session_page(&file, params)
    } else {
        parse_session(&file, true)
    }
}

fn export_native(params: &Value) -> Result<Value, String> {
    let file = find_session(required_native_id(params)?)?;
    Ok(json!({
        "fileName": file.rsplit('/').next().unwrap_or("session.jsonl"),
        "content": Host::file_read("data", &file)?,
        "encoding": "utf8",
    }))
}

fn required_native_id(params: &Value) -> Result<&str, String> {
    params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Qwen Code nativeId is required".to_string())
}

fn find_session(native_id: &str) -> Result<String, String> {
    session_files()?
        .into_iter()
        .find(|file| {
            parse_session(file, false)
                .ok()
                .and_then(|session| {
                    session
                        .get("nativeId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .as_deref()
                == Some(native_id)
        })
        .ok_or_else(|| format!("Qwen Code session not found: {native_id}"))
}

fn session_files() -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    collect_jsonl("tmp", &mut files)?;
    Ok(files)
}

fn collect_jsonl(path: &str, output: &mut Vec<String>) -> Result<(), String> {
    let entries = match Host::file_list("data", path) {
        Ok(Value::Array(entries)) => entries,
        _ => return Ok(()),
    };
    for entry in entries {
        let Some(name) = entry.get("name").and_then(Value::as_str) else {
            continue;
        };
        let child = format!("{path}/{name}");
        if entry.get("directory").and_then(Value::as_bool) == Some(true) {
            collect_jsonl(&child, output)?;
        } else if name.ends_with(".jsonl") {
            output.push(child);
        }
    }
    Ok(())
}

fn parse_session(file: &str, full: bool) -> Result<Value, String> {
    let content = if full {
        Host::file_read("data", file)?
    } else {
        Host::file_read_head("data", file, 512 * 1024)?
    };
    let values = read_json_lines_content(&content);
    let records = active_records(&values);
    let native_id = session_id(&values, file).unwrap_or_else(|| file_stem(file));
    let custom_title = values.iter().rev().find_map(|value| {
        if value.get("type").and_then(Value::as_str) == Some("system")
            && value.get("subtype").and_then(Value::as_str) == Some("custom_title")
        {
            value
                .get("systemPayload")
                .and_then(|payload| payload.get("customTitle"))
                .and_then(Value::as_str)
        } else {
            None
        }
    });
    let first_user = records
        .iter()
        .find(|record| record.get("type").and_then(Value::as_str) == Some("user"));
    let title = custom_title
        .map(truncate_title)
        .filter(|value| !value.is_empty())
        .or_else(|| first_user.map(|record| truncate_title(&record_text(record, false))))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Untitled Qwen Code session".to_string());
    let cwd = records
        .iter()
        .find_map(|record| record.get("cwd").and_then(Value::as_str));
    let model = records
        .iter()
        .rev()
        .find_map(|record| record.get("model").and_then(Value::as_str));
    let messages = if full {
        parse_messages(&records)
    } else {
        Vec::new()
    };
    Ok(json!({
        "id": format!("qwen-code:{native_id}"),
        "agentId": "qwen-code",
        "nativeId": native_id,
        "nativeSessionId": native_id,
        "sourceInstanceId": Value::Null,
        "sourceLabel": Value::Null,
        "title": title,
        "cwd": cwd,
        "repository": cwd,
        "model": model,
        "createdAt": records.first().and_then(|record| timestamp(record.get("timestamp"))),
        "updatedAt": records.last().and_then(|record| timestamp(record.get("timestamp"))),
        "messageCount": if full { messages.len() } else { records.len() },
        "sourceRef": file,
        "parentNativeId": values.iter().find_map(|value| {
            value.get("forkedFrom").and_then(|item| item.get("sessionId"))
        }).cloned().unwrap_or(Value::Null),
        "active": false,
        "messages": messages,
        "rawMetadata": {},
    }))
}

fn parse_session_page(file: &str, params: &Value) -> Result<Value, String> {
    let request = MessagePageRequest::from_params(params);
    let mut session = parse_session(file, false)?;
    let window = Host::file_read_window("data", file, request.before, DEFAULT_SESSION_PAGE_BYTES)?;
    let records = active_records_sourced(&jsonl_records(&window.content, window.start));
    if let Some(model) = records
        .iter()
        .rev()
        .find_map(|(_, record)| record.get("model").and_then(Value::as_str))
    {
        session["model"] = json!(model);
    }
    if let Some(updated_at) = records
        .iter()
        .rev()
        .find_map(|(_, record)| timestamp(record.get("timestamp")))
    {
        session["updatedAt"] = json!(updated_at);
    }
    let mut sourced = Vec::new();
    for (offset, record) in &records {
        for (part_index, mut message) in parse_messages(std::slice::from_ref(record))
            .into_iter()
            .enumerate()
        {
            message["id"] = json!(format!("qwen-message:{file}:{offset}:{part_index}"));
            sourced.push((*offset, message));
        }
    }
    let (messages, cursor, has_more) =
        paginate_sourced_messages(sourced, request.limit, window.start);
    set_message_page(&mut session, messages, cursor, has_more)?;
    Ok(session)
}

fn active_records(values: &[Value]) -> Vec<Value> {
    let by_id = values
        .iter()
        .filter_map(|value| {
            value
                .get("uuid")
                .and_then(Value::as_str)
                .map(|id| (id, value))
        })
        .collect::<HashMap<_, _>>();
    let mut current = values.iter().rev().find(|value| {
        value.get("uuid").is_some()
            && value.get("isSidechain").and_then(Value::as_bool) != Some(true)
            && !(value.get("type").and_then(Value::as_str) == Some("system")
                && value.get("subtype").and_then(Value::as_str) == Some("custom_title"))
    });
    let mut records = Vec::new();
    while let Some(record) = current {
        records.push(record.clone());
        current = record
            .get("parentUuid")
            .and_then(Value::as_str)
            .and_then(|id| by_id.get(id).copied());
    }
    records.reverse();
    records
}

fn active_records_sourced(values: &[(u64, Value)]) -> Vec<(u64, Value)> {
    let by_id = values
        .iter()
        .filter_map(|record| {
            record
                .1
                .get("uuid")
                .and_then(Value::as_str)
                .map(|id| (id, record))
        })
        .collect::<HashMap<_, _>>();
    let mut current = values.iter().rev().find(|(_, value)| {
        value.get("uuid").is_some()
            && value.get("isSidechain").and_then(Value::as_bool) != Some(true)
            && !(value.get("type").and_then(Value::as_str) == Some("system")
                && value.get("subtype").and_then(Value::as_str) == Some("custom_title"))
    });
    let mut records = Vec::new();
    while let Some(record) = current {
        records.push(record.clone());
        current = record
            .1
            .get("parentUuid")
            .and_then(Value::as_str)
            .and_then(|id| by_id.get(id).copied());
    }
    records.reverse();
    records
}

fn parse_messages(records: &[Value]) -> Vec<Value> {
    let mut messages = Vec::new();
    for record in records {
        let record_type = record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if record_type == "system" {
            continue;
        }
        let model = record.get("model").and_then(Value::as_str);
        let message_timestamp = timestamp(record.get("timestamp"));
        let parts = record
            .get("message")
            .and_then(|message| message.get("parts"))
            .and_then(Value::as_array);
        for part in parts.into_iter().flatten() {
            if let Some(text) = part
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
            {
                let reasoning = part.get("thought").and_then(Value::as_bool) == Some(true);
                messages.push(base_message(
                    messages.len(),
                    if record_type == "user" {
                        "user"
                    } else {
                        "assistant"
                    },
                    if reasoning { "reasoning" } else { "text" },
                    Some(text),
                    if record_type == "user" { None } else { model },
                    message_timestamp,
                ));
            }
            if let Some(call) = part.get("functionCall") {
                let mut message = base_message(
                    messages.len(),
                    "assistant",
                    "tool-call",
                    None,
                    model,
                    message_timestamp,
                );
                let object = message.as_object_mut().unwrap();
                object.insert(
                    "toolName".to_string(),
                    call.get("name").cloned().unwrap_or(Value::Null),
                );
                object.insert(
                    "toolInput".to_string(),
                    call.get("args").cloned().unwrap_or(Value::Null),
                );
                object.insert(
                    "toolCallId".to_string(),
                    call.get("id").cloned().unwrap_or(Value::Null),
                );
                messages.push(message);
            }
            if let Some(response) = part.get("functionResponse") {
                let mut message = base_message(
                    messages.len(),
                    "tool",
                    "tool-result",
                    None,
                    model,
                    message_timestamp,
                );
                let object = message.as_object_mut().unwrap();
                object.insert(
                    "toolName".to_string(),
                    response.get("name").cloned().unwrap_or(Value::Null),
                );
                object.insert(
                    "toolOutput".to_string(),
                    response
                        .get("response")
                        .cloned()
                        .unwrap_or_else(|| response.clone()),
                );
                object.insert(
                    "toolCallId".to_string(),
                    response.get("id").cloned().unwrap_or(Value::Null),
                );
                messages.push(message);
            }
        }
    }
    messages
}

fn base_message(
    index: usize,
    role: &str,
    kind: &str,
    text: Option<&str>,
    model: Option<&str>,
    timestamp: Option<i64>,
) -> Value {
    json!({
        "id": format!("message-{index}"),
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

fn read_json_lines_content(content: &str) -> Vec<Value> {
    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn session_id(values: &[Value], file: &str) -> Option<String> {
    values
        .iter()
        .find_map(|value| value.get("sessionId").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .or_else(|| Some(file_stem(file)))
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = name.strip_prefix("session-").unwrap_or(name);
    name.strip_suffix(".jsonl").unwrap_or(name).to_string()
}

fn record_text(record: &Value, reasoning: bool) -> String {
    record
        .get("message")
        .and_then(|message| message.get("parts"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("thought").and_then(Value::as_bool) == Some(reasoning))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
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
            number * 1000
        } else {
            number
        });
    }
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|date| date.timestamp_millis())
}
