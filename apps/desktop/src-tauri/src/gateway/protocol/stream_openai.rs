use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::{
    sse::{push_done, SseDecoder},
    stream_openai_events::{
        push_chat_chunk, push_response_event, push_responses_completed, push_responses_started,
    },
    GatewayFormat,
};

pub(in crate::gateway) struct OpenAiStreamConverter {
    decoder: SseDecoder,
    state: OpenAiStreamState,
}

enum OpenAiStreamState {
    ChatToResponses(ChatToResponsesState),
    ResponsesToChat(ResponsesToChatState),
}

#[derive(Default)]
struct ChatToResponsesState {
    started: bool,
    completed: bool,
    full_text: String,
    tools: BTreeMap<u64, (String, String, String)>,
    response_id: String,
    model: Value,
    usage: Option<Value>,
    sequence: u64,
}

struct ResponsesToChatState {
    response_id: String,
    model: Value,
    role_emitted: bool,
    tool_indexes: BTreeSet<u64>,
    usage: Option<Value>,
    finished: bool,
}

impl Default for ResponsesToChatState {
    fn default() -> Self {
        Self {
            response_id: "chatcmpl_mcp_link".to_string(),
            model: Value::Null,
            role_emitted: false,
            tool_indexes: BTreeSet::new(),
            usage: None,
            finished: false,
        }
    }
}

impl OpenAiStreamConverter {
    pub(in crate::gateway) fn new(from: GatewayFormat, to: GatewayFormat) -> Result<Self, String> {
        let state = match (from, to) {
            (GatewayFormat::OpenAiCompatible, GatewayFormat::OpenAiResponses) => {
                OpenAiStreamState::ChatToResponses(ChatToResponsesState {
                    response_id: "resp_mcp_link".to_string(),
                    model: Value::Null,
                    ..Default::default()
                })
            }
            (GatewayFormat::OpenAiResponses, GatewayFormat::OpenAiCompatible) => {
                OpenAiStreamState::ResponsesToChat(ResponsesToChatState::default())
            }
            _ => return Err("OpenAI stream converter received an unsupported format pair".into()),
        };
        Ok(Self {
            decoder: SseDecoder::default(),
            state,
        })
    }

    pub(in crate::gateway) fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, String> {
        let events = self.decoder.push(chunk)?;
        self.process_events(events)
    }

    pub(in crate::gateway) fn finish(&mut self) -> Result<Vec<u8>, String> {
        let events = self.decoder.finish()?;
        let mut output = self.process_events(events)?;
        output.extend(match &mut self.state {
            OpenAiStreamState::ChatToResponses(state) => state.finish(),
            OpenAiStreamState::ResponsesToChat(state) => state.finish(),
        });
        Ok(output)
    }

    fn process_events(&mut self, events: Vec<String>) -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        for data in events {
            output.extend(match &mut self.state {
                OpenAiStreamState::ChatToResponses(state) => state.process(&data)?,
                OpenAiStreamState::ResponsesToChat(state) => state.process(&data)?,
            });
        }
        Ok(output)
    }
}

impl ChatToResponsesState {
    fn process(&mut self, data: &str) -> Result<Vec<u8>, String> {
        if self.completed {
            return Ok(Vec::new());
        }
        if data == "[DONE]" {
            return Ok(self.finish());
        }
        let value: Value = serde_json::from_str(data).map_err(|error| error.to_string())?;
        self.response_id = value
            .get("id")
            .and_then(Value::as_str)
            .map(|id| id.replacen("chatcmpl", "resp", 1))
            .unwrap_or_else(|| self.response_id.clone());
        if let Some(model) = value.get("model") {
            self.model = model.clone();
        }
        if let Some(usage) = value.get("usage").filter(|usage| !usage.is_null()) {
            self.usage = Some(usage.clone());
        }
        let mut output = String::new();
        self.ensure_started(&mut output);
        let choice = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .unwrap_or(&Value::Null);
        let delta = choice.get("delta").unwrap_or(&Value::Null);
        // Chat-compatible upstreams often put text in reasoning_content, content arrays,
        // or message — not only delta.content as a plain string. Missing those left
        // Responses clients (Codex) with empty assistant text.
        if let Some(content) = chat_stream_text_delta(delta, choice) {
            if !content.is_empty() {
                self.full_text.push_str(&content);
                push_response_event(
                    &mut output,
                    "response.output_text.delta",
                    json!({ "type": "response.output_text.delta", "item_id": "msg_mcp_link", "output_index": 0, "content_index": 0, "delta": content }),
                    &mut self.sequence,
                );
            }
        }
        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            self.process_tool_call(call, &mut output);
        }
        Ok(output.into_bytes())
    }

    fn process_tool_call(&mut self, call: &Value, output: &mut String) {
        let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
        let function = call.get("function").unwrap_or(&Value::Null);
        let call_id = call
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("call_mcp_link_{index}"));
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("tool")
            .to_string();
        let is_new = !self.tools.contains_key(&index);
        let state = self
            .tools
            .entry(index)
            .or_insert_with(|| (call_id.clone(), name.clone(), String::new()));
        if !call_id.starts_with("call_mcp_link_") {
            state.0.clone_from(&call_id);
        }
        if name != "tool" {
            state.1.clone_from(&name);
        }
        if is_new {
            push_response_event(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added", "output_index": index + 1,
                    "item": { "id": format!("fc_mcp_link_{index}"), "type": "function_call", "status": "in_progress", "call_id": call_id, "name": name, "arguments": "" }
                }),
                &mut self.sequence,
            );
        }
        if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
            state.2.push_str(arguments);
            push_response_event(
                output,
                "response.function_call_arguments.delta",
                json!({ "type": "response.function_call_arguments.delta", "item_id": format!("fc_mcp_link_{index}"), "output_index": index + 1, "delta": arguments }),
                &mut self.sequence,
            );
        }
    }

    fn ensure_started(&mut self, output: &mut String) {
        if !self.started {
            push_responses_started(output, &self.response_id, &self.model, &mut self.sequence);
            self.started = true;
        }
    }

    fn finish(&mut self) -> Vec<u8> {
        if self.completed {
            return Vec::new();
        }
        let mut output = String::new();
        self.ensure_started(&mut output);
        push_responses_completed(
            &mut output,
            &self.response_id,
            &self.model,
            &self.full_text,
            &self.tools,
            self.usage.as_ref(),
            &mut self.sequence,
        );
        self.completed = true;
        output.into_bytes()
    }
}

impl ResponsesToChatState {
    fn process(&mut self, data: &str) -> Result<Vec<u8>, String> {
        if self.finished || data == "[DONE]" {
            return Ok(Vec::new());
        }
        let value: Value = serde_json::from_str(data).map_err(|error| error.to_string())?;
        let mut output = String::new();
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Some(response) = value.get("response") {
            self.response_id = response
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id.replacen("resp", "chatcmpl", 1))
                .unwrap_or_else(|| self.response_id.clone());
            if let Some(model) = response.get("model") {
                self.model = model.clone();
            }
            if let Some(usage) = response.get("usage").filter(|usage| !usage.is_null()) {
                self.usage = Some(usage.clone());
            }
        }
        if !self.role_emitted && matches!(event_type, "response.created" | "response.in_progress") {
            push_chat_chunk(
                &mut output,
                &self.response_id,
                &self.model,
                json!({ "role": "assistant" }),
                Value::Null,
                None,
            );
            self.role_emitted = true;
        }
        match event_type {
            "response.output_text.delta" => push_chat_chunk(
                &mut output,
                &self.response_id,
                &self.model,
                json!({ "content": value.get("delta").cloned().unwrap_or_else(|| json!("")) }),
                Value::Null,
                None,
            ),
            "response.output_item.added" => self.process_item(&value, &mut output),
            "response.function_call_arguments.delta" => {
                let index = output_index(&value);
                self.tool_indexes.insert(index);
                push_chat_chunk(
                    &mut output,
                    &self.response_id,
                    &self.model,
                    json!({ "tool_calls": [{ "index": index, "function": { "arguments": value.get("delta").cloned().unwrap_or_else(|| json!("")) } }] }),
                    Value::Null,
                    None,
                );
            }
            "response.completed" => output.push_str(
                &String::from_utf8(self.finish()).expect("generated SSE is always UTF-8"),
            ),
            "response.failed" | "error" => {
                super::sse::push_data(&mut output, &value);
                push_done(&mut output);
                self.finished = true;
            }
            _ => {}
        }
        Ok(output.into_bytes())
    }

    fn process_item(&mut self, value: &Value, output: &mut String) {
        let item = value.get("item").unwrap_or(&Value::Null);
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            return;
        }
        let index = output_index(value);
        self.tool_indexes.insert(index);
        push_chat_chunk(
            output,
            &self.response_id,
            &self.model,
            json!({ "tool_calls": [{
                "index": index,
                "id": item.get("call_id").or_else(|| item.get("id")).cloned().unwrap_or_else(|| json!(format!("call_mcp_link_{index}"))),
                "type": "function",
                "function": { "name": item.get("name").cloned().unwrap_or_else(|| json!("tool")), "arguments": "" }
            }] }),
            Value::Null,
            None,
        );
    }

    fn finish(&mut self) -> Vec<u8> {
        if self.finished {
            return Vec::new();
        }
        let mut output = String::new();
        push_chat_chunk(
            &mut output,
            &self.response_id,
            &self.model,
            json!({}),
            if self.tool_indexes.is_empty() {
                json!("stop")
            } else {
                json!("tool_calls")
            },
            self.usage.as_ref(),
        );
        push_done(&mut output);
        self.finished = true;
        output.into_bytes()
    }
}

fn output_index(value: &Value) -> u64 {
    value
        .get("output_index")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .saturating_sub(1)
}

fn chat_stream_text_delta(delta: &Value, choice: &Value) -> Option<String> {
    use super::portable_gateway_text;

    // Prefer visible content; fall back to common reasoning-only fields used by
    // DeepSeek/Qwen-style chat proxies so Codex still gets non-empty output.
    for key in ["content", "text", "reasoning_content", "reasoning"] {
        if let Some(value) = delta.get(key) {
            let text = portable_gateway_text(value);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    if let Some(message) = choice.get("message") {
        for key in ["content", "text", "reasoning_content", "reasoning"] {
            if let Some(value) = message.get(key) {
                let text = portable_gateway_text(value);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}
