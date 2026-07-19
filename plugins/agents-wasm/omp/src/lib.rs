use mcp_link_agent_wasm_sdk::{
    export_plugin, jsonl_records, paginate_sourced_messages, set_message_page, AgentPlugin, Host,
    MessagePageRequest, DEFAULT_SESSION_PAGE_BYTES,
};
use serde_json::{json, Value};

mod management;

struct Omp;

impl AgentPlugin for Omp {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(),
            "loadSession" => load_session(params),
            "exportNative" => export_native(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            _ => Err(format!("Unsupported OMP method: {method}")),
        }
    }
}

export_plugin!(Omp);

fn list_sessions() -> Result<Value, String> {
    Ok(Value::Array(
        session_files()?
            .iter()
            .map(|file| parse_session(file, false))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

fn load_session(params: &Value) -> Result<Value, String> {
    let native_id = required_native_id(params)?;
    let file = find_session(native_id)?;
    if MessagePageRequest::requested(params) {
        parse_session_page(&file, params)
    } else {
        parse_session(&file, true)
    }
}

fn parse_session_page(file: &str, params: &Value) -> Result<Value, String> {
    let request = MessagePageRequest::from_params(params);
    let mut session = parse_session(file, false)?;
    let window = Host::file_read_window("data", file, request.before, DEFAULT_SESSION_PAGE_BYTES)?;
    let page = parse_session_content(file, &window.content, true, window.start);
    if let Some(model) = page.get("model").filter(|value| !value.is_null()) {
        session["model"] = model.clone();
    }
    let sourced = sourced_messages(&page);
    let (messages, cursor, has_more) =
        paginate_sourced_messages(sourced, request.limit, window.start);
    set_message_page(&mut session, messages, cursor, has_more)?;
    Ok(session)
}

fn parse_session(file: &str, full: bool) -> Result<Value, String> {
    let content = if full {
        Host::file_read("data", file)?
    } else {
        Host::file_read_head("data", file, 512 * 1024)?
    };
    Ok(parse_session_content(file, &content, full, 0))
}

fn parse_session_content(file: &str, content: &str, full: bool, base_offset: u64) -> Value {
    let mut native_id = String::new();
    let mut cwd = String::new();
    let mut title = None;
    let mut model = None;
    let mut messages = Vec::new();
    for (offset, value) in jsonl_records(content, base_offset) {
        match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "session" => {
                native_id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                cwd = value
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
            }
            "session_info" => {
                title = value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            "message" => {
                let message = value.get("message").unwrap_or(&value);
                let role = message
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("assistant");
                if role == "assistant" {
                    model = message
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                if full {
                    if let Some(text) =
                        content_text(message.get("content")).filter(|text| !text.trim().is_empty())
                    {
                        messages.push(message_value(file, offset, role, &text, model.as_deref()));
                    }
                }
            }
            _ => {}
        }
    }
    if native_id.is_empty() {
        native_id = file_stem(file);
    }
    let title = title
        .or_else(|| {
            messages.iter().find_map(|message| {
                message
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|text| text.chars().take(100).collect())
            })
        })
        .unwrap_or_else(|| "Untitled OMP session".to_string());
    json!({
        "id": format!("omp:{native_id}"),
        "agentId": "omp",
        "nativeId": native_id,
        "nativeSessionId": native_id,
        "sourceRef": file,
        "title": title,
        "cwd": cwd,
        "repository": cwd,
        "model": model,
        "messageCount": if full { messages.len() } else { content.lines().count() },
        "messages": if full { Value::Array(messages) } else { Value::Array(Vec::new()) },
        "rawMetadata": {},
    })
}

fn sourced_messages(session: &Value) -> Vec<(u64, Value)> {
    session
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
        .collect()
}

fn message_value(file: &str, offset: u64, role: &str, text: &str, model: Option<&str>) -> Value {
    json!({
        "id": format!("omp-message:{file}:{offset}"),
        "role": role,
        "kind": "text",
        "text": text,
        "model": model,
        "timestamp": Value::Null,
        "attachments": [],
    })
}

fn content_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

fn required_native_id(params: &Value) -> Result<&str, String> {
    params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "OMP nativeId is required".to_string())
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
        .ok_or_else(|| format!("OMP session not found: {native_id}"))
}

fn session_files() -> Result<Vec<String>, String> {
    let mut output = Vec::new();
    collect_jsonl("sessions", &mut output)?;
    Ok(output)
}

fn collect_jsonl(path: &str, output: &mut Vec<String>) -> Result<(), String> {
    let Value::Array(entries) = Host::file_list("data", path)? else {
        return Ok(());
    };
    for entry in entries {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let child = if path.is_empty() {
            name.to_string()
        } else {
            format!("{path}/{name}")
        };
        if entry.get("directory").and_then(Value::as_bool) == Some(true) {
            collect_jsonl(&child, output)?;
        } else if name.ends_with(".jsonl") {
            output.push(child);
        }
    }
    Ok(())
}

fn export_native(params: &Value) -> Result<Value, String> {
    let file = find_session(required_native_id(params)?)?;
    Ok(json!({
        "fileName": file.rsplit('/').next().unwrap_or("session.jsonl"),
        "content": Host::file_read("data", &file)?,
        "encoding": "utf8",
    }))
}

fn file_stem(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".jsonl")
        .to_string()
}
