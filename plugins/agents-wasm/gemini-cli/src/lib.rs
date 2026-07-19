use chrono::DateTime;
use mcp_link_agent_wasm_sdk::{
    export_plugin, jsonl_records, paginate_sourced_messages, set_message_page, AgentPlugin, Host,
    MessagePageRequest, DEFAULT_SESSION_PAGE_BYTES,
};
use serde_json::{json, Map, Value};

mod management;

struct GeminiCliPlugin;

impl AgentPlugin for GeminiCliPlugin {
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
            "exportNative" => {
                let file = find_session(required_native_id(params)?)?;
                Ok(json!({
                    "fileName": file.rsplit('/').next().unwrap_or("session.jsonl"),
                    "content": Host::file_read("data", &file)?,
                    "encoding": "utf8",
                }))
            }
            _ => Err(format!("Unsupported Gemini CLI plugin method: {method}")),
        }
    }
}

export_plugin!(GeminiCliPlugin);

fn required_native_id(params: &Value) -> Result<&str, String> {
    params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Gemini CLI nativeId is required".to_string())
}

fn load_session(params: &Value) -> Result<Value, String> {
    let file = find_session(required_native_id(params)?)?;
    if MessagePageRequest::requested(params) {
        parse_session_page(&file, params)
    } else {
        parse_session(&file, true)
    }
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
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| file_stem(file))
                == native_id
        })
        .ok_or_else(|| format!("Gemini CLI session not found: {native_id}"))
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
    let (metadata, records) = reconstruct(values);
    let native_id = metadata
        .get("sessionId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| file_stem(file));
    let first_user = records
        .iter()
        .find(|record| record.get("type").and_then(Value::as_str) == Some("user"));
    let title = metadata
        .get("summary")
        .and_then(Value::as_str)
        .map(truncate_title)
        .filter(|title| !title.is_empty())
        .or_else(|| first_user.map(|record| truncate_title(&content_text(record.get("content")))))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Untitled Gemini CLI session".to_string());
    let model = records
        .iter()
        .rev()
        .find_map(|record| record.get("model").and_then(Value::as_str));
    let cwd = project_root(file);
    let messages = if full {
        parse_messages(&records)
    } else {
        Vec::new()
    };
    Ok(json!({
        "id": format!("gemini-cli:{native_id}"),
        "agentId": "gemini-cli",
        "nativeId": native_id,
        "nativeSessionId": native_id,
        "sourceInstanceId": Value::Null,
        "sourceLabel": Value::Null,
        "title": title,
        "cwd": cwd,
        "repository": cwd,
        "model": model,
        "createdAt": timestamp(metadata.get("startTime")),
        "updatedAt": records.last().and_then(|record| timestamp(record.get("timestamp"))),
        "messageCount": if full { messages.len() } else { records.len() },
        "sourceRef": file,
        "parentNativeId": Value::Null,
        "active": false,
        "messages": messages,
        "rawMetadata": { "conversation": metadata },
    }))
}

fn parse_session_page(file: &str, params: &Value) -> Result<Value, String> {
    let request = MessagePageRequest::from_params(params);
    let mut session = parse_session(file, false)?;
    let window = Host::file_read_window("data", file, request.before, DEFAULT_SESSION_PAGE_BYTES)?;
    let (_, records) = reconstruct_sourced(jsonl_records(&window.content, window.start));
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
            message["id"] = json!(format!("gemini-message:{file}:{offset}:{part_index}"));
            sourced.push((*offset, message));
        }
    }
    let (messages, cursor, has_more) =
        paginate_sourced_messages(sourced, request.limit, window.start);
    set_message_page(&mut session, messages, cursor, has_more)?;
    Ok(session)
}

fn reconstruct(values: Vec<Value>) -> (Map<String, Value>, Vec<Value>) {
    let mut metadata = Map::new();
    let mut records = Vec::new();
    for value in values {
        if let Some(target) = value.get("$rewindTo").and_then(Value::as_str) {
            if let Some(index) = records
                .iter()
                .position(|record: &Value| record.get("id").and_then(Value::as_str) == Some(target))
            {
                records.truncate(index);
            } else {
                records.clear();
            }
        } else if let Some(set) = value.get("$set").and_then(Value::as_object) {
            if let Some(messages) = set.get("messages").and_then(Value::as_array) {
                records = messages.clone();
            }
            metadata.extend(
                set.iter()
                    .filter(|(key, _)| key.as_str() != "messages")
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        } else if value.get("id").is_some() {
            let id = value.get("id").and_then(Value::as_str);
            if let Some(index) = records
                .iter()
                .position(|record| record.get("id").and_then(Value::as_str) == id)
            {
                records[index] = value;
            } else {
                records.push(value);
            }
        } else if let Some(object) = value.as_object() {
            if let Some(messages) = object.get("messages").and_then(Value::as_array) {
                records = messages.clone();
            }
            metadata.extend(
                object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "messages")
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
    }
    (metadata, records)
}

fn reconstruct_sourced(values: Vec<(u64, Value)>) -> (Map<String, Value>, Vec<(u64, Value)>) {
    let mut metadata = Map::new();
    let mut records = Vec::new();
    for (offset, value) in values {
        if let Some(target) = value.get("$rewindTo").and_then(Value::as_str) {
            if let Some(index) = records.iter().position(|(_, record): &(u64, Value)| {
                record.get("id").and_then(Value::as_str) == Some(target)
            }) {
                records.truncate(index);
            } else {
                records.clear();
            }
        } else if let Some(set) = value.get("$set").and_then(Value::as_object) {
            if let Some(messages) = set.get("messages").and_then(Value::as_array) {
                records = messages
                    .iter()
                    .cloned()
                    .map(|message| (offset, message))
                    .collect();
            }
            metadata.extend(
                set.iter()
                    .filter(|(key, _)| key.as_str() != "messages")
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        } else if value.get("id").is_some() {
            let id = value.get("id").and_then(Value::as_str);
            if let Some(index) = records
                .iter()
                .position(|(_, record)| record.get("id").and_then(Value::as_str) == id)
            {
                records[index] = (offset, value);
            } else {
                records.push((offset, value));
            }
        } else if let Some(object) = value.as_object() {
            if let Some(messages) = object.get("messages").and_then(Value::as_array) {
                records = messages
                    .iter()
                    .cloned()
                    .map(|message| (offset, message))
                    .collect();
            }
            metadata.extend(
                object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "messages")
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
    }
    (metadata, records)
}

fn parse_messages(records: &[Value]) -> Vec<Value> {
    let mut messages = Vec::new();
    for record in records {
        let record_type = record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let model = record.get("model").and_then(Value::as_str);
        let message_timestamp = timestamp(record.get("timestamp"));
        match record_type {
            "user" => push_text(
                &mut messages,
                "user",
                "text",
                record.get("content"),
                None,
                message_timestamp,
            ),
            "gemini" => {
                for thought in record
                    .get("thoughts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let text = ["description", "text", "subject", "summary"]
                        .into_iter()
                        .find_map(|key| thought.get(key).and_then(Value::as_str))
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| thought.to_string());
                    if !text.trim().is_empty() {
                        messages.push(message(
                            messages.len(),
                            "assistant",
                            "reasoning",
                            Some(&text),
                            model,
                            message_timestamp,
                        ));
                    }
                }
                push_text(
                    &mut messages,
                    "assistant",
                    "text",
                    record.get("content"),
                    model,
                    message_timestamp,
                );
                for call in record
                    .get("toolCalls")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let mut value = message(
                        messages.len(),
                        "assistant",
                        "tool-call",
                        None,
                        model,
                        message_timestamp,
                    );
                    let object = value.as_object_mut().unwrap();
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
                    messages.push(value);
                    if let Some(result) = call.get("result") {
                        let mut value = message(
                            messages.len(),
                            "tool",
                            "tool-result",
                            None,
                            model,
                            message_timestamp,
                        );
                        let object = value.as_object_mut().unwrap();
                        object.insert(
                            "toolName".to_string(),
                            call.get("name").cloned().unwrap_or(Value::Null),
                        );
                        object.insert("toolOutput".to_string(), result.clone());
                        object.insert(
                            "toolCallId".to_string(),
                            call.get("id").cloned().unwrap_or(Value::Null),
                        );
                        messages.push(value);
                    }
                }
            }
            "error" | "warning" | "info" => {
                push_text(
                    &mut messages,
                    "system",
                    if record_type == "error" {
                        "error"
                    } else {
                        "system"
                    },
                    record.get("content"),
                    None,
                    message_timestamp,
                );
            }
            _ => {}
        }
    }
    messages
}

fn push_text(
    messages: &mut Vec<Value>,
    role: &str,
    kind: &str,
    content: Option<&Value>,
    model: Option<&str>,
    timestamp: Option<i64>,
) {
    let text = content_text(content);
    if !text.trim().is_empty() {
        messages.push(message(
            messages.len(),
            role,
            kind,
            Some(&text),
            model,
            timestamp,
        ));
    }
}

fn message(
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

fn project_root(file: &str) -> Option<String> {
    let relative = file.strip_prefix("tmp/")?;
    let project_id = relative.split('/').next()?;
    Host::file_read("data", &format!("tmp/{project_id}/.project_root"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = name.strip_prefix("session-").unwrap_or(name);
    name.strip_suffix(".jsonl").unwrap_or(name).to_string()
}

fn content_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| {
                value.as_str().map(ToOwned::to_owned).or_else(|| {
                    value
                        .get("text")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(value) if !value.is_null() => value.to_string(),
        _ => String::new(),
    }
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
