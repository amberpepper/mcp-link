use serde_json::{json, Value};

use crate::{
    state::StoreState,
    util::{json::value_id, time::now_millis},
};

pub(crate) fn query_logs(store: &StoreState, options: Option<&Value>) -> Value {
    let server_filter = options
        .and_then(|value| value.get("serverId"))
        .and_then(Value::as_str);
    let limit = options
        .and_then(|value| value.get("limit"))
        .and_then(Value::as_u64)
        .unwrap_or(100) as usize;
    let mut logs = Vec::new();
    for server in &store.servers {
        let server_id = value_id(server).unwrap_or_default();
        if server_filter.is_some_and(|filter| filter != server_id) {
            continue;
        }
        for message in server
            .get("logs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if message.get("requestType").is_some() {
                logs.push(message.clone());
            } else {
                let server_name = server
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("MCP Server");
                logs.push(json!({
                    "id": format!("{server_id}-{}", message.get("timestamp").and_then(Value::as_u64).unwrap_or(0)),
                    "timestamp": message.get("timestamp").and_then(Value::as_u64).unwrap_or_else(|| now_millis() as u64),
                    "clientId": "local",
                    "clientName": "Local",
                    "serverId": server_id,
                    "serverName": server_name,
                    "requestType": "log",
                    "requestParams": {},
                    "responseStatus": "success",
                    "responseData": message,
                    "duration": 0
                }));
            }
        }
    }
    logs.sort_by(|a, b| {
        let ts_a = a.get("timestamp").and_then(Value::as_u64).unwrap_or(0);
        let ts_b = b.get("timestamp").and_then(Value::as_u64).unwrap_or(0);
        ts_b.cmp(&ts_a)
    });
    let total = logs.len();
    logs.truncate(limit);
    json!({ "logs": logs, "total": total, "hasMore": false })
}
