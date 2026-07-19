use std::collections::{HashMap, HashSet};

use chrono::DateTime;
use mcp_link_agent_wasm_sdk::{
    export_plugin, jsonl_records, paginate_sourced_messages, set_message_page, AgentPlugin, Host,
    MessagePageRequest, DEFAULT_SESSION_PAGE_BYTES,
};
use serde_json::{json, Map, Value};

mod management;

struct ReasonixPlugin;

#[derive(Clone)]
struct TopicSession {
    topic_id: String,
    workspace: String,
    file: String,
    meta: Value,
    active: bool,
}

impl AgentPlugin for ReasonixPlugin {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(params, false),
            "loadSession" => load_session(params),
            "exportNative" => export_native(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            _ => Err(format!("Unsupported Reasonix plugin method: {method}")),
        }
    }
}

export_plugin!(ReasonixPlugin);

fn list_sessions(params: &Value, full: bool) -> Result<Value, String> {
    let sessions = topic_sessions(params)?
        .iter()
        .map(|topic| parse_session(topic, full))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Array(sessions))
}

fn load_session(params: &Value) -> Result<Value, String> {
    let native_id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Reasonix nativeId is required".to_string())?;
    let topic = topic_sessions(params)?
        .into_iter()
        .find(|topic| topic.topic_id == native_id)
        .ok_or_else(|| format!("Reasonix session not found: {native_id}"))?;
    if MessagePageRequest::requested(params) {
        parse_session_page(&topic, params)
    } else {
        parse_session(&topic, true)
    }
}

fn parse_session_page(topic: &TopicSession, params: &Value) -> Result<Value, String> {
    let request = MessagePageRequest::from_params(params);
    let mut session = parse_session(topic, false)?;
    let window = Host::file_read_window(
        "data",
        &topic.file,
        request.before,
        DEFAULT_SESSION_PAGE_BYTES,
    )?;
    let mut sourced = Vec::new();
    let model = topic.meta.get("model").and_then(Value::as_str);
    for (offset, record) in jsonl_records(&window.content, window.start) {
        for (part_index, mut message) in parse_messages(&[record], model).into_iter().enumerate() {
            message["id"] = json!(format!(
                "reasonix-message:{}:{offset}:{part_index}",
                topic.topic_id
            ));
            sourced.push((offset, message));
        }
    }
    let (messages, cursor, has_more) =
        paginate_sourced_messages(sourced, request.limit, window.start);
    set_message_page(&mut session, messages, cursor, has_more)?;
    Ok(session)
}

fn export_native(params: &Value) -> Result<Value, String> {
    let native_id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Reasonix nativeId is required".to_string())?;
    let topic = topic_sessions(params)?
        .into_iter()
        .find(|topic| topic.topic_id == native_id)
        .ok_or_else(|| format!("Reasonix session not found: {native_id}"))?;
    let events_file = events_file(&topic.file);
    let (file_name, content) = match Host::file_read("data", &events_file) {
        Ok(content) if !content.trim().is_empty() => (
            events_file
                .rsplit('/')
                .next()
                .unwrap_or("session.events.jsonl"),
            content,
        ),
        _ => (
            topic.file.rsplit('/').next().unwrap_or("session.jsonl"),
            Host::file_read("data", &topic.file)?,
        ),
    };
    Ok(json!({
        "fileName": file_name,
        "content": content,
        "encoding": "utf8",
    }))
}

fn topic_sessions(params: &Value) -> Result<Vec<TopicSession>, String> {
    let index = read_json("desktop-projects.json");
    let deleted = index
        .get("deletedTopics")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    let mut ordered = Vec::<(String, String)>::new();
    if let Some(root) = instance_root(params) {
        let global_workspace = join_path(root, "global-workspace");
        for topic_id in index
            .get("globalTopics")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !deleted.contains(topic_id) {
                ordered.push((topic_id.to_string(), global_workspace.clone()));
            }
        }
    }
    for project in index
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(workspace) = project.get("root").and_then(Value::as_str) else {
            continue;
        };
        for topic_id in project
            .get("topics")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !deleted.contains(topic_id) {
                ordered.push((topic_id.to_string(), workspace.to_string()));
            }
        }
    }
    let allowed = ordered
        .iter()
        .map(|(topic_id, workspace)| (topic_id.clone(), workspace.clone()))
        .collect::<HashMap<_, _>>();
    let mut selected = HashMap::<String, (String, Value, (i64, u64))>::new();
    for file in session_files()? {
        let meta = read_json(&format!("{file}.meta"));
        let Some(topic_id) = meta.get("topic_id").and_then(Value::as_str) else {
            continue;
        };
        if !allowed.contains_key(topic_id) {
            continue;
        }
        let score = (
            timestamp(meta.get("updated_at"))
                .or_else(|| timestamp(meta.get("created_at")))
                .unwrap_or_default(),
            meta.get("revision")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        );
        let replace = selected
            .get(topic_id)
            .is_none_or(|(_, _, current_score)| score > *current_score);
        if replace {
            selected.insert(topic_id.to_string(), (file, meta, score));
        }
    }
    let active_topic = active_topic_id();
    Ok(ordered
        .into_iter()
        .filter_map(|(topic_id, workspace)| {
            let (file, meta, _) = selected.remove(&topic_id)?;
            Some(TopicSession {
                active: active_topic.as_deref() == Some(topic_id.as_str()),
                topic_id,
                workspace,
                file,
                meta,
            })
        })
        .collect())
}

fn session_files() -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    collect_session_directory("sessions", &mut files)?;
    let projects = match Host::file_list("data", "projects") {
        Ok(Value::Array(entries)) => entries,
        _ => Vec::new(),
    };
    for project in projects {
        if project.get("directory").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(name) = project.get("name").and_then(Value::as_str) else {
            continue;
        };
        collect_session_directory(&format!("projects/{name}/sessions"), &mut files)?;
    }
    Ok(files)
}

fn collect_session_directory(path: &str, output: &mut Vec<String>) -> Result<(), String> {
    let entries = match Host::file_list("data", path) {
        Ok(Value::Array(entries)) => entries,
        _ => return Ok(()),
    };
    for entry in entries {
        if entry.get("directory").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(name) = entry.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.ends_with(".jsonl")
            && !name.ends_with(".events.jsonl")
            && !name.ends_with(".conflicts.jsonl")
        {
            output.push(format!("{path}/{name}"));
        }
    }
    Ok(())
}

fn parse_session(topic: &TopicSession, full: bool) -> Result<Value, String> {
    let meta = &topic.meta;
    let title = ["topic_title", "preview", "name"]
        .into_iter()
        .find_map(|key| meta.get(key).and_then(Value::as_str))
        .map(truncate_title)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Untitled Reasonix session".to_string());
    let records = if full {
        read_session_records(&topic.file)?
    } else {
        Vec::new()
    };
    let messages = if full {
        parse_messages(&records, meta.get("model").and_then(Value::as_str))
    } else {
        Vec::new()
    };
    let message_count = if full {
        messages.len()
    } else {
        meta.get("turns")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize
    };
    Ok(json!({
        "id": format!("reasonix:{}", topic.topic_id),
        "agentId": "reasonix",
        "nativeId": topic.topic_id.clone(),
        "nativeSessionId": topic.topic_id.clone(),
        "sourceInstanceId": Value::Null,
        "sourceLabel": Value::Null,
        "title": title,
        "cwd": topic.workspace.clone(),
        "repository": topic.workspace.clone(),
        "model": meta.get("model").cloned().unwrap_or(Value::Null),
        "createdAt": timestamp(meta.get("created_at")),
        "updatedAt": timestamp(meta.get("updated_at")),
        "messageCount": message_count,
        "sourceRef": topic.file.clone(),
        "parentNativeId": Value::Null,
        "active": topic.active,
        "messages": messages,
        "rawMetadata": {
            "reasonixMeta": meta,
            "reasonixTopicId": topic.topic_id.clone(),
            "reasonixSessionFile": topic.file.clone(),
        },
    }))
}

fn instance_root(params: &Value) -> Option<&str> {
    params
        .get("instance")
        .and_then(|instance| instance.get("cliRoot"))
        .and_then(Value::as_str)
}

fn join_path(root: &str, child: &str) -> String {
    let separator = if root.contains('\\') { "\\" } else { "/" };
    format!(
        "{}{}{}",
        root.trim_end_matches(['\\', '/']),
        separator,
        child,
    )
}

fn active_topic_id() -> Option<String> {
    let tabs = read_json("desktop-tabs.json");
    let active_tab = tabs.get("activeTab").and_then(Value::as_str)?;
    tabs.get("tabs")
        .and_then(Value::as_array)?
        .iter()
        .find(|tab| tab.get("id").and_then(Value::as_str) == Some(active_tab))
        .and_then(|tab| tab.get("topicId"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn events_file(file: &str) -> String {
    file.strip_suffix(".jsonl")
        .map(|base| format!("{base}.events.jsonl"))
        .unwrap_or_else(|| format!("{file}.events.jsonl"))
}

fn read_session_records(file: &str) -> Result<Vec<Value>, String> {
    let events = events_file(file);
    if let Ok(content) = Host::file_read("data", &events) {
        if !content.trim().is_empty() {
            let mut messages = Vec::new();
            for event in content
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            {
                let event_messages = event
                    .get("messages")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                match event.get("type").and_then(Value::as_str) {
                    Some("replace") => messages = event_messages,
                    Some("append") => {
                        let index = event
                            .get("message_index")
                            .and_then(Value::as_u64)
                            .unwrap_or(messages.len() as u64)
                            as usize;
                        messages.truncate(index.min(messages.len()));
                        messages.extend(event_messages);
                    }
                    _ => {}
                }
            }
            return Ok(messages);
        }
    }
    read_json_lines(file)
}

fn parse_messages(records: &[Value], model: Option<&str>) -> Vec<Value> {
    let mut messages = Vec::new();
    for record in records {
        let role = record.get("role").and_then(Value::as_str).unwrap_or("user");
        let message_timestamp = timestamp(record.get("timestamp"));
        if role == "assistant" {
            if let Some(reasoning) = record
                .get("reasoning_content")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                messages.push(message(
                    messages.len(),
                    "assistant",
                    "reasoning",
                    Some(reasoning),
                    model,
                    message_timestamp,
                ));
            }
            let text = content_text(record.get("content"));
            if !text.trim().is_empty() {
                messages.push(message(
                    messages.len(),
                    "assistant",
                    "text",
                    Some(&text),
                    model,
                    message_timestamp,
                ));
            }
            for call in record
                .get("tool_calls")
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
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "toolName".to_string(),
                        call.get("name").cloned().unwrap_or(Value::Null),
                    );
                    object.insert(
                        "toolInput".to_string(),
                        embedded_json(call.get("arguments")),
                    );
                    object.insert(
                        "toolCallId".to_string(),
                        call.get("id").cloned().unwrap_or(Value::Null),
                    );
                }
                messages.push(value);
            }
        } else if role == "tool" {
            let mut value = message(
                messages.len(),
                "tool",
                "tool-result",
                None,
                model,
                message_timestamp,
            );
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "toolName".to_string(),
                    record.get("name").cloned().unwrap_or(Value::Null),
                );
                object.insert(
                    "toolOutput".to_string(),
                    embedded_json(record.get("content")),
                );
                object.insert(
                    "toolCallId".to_string(),
                    record.get("tool_call_id").cloned().unwrap_or(Value::Null),
                );
            }
            messages.push(value);
        } else {
            let text = content_text(record.get("content"));
            if !text.trim().is_empty() {
                messages.push(message(
                    messages.len(),
                    if role == "system" { "system" } else { "user" },
                    if role == "system" { "system" } else { "text" },
                    Some(&text),
                    None,
                    message_timestamp,
                ));
            }
        }
    }
    messages
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

fn read_json(path: &str) -> Value {
    Host::file_read("data", path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(|| Value::Object(Map::new()))
}

fn read_json_lines(path: &str) -> Result<Vec<Value>, String> {
    Ok(Host::file_read("data", path)?
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
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

fn embedded_json(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(value)) => {
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.clone()))
        }
        Some(value) => value.clone(),
        None => Value::Null,
    }
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
