use mcp_link_agent_wasm_sdk::{
    export_plugin, set_message_page, AgentPlugin, Host, MessagePageRequest,
};
use serde_json::{json, Value};

mod management;

struct Zcode;
impl AgentPlugin for Zcode {
    fn handle(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "listSessions" => list_sessions(),
            "loadSession" => load_session(params),
            "loadSessionStats" => load_session_stats(params),
            "describeManagement" => management::describe(params),
            "loadManagementSection" => management::load_section(params),
            "mutateManagementSection" => management::mutate(params),
            _ => Err(format!("Unsupported ZCode method: {method}")),
        }
    }
}
export_plugin!(Zcode);

const SESSION_SQL: &str = "SELECT s.id,s.parent_id,s.directory,s.path,s.title,s.time_created,s.time_updated,(SELECT COUNT(*) FROM part p WHERE p.session_id=s.id AND json_extract(p.data,'$.type') IN ('text','reasoning','tool')) AS message_count,(SELECT COALESCE(json_extract(m.data,'$.modelID'),json_extract(m.data,'$.model.modelID')) FROM message m WHERE m.session_id=s.id ORDER BY m.time_created DESC LIMIT 1) AS model FROM session s";

fn list_sessions() -> Result<Value, String> {
    let rows = Host::sqlite_query(
        "sessions",
        &format!("{SESSION_SQL} WHERE time_archived IS NULL ORDER BY time_updated DESC"),
        &[],
    )?;
    Ok(Value::Array(
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                summary(
                    row,
                    Vec::new(),
                    row.get("model").and_then(Value::as_str),
                    row.get("message_count")
                        .and_then(Value::as_u64)
                        .unwrap_or_default() as usize,
                )
            })
            .collect(),
    ))
}

fn load_session(params: &Value) -> Result<Value, String> {
    let id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or("ZCode nativeId is required")?;
    let rows = Host::sqlite_query(
        "sessions",
        &format!("{SESSION_SQL} WHERE id = ?"),
        &[json!(id)],
    )?;
    let row = rows
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or_else(|| format!("ZCode session not found: {id}"))?;
    let page_request =
        MessagePageRequest::requested(params).then(|| MessagePageRequest::from_params(params));
    let (messages, parts, page_start) = if let Some(request) = page_request {
        let total = Host::sqlite_query(
            "sessions",
            "SELECT COUNT(*) AS count FROM message WHERE session_id = ?",
            &[json!(id)],
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
            Host::sqlite_query("sessions", "SELECT id,time_created,data FROM message WHERE session_id = ? ORDER BY time_created,id", &[json!(id)])?,
            Host::sqlite_query("sessions", "SELECT id,message_id,time_created,data FROM part WHERE session_id = ? ORDER BY time_created,id", &[json!(id)])?,
            None,
        )
    };
    let mut output = Vec::new();
    let mut model = None;
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
                .and_then(|value| value.get("modelID"))
                .and_then(Value::as_str)
        });
        if current_model.is_some() {
            model = current_model.map(str::to_string);
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
    let count = row
        .get("message_count")
        .and_then(Value::as_u64)
        .unwrap_or(output.len() as u64) as usize;
    let mut session = summary(row, output.clone(), model.as_deref(), count);
    if let Some(start) = page_start {
        set_message_page(&mut session, output, start, start > 0)?;
    }
    Ok(session)
}

#[derive(Default)]
struct StatsTotals {
    input: u64,
    output: u64,
    reasoning: u64,
    cache_read: u64,
    cache_write: u64,
    total: u64,
    cost: f64,
    has_tokens: bool,
    has_cost: bool,
}

fn load_session_stats(params: &Value) -> Result<Value, String> {
    let id = params
        .get("nativeId")
        .and_then(Value::as_str)
        .ok_or("ZCode nativeId is required")?;
    let rows = Host::sqlite_query(
        "sessions",
        "SELECT data FROM message WHERE session_id = ? ORDER BY time_created,id",
        &[json!(id)],
    )?;
    let mut stats = StatsTotals::default();
    for row in rows.as_array().into_iter().flatten() {
        let data = parse_json(row.get("data"));
        add_message_stats(&data, &mut stats);
    }
    if !stats.has_tokens && !stats.has_cost {
        return Ok(Value::Null);
    }
    Ok(json!({
        "inputTokens": stats.has_tokens.then_some(stats.input),
        "outputTokens": stats.has_tokens.then_some(stats.output),
        "cachedInputTokens": stats.has_tokens.then_some(stats.cache_read),
        "cacheWriteTokens": stats.has_tokens.then_some(stats.cache_write),
        "reasoningTokens": stats.has_tokens.then_some(stats.reasoning),
        "totalTokens": stats.has_tokens.then_some(stats.total),
        "cost": stats.has_cost.then_some(stats.cost),
        "source": "reported",
    }))
}

fn add_message_stats(data: &Value, stats: &mut StatsTotals) {
    if let Some(cost) = data.get("cost").and_then(Value::as_f64) {
        stats.cost += cost;
        stats.has_cost = true;
    }
    let Some(tokens) = data.get("tokens").or_else(|| data.get("usage")) else {
        return;
    };
    let input = token_any(tokens, &["input", "input_tokens", "prompt_tokens"]);
    let output = token_any(tokens, &["output", "output_tokens", "completion_tokens"]);
    let reasoning = token_any(tokens, &["reasoning", "reasoning_tokens"]);
    let cache = tokens.get("cache").unwrap_or(&Value::Null);
    let cache_read = token_optional(tokens, &["cache_read", "cached_input_tokens"])
        .unwrap_or_else(|| token_any(cache, &["read"]));
    let cache_write =
        token_optional(tokens, &["cache_write"]).unwrap_or_else(|| token_any(cache, &["write"]));
    let component_total = input
        .saturating_add(output)
        .saturating_add(reasoning)
        .saturating_add(cache_read)
        .saturating_add(cache_write);
    stats.input = stats.input.saturating_add(input);
    stats.output = stats.output.saturating_add(output);
    stats.reasoning = stats.reasoning.saturating_add(reasoning);
    stats.cache_read = stats.cache_read.saturating_add(cache_read);
    stats.cache_write = stats.cache_write.saturating_add(cache_write);
    stats.total = stats.total.saturating_add(
        token_optional(tokens, &["total", "total_tokens"]).unwrap_or(component_total),
    );
    stats.has_tokens = true;
}

fn token_any(value: &Value, keys: &[&str]) -> u64 {
    token_optional(value, keys).unwrap_or_default()
}

fn token_optional(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn summary(row: &Value, messages: Vec<Value>, model: Option<&str>, count: usize) -> Value {
    let id = row.get("id").and_then(Value::as_str).unwrap_or_default();
    let cwd = row
        .get("path")
        .or_else(|| row.get("directory"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({"id":format!("zcode:{id}"),"agentId":"zcode","nativeId":id,"nativeSessionId":id,"sourceRef":"sessions","title":row.get("title"),"cwd":cwd,"repository":cwd,"model":model,"createdAt":row.get("time_created"),"updatedAt":row.get("time_updated"),"messageCount":count,"parentNativeId":row.get("parent_id"),"messages":messages,"rawMetadata":{}})
}

fn parse_json(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[no_mangle]
    extern "C" fn host_call(_: i32, _: i32, _: i32, _: i32) -> i32 {
        -1
    }

    #[test]
    fn aggregates_reported_message_usage_without_double_counting_cache() {
        let mut stats = StatsTotals::default();
        add_message_stats(
            &json!({
                "cost": 0.25,
                "tokens": {
                    "total": 180,
                    "input": 100,
                    "output": 20,
                    "reasoning": 10,
                    "cache_read": 50,
                    "cache": { "read": 50, "write": 0 }
                }
            }),
            &mut stats,
        );
        assert_eq!(stats.input, 100);
        assert_eq!(stats.output, 20);
        assert_eq!(stats.reasoning, 10);
        assert_eq!(stats.cache_read, 50);
        assert_eq!(stats.total, 180);
        assert_eq!(stats.cost, 0.25);
    }
}
