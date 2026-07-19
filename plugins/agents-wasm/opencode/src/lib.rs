use mcp_link_agent_wasm_sdk::{
    export_plugin, set_message_page, AgentPlugin, Host, MessagePageRequest,
};
use serde_json::{json, Value};

mod management;

struct OpenCode;

impl AgentPlugin for OpenCode {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(),
            "loadSession" => load_session(params),
            "loadSessionStats" => load_session_stats(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            "resumeCommand" => resume_command(params),
            "duplicateSession" => duplicate_session(params),
            "exportNative" => export_native(params),
            "renameSession" => rename_session(params),
            "deleteSession" => delete_session(params),
            "loadAttachment" => load_attachment(params),
            _ => Err(format!("Unsupported OpenCode method: {method}")),
        }
    }
}

export_plugin!(OpenCode);

const SESSION_SQL: &str = "SELECT s.id,s.parent_id,s.directory,s.title,s.time_created,s.time_updated,(SELECT COUNT(*) FROM message c WHERE c.session_id=s.id) AS message_count,(SELECT COALESCE(json_extract(m.data,'$.modelID'),json_extract(m.data,'$.model.modelID')) FROM message m WHERE m.session_id=s.id ORDER BY m.time_created DESC LIMIT 1) AS model FROM session s";

fn list_sessions() -> Result<Value, String> {
    let rows = Host::sqlite_query(
        "sessions",
        &format!("{SESSION_SQL} ORDER BY s.time_updated DESC"),
        &[],
    )?;
    Ok(Value::Array(
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|row| summary(row, Vec::new(), row.get("model").and_then(Value::as_str)))
            .collect(),
    ))
}

fn load_session(params: &Value) -> Result<Value, String> {
    let id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or("OpenCode nativeId is required")?;
    let rows = Host::sqlite_query(
        "sessions",
        &format!("{SESSION_SQL} WHERE s.id = ?"),
        &[json!(id)],
    )?;
    let row = rows
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or_else(|| format!("OpenCode session not found: {id}"))?;
    let page_request =
        MessagePageRequest::requested(params).then(|| MessagePageRequest::from_params(params));
    let (messages, parts, page_start) = if let Some(request) = page_request {
        let total = row
            .get("message_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let (start, count) = request.database_bounds(total);
        (
            Host::sqlite_query(
                "sessions",
                "SELECT id,time_created,data FROM message WHERE session_id = ? ORDER BY time_created,id LIMIT ? OFFSET ?",
                &[json!(id), json!(count), json!(start)],
            )?,
            Host::sqlite_query(
                "sessions",
                "SELECT id,message_id,time_created,data FROM part WHERE session_id = ? AND message_id IN (SELECT id FROM message WHERE session_id = ? ORDER BY time_created,id LIMIT ? OFFSET ?) ORDER BY time_created,id",
                &[json!(id), json!(id), json!(count), json!(start)],
            )?,
            Some(start),
        )
    } else {
        (
            Host::sqlite_query(
                "sessions",
                "SELECT id,time_created,data FROM message WHERE session_id = ? ORDER BY time_created,id",
                &[json!(id)],
            )?,
            Host::sqlite_query(
                "sessions",
                "SELECT id,message_id,time_created,data FROM part WHERE session_id = ? ORDER BY time_created,id",
                &[json!(id)],
            )?,
            None,
        )
    };
    let mut output = Vec::new();
    let mut model = row.get("model").and_then(Value::as_str).map(str::to_owned);
    for message in messages.as_array().into_iter().flatten() {
        let data = parse_json(message.get("data"));
        let role = data
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("assistant");
        let message_id = message
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let current_model = data.get("modelID").and_then(Value::as_str).or_else(|| {
            data.get("model")
                .and_then(|v| v.get("modelID"))
                .and_then(Value::as_str)
        });
        if current_model.is_some() {
            model = current_model.map(str::to_owned);
        }
        for part in parts
            .as_array()
            .into_iter()
            .flatten()
            .filter(|part| part.get("message_id").and_then(Value::as_str) == Some(message_id))
        {
            let data = parse_json(part.get("data"));
            let kind = data.get("type").and_then(Value::as_str).unwrap_or_default();
            let timestamp = part.get("time_created").cloned().unwrap_or(Value::Null);
            match kind {
                "text" | "reasoning" => {
                    if let Some(text) = data
                        .get("text")
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                    {
                        output.push(json!({"id":part.get("id"),"role":role,"kind":kind,"text":text,"model":current_model,"timestamp":timestamp,"attachments":[]}));
                    }
                }
                "tool" => {
                    let state = data.get("state").unwrap_or(&Value::Null);
                    output.push(json!({"id":part.get("id"),"role":"assistant","kind":"tool-call","text":null,"toolName":data.get("tool"),"toolInput":state.get("input"),"toolOutput":state.get("output"),"toolCallId":data.get("callID"),"model":current_model,"timestamp":timestamp,"attachments":[]}));
                }
                _ => {}
            }
        }
    }
    let mut session = summary(row, output.clone(), model.as_deref());
    if let Some(start) = page_start {
        set_message_page(&mut session, output, start, start > 0)?;
    }
    Ok(session)
}

fn load_session_stats(params: &Value) -> Result<Value, String> {
    let id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or("OpenCode nativeId is required")?;
    let rows = Host::sqlite_query(
        "sessions",
        "SELECT cost,tokens_input,tokens_output,tokens_reasoning,tokens_cache_read,tokens_cache_write FROM session WHERE id = ? LIMIT 1",
        &[json!(id)],
    )?;
    let Some(row) = rows.as_array().and_then(|rows| rows.first()) else {
        return Ok(Value::Null);
    };
    let input = stat_token(row, "tokens_input");
    let output = stat_token(row, "tokens_output");
    let reasoning = stat_token(row, "tokens_reasoning");
    let cache_read = stat_token(row, "tokens_cache_read");
    let cache_write = stat_token(row, "tokens_cache_write");
    let total = input
        .saturating_add(output)
        .saturating_add(reasoning)
        .saturating_add(cache_read)
        .saturating_add(cache_write);
    Ok(json!({
        "inputTokens": input,
        "outputTokens": output,
        "cachedInputTokens": cache_read,
        "cacheWriteTokens": cache_write,
        "reasoningTokens": reasoning,
        "totalTokens": total,
        "cost": row.get("cost").and_then(Value::as_f64).unwrap_or_default(),
        "source": "reported",
    }))
}

fn stat_token(row: &Value, key: &str) -> u64 {
    row.get(key).and_then(Value::as_u64).unwrap_or_default()
}

fn resume_command(params: &Value) -> Result<Value, String> {
    let id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or("OpenCode nativeId is required")?;
    Ok(json!({"command":"opencode","args":["--session",id]}))
}

fn rename_session(params: &Value) -> Result<Value, String> {
    let id = required_native_id(params)?;
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("OpenCode session title is required")?;
    Host::sqlite_transaction(
        "sessions",
        &[json!({
            "sql": "UPDATE session SET title = ?, time_updated = CAST(strftime('%s','now') AS INTEGER) * 1000 WHERE id = ?",
            "params": [title, id]
        })],
    )?;
    Ok(operation(id, None))
}

fn duplicate_session(params: &Value) -> Result<Value, String> {
    let source_id = required_native_id(params)?;
    let new_id = Host::sqlite_query(
        "sessions",
        "SELECT 'ses_mcl_' || lower(hex(randomblob(16))) AS id",
        &[],
    )?
    .as_array()
    .and_then(|rows| rows.first())
    .and_then(|row| row.get("id"))
    .and_then(Value::as_str)
    .ok_or("OpenCode could not allocate a session ID")?
    .to_string();
    let until = params.get("untilMessage").and_then(Value::as_u64);
    let session_sql = if until.is_some() {
        "INSERT INTO session (id,parent_id,directory,title,time_created,time_updated) SELECT ?,id,directory,title || ' (Copy)',time_created,CAST(strftime('%s','now') AS INTEGER) * 1000 FROM session WHERE id = ?"
    } else {
        "INSERT INTO session (id,parent_id,directory,title,time_created,time_updated) SELECT ?,id,directory,title || ' (Copy)',time_created,CAST(strftime('%s','now') AS INTEGER) * 1000 FROM session WHERE id = ?"
    };
    let mut statements = vec![json!({"sql":session_sql,"params":[new_id,source_id]})];
    let message_filter = until
        .map(|_| " AND id IN (SELECT id FROM message WHERE session_id = ? ORDER BY time_created,id LIMIT ?)")
        .unwrap_or("");
    statements.push(json!({
        "sql": format!("INSERT INTO message (id,session_id,time_created,time_updated,data) SELECT 'msg_mcl_' || id, ?,time_created,time_updated,data FROM message WHERE session_id = ?{}", message_filter),
        "params": if let Some(limit) = until { json!([new_id, source_id, source_id, limit]) } else { json!([new_id, source_id]) }
    }));
    statements.push(json!({
        "sql": "INSERT INTO part (id,message_id,session_id,time_created,time_updated,data) SELECT 'prt_mcl_' || p.id,'msg_mcl_' || p.message_id,?,p.time_created,p.time_updated,p.data FROM part p JOIN message m ON m.id=p.message_id WHERE m.session_id=? AND EXISTS (SELECT 1 FROM message copied WHERE copied.session_id=? AND copied.id='msg_mcl_' || p.message_id)",
        "params": [new_id, source_id, new_id]
    }));
    Host::sqlite_transaction("sessions", &statements)?;
    Ok(operation(&new_id, Some(source_id)))
}

fn export_native(params: &Value) -> Result<Value, String> {
    let id = required_native_id(params)?;
    let session = load_session(&json!({"nativeId": id}))?;
    let content = serde_json::to_string_pretty(&session).map_err(|error| error.to_string())?;
    Ok(json!({
        "fileName": format!("opencode-{id}.json"),
        "encoding": "utf8",
        "content": content
    }))
}

fn delete_session(params: &Value) -> Result<Value, String> {
    let id = required_native_id(params)?;
    Host::sqlite_transaction(
        "sessions",
        &[
            json!({"sql":"DELETE FROM part WHERE session_id = ?","params":[id]}),
            json!({"sql":"DELETE FROM message WHERE session_id = ?","params":[id]}),
            json!({"sql":"DELETE FROM session WHERE id = ?","params":[id]}),
        ],
    )?;
    Ok(operation(id, None))
}

fn load_attachment(params: &Value) -> Result<Value, String> {
    let reference = params
        .get("attachment")
        .and_then(|value| value.get("reference"))
        .and_then(Value::as_str)
        .ok_or("OpenCode attachment reference is required")?;
    let part_id = reference.strip_prefix("part:").unwrap_or(reference);
    let rows = Host::sqlite_query(
        "sessions",
        "SELECT data FROM part WHERE id = ? LIMIT 1",
        &[json!(part_id)],
    )?;
    let data = rows
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("data"))
        .and_then(Value::as_str)
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .ok_or("OpenCode attachment part was not found")?;
    let data_url = data
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| data.get("dataUrl").and_then(Value::as_str))
        .or_else(|| data.get("source").and_then(Value::as_str))
        .filter(|value| {
            value.starts_with("data:")
                || value.starts_with("http://")
                || value.starts_with("https://")
        })
        .ok_or("OpenCode attachment has no supported data URL")?;
    Ok(json!({
        "id": params.get("attachment").and_then(|v| v.get("id")),
        "name": params.get("attachment").and_then(|v| v.get("name")),
        "mimeType": params.get("attachment").and_then(|v| v.get("mimeType")),
        "dataUrl": data_url
    }))
}

fn required_native_id(params: &Value) -> Result<&str, String> {
    params
        .get("nativeId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "OpenCode nativeId is required".to_string())
}

fn operation(native_id: &str, source_native_id: Option<&str>) -> Value {
    json!({
        "ok": true,
        "agentId": "opencode",
        "nativeId": native_id,
        "command": null,
        "sourceNativeId": source_native_id,
        "warnings": [],
        "backupPath": null
    })
}

fn summary(row: &Value, messages: Vec<Value>, model: Option<&str>) -> Value {
    let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
    let cwd = row
        .get("directory")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let count = row
        .get("message_count")
        .and_then(Value::as_u64)
        .unwrap_or(messages.len() as u64);
    json!({"id":format!("opencode:{id}"),"agentId":"opencode","nativeId":id,"nativeSessionId":id,"sourceRef":"sessions","title":row.get("title"),"cwd":cwd,"repository":cwd,"model":model,"createdAt":row.get("time_created"),"updatedAt":row.get("time_updated"),"messageCount":count,"parentNativeId":row.get("parent_id"),"messages":messages,"rawMetadata":{}})
}

fn parse_json(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or(Value::Null)
}
