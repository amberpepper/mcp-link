use serde_json::{json, Value};

pub const DEFAULT_MESSAGE_PAGE_SIZE: usize = 50;
pub const MAX_MESSAGE_PAGE_SIZE: usize = 200;
pub const DEFAULT_SESSION_PAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct MessagePageRequest {
    pub limit: usize,
    pub before: Option<u64>,
}

impl MessagePageRequest {
    pub fn from_params(params: &Value) -> Self {
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_MESSAGE_PAGE_SIZE as u64)
            .clamp(1, MAX_MESSAGE_PAGE_SIZE as u64) as usize;
        let before = params.get("before").and_then(Value::as_u64);
        Self { limit, before }
    }

    pub fn requested(params: &Value) -> bool {
        params.get("limit").and_then(Value::as_u64).is_some()
    }

    pub fn database_bounds(self, total_rows: u64) -> (u64, u64) {
        let end = self.before.unwrap_or(total_rows).min(total_rows);
        let start = end.saturating_sub(self.limit as u64);
        (start, end - start)
    }
}

#[derive(Debug)]
pub struct FileWindow {
    pub content: String,
    pub start: u64,
    pub end: u64,
    pub file_size: u64,
}

impl FileWindow {
    fn from_host_value(value: Value) -> Result<Self, String> {
        let content = value
            .get("content")
            .and_then(Value::as_str)
            .ok_or("Host file.read returned no content")?
            .to_string();
        let start = value
            .get("start")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let end = value.get("end").and_then(Value::as_u64).unwrap_or(start);
        let file_size = value.get("fileSize").and_then(Value::as_u64).unwrap_or(end);
        Ok(Self {
            content,
            start,
            end,
            file_size,
        })
    }
}

pub fn jsonl_records(content: &str, base_offset: u64) -> Vec<(u64, Value)> {
    let mut local_offset = 0_u64;
    content
        .split_inclusive('\n')
        .filter_map(|segment| {
            let offset = base_offset.saturating_add(local_offset);
            local_offset = local_offset.saturating_add(segment.len() as u64);
            let line = segment.trim_end_matches(['\r', '\n']);
            serde_json::from_str::<Value>(line)
                .ok()
                .map(|value| (offset, value))
        })
        .collect()
}

pub fn scan_jsonl_reverse<F>(
    resource_id: &str,
    path: &str,
    window_bytes: usize,
    mut visit: F,
) -> Result<(), String>
where
    F: FnMut(&Value) -> bool,
{
    let mut before = None;
    loop {
        let window = Host::file_read_window(resource_id, path, before, window_bytes)?;
        for (_, value) in jsonl_records(&window.content, window.start)
            .into_iter()
            .rev()
        {
            if !visit(&value) {
                return Ok(());
            }
        }
        if window.start == 0 {
            return Ok(());
        }
        if before == Some(window.start) {
            return Err("Host file.read did not advance while scanning JSONL".to_string());
        }
        before = Some(window.start);
    }
}

pub fn paginate_sourced_messages(
    mut messages: Vec<(u64, Value)>,
    limit: usize,
    fallback_cursor: u64,
) -> (Vec<Value>, u64, bool) {
    if messages.len() > limit {
        let mut split = messages.len() - limit;
        let boundary = messages[split].0;
        while split > 0 && messages[split - 1].0 == boundary {
            split -= 1;
        }
        messages.drain(..split);
    }
    let cursor = messages
        .first()
        .map(|(cursor, _)| *cursor)
        .unwrap_or(fallback_cursor);
    (
        messages.into_iter().map(|(_, message)| message).collect(),
        cursor,
        cursor > 0,
    )
}

pub fn set_message_page(
    session: &mut Value,
    messages: Vec<Value>,
    cursor: u64,
    has_more: bool,
) -> Result<(), String> {
    let object = session
        .as_object_mut()
        .ok_or("Agent session page is invalid")?;
    object.insert("messages".to_string(), Value::Array(messages));
    object.insert("messageCursor".to_string(), json!(cursor));
    object.insert("hasMoreMessages".to_string(), json!(has_more));
    Ok(())
}

pub fn read_json_config(resource_id: &str, path: &str) -> Result<(Value, String), String> {
    let document = Host::config_read(resource_id, path)?;
    let value = if document.content.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&document.content)
            .map_err(|error| format!("Invalid JSON configuration: {error}"))?
    };
    Ok((value, document.revision))
}

pub fn read_json_document(
    resource_id: &str,
    path: &str,
    label: &str,
) -> Result<(ConfigDocument, Value), String> {
    let document = Host::config_read(resource_id, path)?;
    let value = if document.content.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&document.content)
            .map_err(|error| format!("Invalid {label}: {error}"))?
    };
    Ok((document, value))
}

pub fn finish_json_management_mutation(
    resource_id: &str,
    path: &str,
    section: &str,
    changed_resource: &str,
    value: &Value,
    document: ConfigDocument,
    dry_run: bool,
    restart_required: bool,
) -> Result<Value, String> {
    let mut content = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    content.push('\n');
    let changed = content != document.content;
    let revision = if dry_run {
        document.revision
    } else {
        Host::config_write_atomic(resource_id, path, &content, &document.revision)?
            .get("revision")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or("Host file.writeAtomic returned no revision")?
    };
    Ok(json!({
        "section": section,
        "revision": revision,
        "changed": changed,
        "changedResources": [changed_resource],
        "restartRequired": restart_required,
        "warnings": [],
    }))
}

pub fn write_json_config(
    resource_id: &str,
    path: &str,
    value: &Value,
    expected_revision: &str,
) -> Result<String, String> {
    let mut content = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    content.push('\n');
    Host::config_write_atomic(resource_id, path, &content, expected_revision)?
        .get("revision")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Host file.writeAtomic returned no revision".to_string())
}

pub fn masked_secret(value: Option<&str>) -> Value {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.starts_with('$') => json!({
            "configured": true,
            "source": "environment",
            "masked": "••••••••",
            "environmentVariable": value.trim_start_matches('$'),
        }),
        Some(_) => json!({
            "configured": true,
            "source": "inline",
            "masked": "••••••••",
        }),
        None => json!({ "configured": false }),
    }
}

pub fn management_section(id: &str, revision: &str, data: Value) -> Value {
    json!({
        "id": id,
        "revision": revision,
        "data": data,
        "warnings": [],
    })
}

pub fn management_section_descriptor(
    id: &str,
    renderer: &str,
    source: &str,
    read_only: bool,
) -> Value {
    json!({
        "id": id,
        "renderer": renderer,
        "source": source,
        "readOnly": read_only,
        "features": [],
    })
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "mcp_link")]
extern "C" {
    #[link_name = "host_call"]
    fn mcp_link_host_call(
        request_ptr: i32,
        request_len: i32,
        output_ptr: i32,
        output_capacity: i32,
    ) -> i32;
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn mcp_link_host_call(
    _request_ptr: i32,
    _request_len: i32,
    _output_ptr: i32,
    _output_capacity: i32,
) -> i32 {
    -1
}

pub trait AgentPlugin {
    fn handle(method: &str, params: &Value) -> Result<Value, String>;
}

pub struct Host;

#[derive(Debug, Clone)]
pub struct ConfigDocument {
    pub content: String,
    pub revision: String,
}

impl Host {
    pub fn call(method: &str, params: Value) -> Result<Value, String> {
        let request = serde_json::to_vec(&json!({ "method": method, "params": params }))
            .map_err(|error| error.to_string())?;
        let request_len =
            i32::try_from(request.len()).map_err(|_| "Host request is too large".to_string())?;
        let required = unsafe { mcp_link_host_call(request.as_ptr() as i32, request_len, 0, 0) };
        if required < 0 {
            return Err("Host call failed".to_string());
        }
        let mut response = vec![0_u8; required as usize];
        let written = unsafe {
            mcp_link_host_call(
                request.as_ptr() as i32,
                request_len,
                response.as_mut_ptr() as i32,
                required,
            )
        };
        if written < 0 || written as usize > response.len() {
            return Err("Host response is invalid".to_string());
        }
        let response: Value = serde_json::from_slice(&response[..written as usize])
            .map_err(|error| error.to_string())?;
        if let Some(error) = response.get("error") {
            return Err(error.as_str().unwrap_or("Host call failed").to_string());
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn file_read(resource_id: &str, path: &str) -> Result<String, String> {
        Self::call(
            "file.read",
            json!({ "resourceId": resource_id, "path": path }),
        )?
        .get("content")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Host file.read returned no content".to_string())
    }

    pub fn config_read(resource_id: &str, path: &str) -> Result<ConfigDocument, String> {
        let value = Self::call(
            "file.read",
            json!({
                "resourceId": resource_id,
                "path": path,
                "includeRevision": true,
            }),
        )?;
        Ok(ConfigDocument {
            content: value
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            revision: value
                .get("revision")
                .and_then(Value::as_str)
                .unwrap_or("missing")
                .to_string(),
        })
    }

    pub fn config_write_atomic(
        resource_id: &str,
        path: &str,
        content: &str,
        expected_revision: &str,
    ) -> Result<Value, String> {
        Self::call(
            "file.writeAtomic",
            json!({
                "resourceId": resource_id,
                "path": path,
                "content": content,
                "expectedRevision": expected_revision,
            }),
        )
    }

    pub fn file_read_head(
        resource_id: &str,
        path: &str,
        max_bytes: usize,
    ) -> Result<String, String> {
        Self::call(
            "file.read",
            json!({ "resourceId": resource_id, "path": path, "maxBytes": max_bytes }),
        )?
        .get("content")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Host file.read returned no content".to_string())
    }

    pub fn file_read_before(
        resource_id: &str,
        path: &str,
        before: Option<u64>,
        max_bytes: usize,
    ) -> Result<Value, String> {
        Self::call(
            "file.read",
            json!({
                "resourceId": resource_id,
                "path": path,
                "before": before,
                "maxBytes": max_bytes,
            }),
        )
    }

    pub fn file_read_window(
        resource_id: &str,
        path: &str,
        before: Option<u64>,
        max_bytes: usize,
    ) -> Result<FileWindow, String> {
        FileWindow::from_host_value(Self::file_read_before(
            resource_id,
            path,
            before,
            max_bytes,
        )?)
    }

    pub fn file_read_range(
        resource_id: &str,
        path: &str,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Value, String> {
        Self::call(
            "file.read",
            json!({
                "resourceId": resource_id,
                "path": path,
                "offset": offset,
                "maxBytes": max_bytes,
            }),
        )
    }

    pub fn file_write(resource_id: &str, path: &str, content: &str) -> Result<Value, String> {
        Self::call(
            "file.write",
            json!({ "resourceId": resource_id, "path": path, "content": content }),
        )
    }

    pub fn file_list(resource_id: &str, path: &str) -> Result<Value, String> {
        Self::call(
            "file.list",
            json!({ "resourceId": resource_id, "path": path }),
        )
    }

    pub fn file_remove(resource_id: &str, path: &str) -> Result<Value, String> {
        Self::call(
            "file.remove",
            json!({ "resourceId": resource_id, "path": path }),
        )
    }

    pub fn sqlite_query(database_id: &str, sql: &str, params: &[Value]) -> Result<Value, String> {
        Self::call(
            "sqlite.query",
            json!({ "databaseId": database_id, "sql": sql, "params": params }),
        )
    }

    pub fn sqlite_transaction(database_id: &str, statements: &[Value]) -> Result<Value, String> {
        Self::call(
            "sqlite.transaction",
            json!({ "databaseId": database_id, "statements": statements }),
        )
    }
}

pub fn handle_request<P: AgentPlugin>(request: &[u8]) -> Vec<u8> {
    let response = match serde_json::from_slice::<Value>(request) {
        Ok(request) => {
            let method = request
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = request.get("params").unwrap_or(&Value::Null);
            match P::handle(method, params) {
                Ok(result) => json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "result": result,
                }),
                Err(error) => json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "error": { "message": error },
                }),
            }
        }
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": Value::Null,
            "error": { "message": error.to_string() },
        }),
    };
    serde_json::to_vec(&response)
        .unwrap_or_else(|_| b"{\"error\":{\"message\":\"serialization failed\"}}".to_vec())
}

#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        #[no_mangle]
        pub extern "C" fn mcp_link_alloc(length: i32) -> i32 {
            if length <= 0 {
                return 0;
            }
            let mut buffer = Vec::<u8>::with_capacity(length as usize);
            let pointer = buffer.as_mut_ptr();
            std::mem::forget(buffer);
            pointer as i32
        }

        #[no_mangle]
        pub unsafe extern "C" fn mcp_link_dealloc(pointer: i32, length: i32) {
            let _ = (pointer, length);
        }

        #[no_mangle]
        pub unsafe extern "C" fn mcp_link_call(pointer: i32, length: i32) -> i64 {
            if pointer == 0 || length <= 0 {
                return 0;
            }
            let request = std::slice::from_raw_parts(pointer as *const u8, length as usize);
            let response = $crate::handle_request::<$plugin>(request);
            let response_pointer = response.as_ptr() as u32;
            let response_length = response.len() as u32;
            std::mem::forget(response);
            ((response_pointer as u64) << 32 | response_length as u64) as i64
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_pages_move_strictly_toward_the_start() {
        let first = MessagePageRequest {
            limit: 50,
            before: None,
        };
        assert_eq!(first.database_bounds(123), (73, 50));

        let second = MessagePageRequest {
            limit: 50,
            before: Some(73),
        };
        assert_eq!(second.database_bounds(123), (23, 50));

        let final_page = MessagePageRequest {
            limit: 50,
            before: Some(23),
        };
        assert_eq!(final_page.database_bounds(123), (0, 23));
    }

    #[test]
    fn file_pages_keep_messages_from_one_source_record_together() {
        let messages = vec![
            (10, json!({"id": "a"})),
            (10, json!({"id": "b"})),
            (20, json!({"id": "c"})),
        ];
        let (page, cursor, has_more) = paginate_sourced_messages(messages, 2, 0);
        assert_eq!(page.len(), 3);
        assert_eq!(cursor, 10);
        assert!(has_more);
    }

    #[test]
    fn empty_file_page_still_advances_its_cursor() {
        let (page, cursor, has_more) = paginate_sourced_messages(Vec::new(), 50, 4096);
        assert!(page.is_empty());
        assert_eq!(cursor, 4096);
        assert!(has_more);
    }
}
