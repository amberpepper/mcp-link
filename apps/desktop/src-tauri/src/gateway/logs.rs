use std::{path::PathBuf, time::Instant};

use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{state::DesktopState, util::time::now_millis};

const RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
const MAX_LOG_ROWS: i64 = 5_000;
const MAX_QUERY_ROWS: u64 = 500;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayTokenUsage {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cache_read_tokens: u64,
    pub(crate) cache_write_tokens: u64,
    pub(crate) total_tokens: u64,
}

pub(crate) struct GatewayCallStart<'a> {
    pub(crate) client_protocol: &'a str,
    pub(crate) upstream_protocol: &'a str,
    pub(crate) requested_model: &'a str,
    pub(crate) upstream_model: &'a str,
    pub(crate) provider_id: &'a str,
    pub(crate) provider_name: &'a str,
    pub(crate) streaming: bool,
}

pub(crate) struct GatewayCallCompletion<'a> {
    pub(crate) status: &'a str,
    pub(crate) http_status: Option<u16>,
    pub(crate) first_token_ms: Option<u64>,
    pub(crate) usage: &'a GatewayTokenUsage,
    pub(crate) error: Option<&'a str>,
}

pub(crate) struct GatewayCallGuard {
    database: PathBuf,
    id: String,
    request_id: String,
    started: Instant,
    completed: bool,
    recorded: bool,
}

impl GatewayCallGuard {
    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }

    pub(crate) fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub(crate) fn finish(&mut self, completion: GatewayCallCompletion<'_>) {
        if self.completed {
            return;
        }
        self.completed = true;
        if !self.recorded {
            return;
        }
        if let Err(error) = update_call(
            &self.database,
            &self.id,
            self.started.elapsed().as_millis() as u64,
            completion,
        ) {
            eprintln!("Failed to finish gateway call log: {error}");
        }
    }
}

impl Drop for GatewayCallGuard {
    fn drop(&mut self) {
        if self.completed || !self.recorded {
            return;
        }
        let usage = GatewayTokenUsage::default();
        self.finish(GatewayCallCompletion {
            status: "cancelled",
            http_status: None,
            first_token_ms: None,
            usage: &usage,
            error: Some("Client disconnected before the response completed"),
        });
    }
}

pub(crate) fn start_call(state: &DesktopState, input: GatewayCallStart<'_>) -> GatewayCallGuard {
    let id = Uuid::new_v4().to_string();
    let request_id = format!("gw_{}", Uuid::new_v4().simple());
    let database = state.store_path.clone();
    let result = insert_call(&database, &id, &request_id, input);
    if let Err(error) = result.as_ref() {
        eprintln!("Failed to create gateway call log: {error}");
    }
    GatewayCallGuard {
        database,
        id,
        request_id,
        started: Instant::now(),
        completed: false,
        recorded: result.is_ok(),
    }
}

pub(crate) fn list_call_logs(state: &DesktopState, input: Option<&Value>) -> Result<Value, String> {
    let input = input.and_then(Value::as_object);
    let limit = input
        .and_then(|input| input.get("limit"))
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .clamp(1, MAX_QUERY_ROWS);
    let mut conditions = Vec::new();
    let mut values = Vec::<SqlValue>::new();
    if let Some(before) = input
        .and_then(|input| input.get("before"))
        .and_then(Value::as_i64)
    {
        conditions.push("started_at_ms < ?".to_string());
        values.push(before.into());
    }
    if let Some(status) = optional_query_string(input, "status") {
        conditions.push("status = ?".to_string());
        values.push(status.into());
    }
    if let Some(provider_id) = optional_query_string(input, "providerId") {
        conditions.push("provider_id = ?".to_string());
        values.push(provider_id.into());
    }
    if let Some(search) = optional_query_string(input, "search") {
        conditions.push("(request_id LIKE ? OR requested_model LIKE ? OR upstream_model LIKE ? OR provider_name LIKE ?)".to_string());
        let search = format!("%{search}%");
        values.extend((0..4).map(|_| SqlValue::Text(search.clone())));
    }
    values.push((limit as i64).into());
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    let sql = format!(
        "SELECT id,request_id,started_at_ms,finished_at_ms,status,http_status,streaming,client_protocol,upstream_protocol,requested_model,upstream_model,provider_id,provider_name,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,total_tokens,first_token_ms,duration_ms,error FROM gateway_call_logs{where_clause} ORDER BY started_at_ms DESC LIMIT ?"
    );
    let connection = Connection::open(&state.store_path).map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let logs = statement
        .query_map(params_from_iter(values), |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "requestId": row.get::<_, String>(1)?,
                "startedAt": row.get::<_, i64>(2)?,
                "finishedAt": row.get::<_, Option<i64>>(3)?,
                "status": row.get::<_, String>(4)?,
                "httpStatus": row.get::<_, Option<u16>>(5)?,
                "streaming": row.get::<_, bool>(6)?,
                "clientProtocol": row.get::<_, String>(7)?,
                "upstreamProtocol": row.get::<_, String>(8)?,
                "requestedModel": row.get::<_, String>(9)?,
                "upstreamModel": row.get::<_, String>(10)?,
                "providerId": row.get::<_, String>(11)?,
                "providerName": row.get::<_, String>(12)?,
                "inputTokens": row.get::<_, u64>(13)?,
                "outputTokens": row.get::<_, u64>(14)?,
                "cacheReadTokens": row.get::<_, u64>(15)?,
                "cacheWriteTokens": row.get::<_, u64>(16)?,
                "totalTokens": row.get::<_, u64>(17)?,
                "firstTokenMs": row.get::<_, Option<u64>>(18)?,
                "durationMs": row.get::<_, Option<u64>>(19)?,
                "error": row.get::<_, Option<String>>(20)?,
            }))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(Value::Array(logs))
}

pub(crate) fn clear_call_logs(state: &DesktopState) -> Result<Value, String> {
    let connection = Connection::open(&state.store_path).map_err(|error| error.to_string())?;
    let count = connection
        .execute("DELETE FROM gateway_call_logs", [])
        .map_err(|error| error.to_string())?;
    Ok(json!(count))
}

fn insert_call(
    database: &PathBuf,
    id: &str,
    request_id: &str,
    input: GatewayCallStart<'_>,
) -> Result<(), String> {
    let connection = Connection::open(database).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO gateway_call_logs (id,request_id,started_at_ms,status,streaming,client_protocol,upstream_protocol,requested_model,upstream_model,provider_id,provider_name) VALUES (?1,?2,?3,'running',?4,?5,?6,?7,?8,?9,?10)",
            params![
                id,
                request_id,
                now_millis(),
                input.streaming,
                input.client_protocol,
                input.upstream_protocol,
                input.requested_model,
                input.upstream_model,
                input.provider_id,
                input.provider_name,
            ],
        )
        .map_err(|error| error.to_string())?;
    prune_logs(&connection)
}

fn update_call(
    database: &PathBuf,
    id: &str,
    duration_ms: u64,
    completion: GatewayCallCompletion<'_>,
) -> Result<(), String> {
    let connection = Connection::open(database).map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE gateway_call_logs SET finished_at_ms=?2,status=?3,http_status=?4,input_tokens=?5,output_tokens=?6,cache_read_tokens=?7,cache_write_tokens=?8,total_tokens=?9,first_token_ms=?10,duration_ms=?11,error=?12 WHERE id=?1",
            params![
                id,
                now_millis(),
                completion.status,
                completion.http_status,
                completion.usage.input_tokens,
                completion.usage.output_tokens,
                completion.usage.cache_read_tokens,
                completion.usage.cache_write_tokens,
                completion.usage.total_tokens,
                completion.first_token_ms,
                duration_ms,
                completion.error.map(|error| error.chars().take(1_000).collect::<String>()),
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn prune_logs(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM gateway_call_logs WHERE started_at_ms < ?1",
            [now_millis() - RETENTION_MS],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM gateway_call_logs WHERE id IN (SELECT id FROM gateway_call_logs ORDER BY started_at_ms DESC LIMIT -1 OFFSET ?1)",
            [MAX_LOG_ROWS],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn optional_query_string(
    input: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<String> {
    input
        .and_then(|input| input.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_queries_and_clears_gateway_logs_without_payloads() {
        let root = std::env::temp_dir().join(format!("gateway-log-{}", Uuid::new_v4()));
        let state = DesktopState::load(root.join("mcp.db"));
        let mut call = start_call(
            &state,
            GatewayCallStart {
                client_protocol: "openai-compatible",
                upstream_protocol: "openai-responses",
                requested_model: "alias",
                upstream_model: "upstream",
                provider_id: "provider",
                provider_name: "Provider",
                streaming: true,
            },
        );
        call.finish(GatewayCallCompletion {
            status: "succeeded",
            http_status: Some(200),
            first_token_ms: Some(12),
            usage: &GatewayTokenUsage {
                input_tokens: 4,
                output_tokens: 2,
                total_tokens: 6,
                ..Default::default()
            },
            error: None,
        });
        let logs = list_call_logs(&state, Some(&json!({ "status": "succeeded" }))).unwrap();
        assert_eq!(logs[0]["requestedModel"], "alias");
        assert_eq!(logs[0]["inputTokens"], 4);
        assert!(logs[0].get("requestBody").is_none());
        assert_eq!(clear_call_logs(&state).unwrap(), 1);
        assert_eq!(list_call_logs(&state, None).unwrap(), json!([]));
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }
}
