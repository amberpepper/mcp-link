use mcp_link_agent_wasm_sdk::{
    export_plugin, set_message_page, AgentPlugin, Host, MessagePageRequest,
};
use serde_json::{json, Value};

mod management;

struct CrushPlugin;

impl AgentPlugin for CrushPlugin {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(params),
            "loadSession" => load_session(params),
            "loadSessionStats" => load_session_stats(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            _ => Err(format!("Unsupported Crush plugin method: {method}")),
        }
    }
}

export_plugin!(CrushPlugin);

const SESSION_COLUMNS: &str = r#"
SELECT
  s.id,
  s.title,
  s.parent_session_id,
  s.created_at,
  s.updated_at,
  s.message_count,
  (
    SELECT m.model
    FROM messages m
    WHERE m.session_id = s.id
      AND m.model IS NOT NULL
      AND m.model != ''
    ORDER BY m.created_at DESC
    LIMIT 1
  ) AS model
FROM sessions s
"#;

fn list_sessions(params: &Value) -> Result<Value, String> {
    let cwd = instance_cwd(params);
    let rows = Host::sqlite_query(
        "sessions",
        &format!("{SESSION_COLUMNS} ORDER BY s.updated_at DESC"),
        &[],
    )?;
    Ok(Value::Array(
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|row| summary(row, &cwd, Vec::new()))
            .collect(),
    ))
}

fn load_session(params: &Value) -> Result<Value, String> {
    let native_id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Crush nativeId is required".to_string())?;
    let rows = Host::sqlite_query(
        "sessions",
        &format!("{SESSION_COLUMNS} WHERE s.id = ?"),
        &[json!(native_id)],
    )?;
    let row = rows
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or_else(|| format!("Crush session not found: {native_id}"))?;
    let page_request =
        MessagePageRequest::requested(params).then(|| MessagePageRequest::from_params(params));
    let (stored_messages, page_start) = if let Some(request) = page_request {
        let total = Host::sqlite_query(
            "sessions",
            "SELECT COUNT(*) AS count FROM messages WHERE session_id = ? AND COALESCE(is_summary_message, 0) != 1",
            &[json!(native_id)],
        )?
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
        let (start, count) = request.database_bounds(total);
        (
            Host::sqlite_query(
                "sessions",
                "SELECT id, role, parts, model, created_at, is_summary_message FROM messages WHERE session_id = ? AND COALESCE(is_summary_message, 0) != 1 ORDER BY created_at, id LIMIT ? OFFSET ?",
                &[json!(native_id), json!(count), json!(start)],
            )?,
            Some(start),
        )
    } else {
        (
            Host::sqlite_query(
                "sessions",
                "SELECT id, role, parts, model, created_at, is_summary_message FROM messages WHERE session_id = ? ORDER BY created_at, id",
                &[json!(native_id)],
            )?,
            None,
        )
    };
    let mut messages = Vec::new();
    for stored in stored_messages.as_array().into_iter().flatten() {
        if stored.get("is_summary_message").and_then(Value::as_i64) == Some(1) {
            continue;
        }
        let role = stored.get("role").and_then(Value::as_str).unwrap_or("user");
        let model = stored.get("model").and_then(Value::as_str);
        let timestamp = millis(stored.get("created_at"));
        let stored_id = stored
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("message");
        let parts = stored
            .get("parts")
            .and_then(Value::as_str)
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        for (part_index, part) in parts.into_iter().enumerate() {
            let message_id = format!("crush-message:{native_id}:{stored_id}:{part_index}");
            let part_type = part.get("type").and_then(Value::as_str).unwrap_or_default();
            let data = part.get("data").unwrap_or(&Value::Null);
            match part_type {
                "text" => {
                    if let Some(text) = data
                        .get("text")
                        .and_then(Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                    {
                        messages.push(message(
                            &message_id,
                            role,
                            "text",
                            Some(text),
                            model,
                            timestamp,
                        ));
                    }
                }
                "reasoning" => {
                    if let Some(text) = data
                        .get("thinking")
                        .and_then(Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                    {
                        messages.push(message(
                            &message_id,
                            "assistant",
                            "reasoning",
                            Some(text),
                            model,
                            timestamp,
                        ));
                    }
                }
                "tool_call" => {
                    let mut value = message(
                        &message_id,
                        "assistant",
                        "tool-call",
                        None,
                        model,
                        timestamp,
                    );
                    let object = value.as_object_mut().unwrap();
                    object.insert(
                        "toolName".to_string(),
                        data.get("name").cloned().unwrap_or(Value::Null),
                    );
                    object.insert(
                        "toolInput".to_string(),
                        data.get("input").cloned().unwrap_or(Value::Null),
                    );
                    object.insert(
                        "toolCallId".to_string(),
                        data.get("id").cloned().unwrap_or(Value::Null),
                    );
                    messages.push(value);
                }
                "tool_result" => {
                    let mut value =
                        message(&message_id, "tool", "tool-result", None, model, timestamp);
                    let object = value.as_object_mut().unwrap();
                    object.insert(
                        "toolName".to_string(),
                        data.get("name").cloned().unwrap_or(Value::Null),
                    );
                    object.insert(
                        "toolOutput".to_string(),
                        data.get("content")
                            .or_else(|| data.get("metadata"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                    object.insert(
                        "toolCallId".to_string(),
                        data.get("tool_call_id").cloned().unwrap_or(Value::Null),
                    );
                    messages.push(value);
                }
                "shell_command" => {
                    let text = format!(
                        "$ {}\n{}\n(exit code {})",
                        data.get("command")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        data.get("output")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        data.get("exit_code")
                            .and_then(Value::as_i64)
                            .unwrap_or_default(),
                    );
                    messages.push(message(
                        &message_id,
                        role,
                        "text",
                        Some(&text),
                        model,
                        timestamp,
                    ));
                }
                _ => {}
            }
        }
    }
    let mut session = summary(row, &instance_cwd(params), messages.clone());
    if let Some(start) = page_start {
        set_message_page(&mut session, messages, start, start > 0)?;
    }
    Ok(session)
}

fn load_session_stats(params: &Value) -> Result<Value, String> {
    let native_id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Crush nativeId is required".to_string())?;
    let rows = Host::sqlite_query(
        "sessions",
        "SELECT prompt_tokens, completion_tokens, cost FROM sessions WHERE id = ? LIMIT 1",
        &[json!(native_id)],
    )?;
    let Some(row) = rows.as_array().and_then(|rows| rows.first()) else {
        return Ok(Value::Null);
    };
    let input = row
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let output = row
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    Ok(json!({
        "inputTokens": input,
        "outputTokens": output,
        "totalTokens": input.saturating_add(output),
        "cost": row.get("cost").and_then(Value::as_f64).unwrap_or_default(),
        "source": "reported",
    }))
}

fn summary(row: &Value, cwd: &str, messages: Vec<Value>) -> Value {
    let native_id = row.get("id").and_then(Value::as_str).unwrap_or_default();
    let message_count = row
        .get("message_count")
        .and_then(Value::as_u64)
        .unwrap_or(messages.len() as u64) as usize;
    json!({
        "id": format!("crush:{native_id}"),
        "agentId": "crush",
        "nativeId": native_id,
        "nativeSessionId": native_id,
        "sourceInstanceId": Value::Null,
        "sourceLabel": Value::Null,
        "title": row.get("title").and_then(Value::as_str).filter(|title| !title.trim().is_empty()).unwrap_or("Untitled Crush session"),
        "cwd": cwd,
        "repository": cwd,
        "model": row.get("model").cloned().unwrap_or(Value::Null),
        "createdAt": millis(row.get("created_at")),
        "updatedAt": millis(row.get("updated_at")),
        "messageCount": message_count,
        "sourceRef": "sessions",
        "parentNativeId": row.get("parent_session_id").cloned().unwrap_or(Value::Null),
        "active": false,
        "messages": messages,
        "rawMetadata": {},
    })
}

fn message(
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

fn instance_cwd(params: &Value) -> String {
    let root = params
        .get("instance")
        .and_then(|instance| instance.get("cliRoot"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    root.rsplit_once(['/', '\\'])
        .map(|(parent, _)| parent)
        .unwrap_or(root)
        .to_string()
}

fn millis(value: Option<&Value>) -> Option<i64> {
    let value = value?.as_i64()?;
    Some(if value < 10_000_000_000 {
        value * 1000
    } else {
        value
    })
}
