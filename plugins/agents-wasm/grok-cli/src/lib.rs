use mcp_link_agent_wasm_sdk::{
    export_plugin, set_message_page, AgentPlugin, Host, MessagePageRequest,
};
use serde_json::{json, Value};

mod management;

struct Grok;

impl AgentPlugin for Grok {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(),
            "loadSession" => load_session(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            _ => Err(format!("Unsupported Grok CLI method: {method}")),
        }
    }
}

export_plugin!(Grok);

const SESSION_COLUMNS: &str = "SELECT id,title,model,cwd_last,cwd_at_start,created_at,updated_at,(SELECT COUNT(*) FROM messages m WHERE m.session_id = sessions.id) AS message_count FROM sessions";

fn list_sessions() -> Result<Value, String> {
    let rows = Host::sqlite_query(
        "sessions",
        &format!("{SESSION_COLUMNS} ORDER BY updated_at DESC"),
        &[],
    )?;
    Ok(Value::Array(
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|row| summary(row, Vec::new()))
            .collect(),
    ))
}

fn load_session(params: &Value) -> Result<Value, String> {
    let id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or("Grok nativeId is required")?;
    let rows = Host::sqlite_query(
        "sessions",
        &format!("{SESSION_COLUMNS} WHERE id = ?"),
        &[json!(id)],
    )?;
    let row = rows
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or("Grok session not found")?;
    let page_request =
        MessagePageRequest::requested(params).then(|| MessagePageRequest::from_params(params));
    let (stored, page_start) = if let Some(request) = page_request {
        let total = row
            .get("message_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let (start, count) = request.database_bounds(total);
        (
            Host::sqlite_query(
                "sessions",
                "SELECT seq,role,message_json,created_at FROM messages WHERE session_id = ? ORDER BY seq LIMIT ? OFFSET ?",
                &[json!(id), json!(count), json!(start)],
            )?,
            Some(start),
        )
    } else {
        (
            Host::sqlite_query(
                "sessions",
                "SELECT seq,role,message_json,created_at FROM messages WHERE session_id = ? ORDER BY seq",
                &[json!(id)],
            )?,
            None,
        )
    };
    let mut messages = Vec::new();
    for item in stored.as_array().into_iter().flatten() {
        let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
        let raw = item
            .get("message_json")
            .and_then(Value::as_str)
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
            .unwrap_or(Value::Null);
        let model = raw
            .get("model")
            .and_then(Value::as_str)
            .or_else(|| row.get("model").and_then(Value::as_str));
        let sequence = item.get("seq").and_then(Value::as_i64).unwrap_or_default();
        let message_id = format!("grok-message:{id}:{sequence}");
        append_content(
            &mut messages,
            &message_id,
            role,
            model,
            item.get("created_at").cloned().unwrap_or(Value::Null),
            &raw,
        );
    }
    let mut session = summary(row, messages.clone());
    if let Some(start) = page_start {
        set_message_page(&mut session, messages, start, start > 0)?;
    }
    Ok(session)
}

fn append_content(
    output: &mut Vec<Value>,
    message_id: &str,
    role: &str,
    model: Option<&str>,
    timestamp: Value,
    message: &Value,
) {
    let content = message.get("content").or_else(|| message.get("text"));
    match content {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            output.push(message_value(
                &format!("{message_id}:0"),
                role,
                "text",
                Some(text),
                model,
                timestamp,
            ));
        }
        Some(Value::Array(parts)) => {
            for (part_index, part) in parts.iter().enumerate() {
                let part_id = format!("{message_id}:{part_index}");
                let kind = part.get("type").and_then(Value::as_str).unwrap_or("text");
                match kind {
                    "text" | "input_text" | "output_text" => {
                        if let Some(text) = part.get("text").and_then(Value::as_str) {
                            output.push(message_value(
                                &part_id,
                                role,
                                "text",
                                Some(text),
                                model,
                                timestamp.clone(),
                            ));
                        }
                    }
                    "reasoning" | "thinking" => {
                        if let Some(text) = part
                            .get("text")
                            .or_else(|| part.get("thinking"))
                            .and_then(Value::as_str)
                        {
                            output.push(message_value(
                                &part_id,
                                "assistant",
                                "reasoning",
                                Some(text),
                                model,
                                timestamp.clone(),
                            ));
                        }
                    }
                    "tool_use" | "tool_call" => {
                        let mut value = message_value(
                            &part_id,
                            "assistant",
                            "tool-call",
                            None,
                            model,
                            timestamp.clone(),
                        );
                        let object = value.as_object_mut().unwrap();
                        object.insert(
                            "toolName".into(),
                            part.get("name")
                                .or_else(|| part.get("tool"))
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        object.insert(
                            "toolInput".into(),
                            part.get("input")
                                .or_else(|| part.get("arguments"))
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        object.insert(
                            "toolCallId".into(),
                            part.get("id")
                                .or_else(|| part.get("call_id"))
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        output.push(value);
                    }
                    "tool_result" | "tool_output" => {
                        let mut value = message_value(
                            &part_id,
                            "tool",
                            "tool-result",
                            None,
                            model,
                            timestamp.clone(),
                        );
                        let object = value.as_object_mut().unwrap();
                        object.insert(
                            "toolOutput".into(),
                            part.get("content")
                                .or_else(|| part.get("output"))
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        object.insert(
                            "toolCallId".into(),
                            part.get("tool_use_id")
                                .or_else(|| part.get("call_id"))
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        output.push(value);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn message_value(
    id: &str,
    role: &str,
    kind: &str,
    text: Option<&str>,
    model: Option<&str>,
    timestamp: Value,
) -> Value {
    json!({"id":id,"role":role,"kind":kind,"text":text,"model":model,"timestamp":timestamp,"attachments":[]})
}

fn summary(row: &Value, messages: Vec<Value>) -> Value {
    let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
    let cwd = row
        .get("cwd_last")
        .or_else(|| row.get("cwd_at_start"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message_count = row
        .get("message_count")
        .and_then(Value::as_u64)
        .unwrap_or(messages.len() as u64);
    json!({"id":format!("grok-cli:{id}"),"agentId":"grok-cli","nativeId":id,"nativeSessionId":id,"sourceRef":"sessions","title":row.get("title").and_then(Value::as_str).filter(|value|!value.is_empty()).unwrap_or("Untitled Grok session"),"cwd":cwd,"repository":cwd,"model":row.get("model"),"createdAt":row.get("created_at"),"updatedAt":row.get("updated_at"),"messageCount":message_count,"messages":messages,"rawMetadata":{}})
}
