use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::{
    sse::{push_data, push_done, push_event, SseDecoder},
    stream_openai_events::push_chat_chunk,
    GatewayFormat,
};

pub(super) struct AnthropicStreamConverter {
    decoder: SseDecoder,
    state: AnthropicStreamState,
}

enum AnthropicStreamState {
    ChatToAnthropic(ChatToAnthropicState),
    AnthropicToChat(AnthropicToChatState),
}

#[derive(Default)]
struct ChatToAnthropicState {
    started: bool,
    text_started: bool,
    open_tools: BTreeSet<u64>,
    stopped: bool,
    message_id: String,
    model: Value,
    usage: Option<Value>,
    stop_reason: Option<String>,
}

struct AnthropicToChatState {
    response_id: String,
    model: Value,
    role_emitted: bool,
    tool_indexes: BTreeMap<u64, u64>,
    stop_reason: Option<Value>,
    usage: Option<Value>,
    finished: bool,
}

impl Default for AnthropicToChatState {
    fn default() -> Self {
        Self {
            response_id: "chatcmpl_mcp_link".to_string(),
            model: Value::Null,
            role_emitted: false,
            tool_indexes: BTreeMap::new(),
            stop_reason: None,
            usage: None,
            finished: false,
        }
    }
}

impl AnthropicStreamConverter {
    pub(super) fn new(from: GatewayFormat, to: GatewayFormat) -> Result<Self, String> {
        let state = match (from, to) {
            (GatewayFormat::OpenAiCompatible, GatewayFormat::Anthropic) => {
                AnthropicStreamState::ChatToAnthropic(ChatToAnthropicState {
                    message_id: "msg_mcp_link".to_string(),
                    model: Value::Null,
                    ..Default::default()
                })
            }
            (GatewayFormat::Anthropic, GatewayFormat::OpenAiCompatible) => {
                AnthropicStreamState::AnthropicToChat(AnthropicToChatState::default())
            }
            _ => return Err("Anthropic stream converter received an unsupported pair".into()),
        };
        Ok(Self {
            decoder: SseDecoder::default(),
            state,
        })
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, String> {
        let events = self.decoder.push(chunk)?;
        self.process_events(events)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<u8>, String> {
        let events = self.decoder.finish()?;
        let mut output = self.process_events(events)?;
        output.extend(match &mut self.state {
            AnthropicStreamState::ChatToAnthropic(state) => state.finish(),
            AnthropicStreamState::AnthropicToChat(state) => state.finish(),
        });
        Ok(output)
    }

    fn process_events(&mut self, events: Vec<String>) -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        for data in events {
            output.extend(match &mut self.state {
                AnthropicStreamState::ChatToAnthropic(state) => state.process(&data)?,
                AnthropicStreamState::AnthropicToChat(state) => state.process(&data)?,
            });
        }
        Ok(output)
    }
}

impl ChatToAnthropicState {
    fn process(&mut self, data: &str) -> Result<Vec<u8>, String> {
        if self.stopped {
            return Ok(Vec::new());
        }
        if data == "[DONE]" {
            return Ok(self.finish());
        }
        let value: Value = serde_json::from_str(data).map_err(|error| error.to_string())?;
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            self.message_id = id.replacen("chatcmpl", "msg", 1);
        }
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
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            self.ensure_text_started(&mut output);
            push_event(
                &mut output,
                "content_block_delta",
                &json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": content } }),
            );
        }
        for call in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            self.process_tool_call(call, &mut output);
        }
        if let Some(reason) = choice
            .get("finish_reason")
            .filter(|reason| !reason.is_null())
        {
            self.stop_reason = Some(
                match reason.as_str() {
                    Some("tool_calls") => "tool_use",
                    Some("length") => "max_tokens",
                    _ => "end_turn",
                }
                .to_string(),
            );
        }
        Ok(output.into_bytes())
    }

    fn process_tool_call(&mut self, call: &Value, output: &mut String) {
        let chat_index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
        let block_index = chat_index + 1;
        let function = call.get("function").unwrap_or(&Value::Null);
        if self.open_tools.insert(block_index) {
            push_event(
                output,
                "content_block_start",
                &json!({
                    "type": "content_block_start",
                    "index": block_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": call.get("id").cloned().unwrap_or_else(|| json!(format!("call_mcp_link_{chat_index}"))),
                        "name": function.get("name").cloned().unwrap_or_else(|| json!("tool")),
                        "input": {}
                    }
                }),
            );
        }
        if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
            push_event(
                output,
                "content_block_delta",
                &json!({
                    "type": "content_block_delta", "index": block_index,
                    "delta": { "type": "input_json_delta", "partial_json": arguments }
                }),
            );
        }
    }

    fn ensure_started(&mut self, output: &mut String) {
        if self.started {
            return;
        }
        push_event(
            output,
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id, "type": "message", "role": "assistant", "content": [],
                    "model": self.model, "stop_reason": null, "stop_sequence": null,
                    "usage": anthropic_usage(self.usage.as_ref())
                }
            }),
        );
        self.started = true;
    }

    fn ensure_text_started(&mut self, output: &mut String) {
        if self.text_started {
            return;
        }
        push_event(
            output,
            "content_block_start",
            &json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }),
        );
        self.text_started = true;
    }

    fn complete(&mut self, output: &mut String, stop_reason: &str) {
        if self.stopped {
            return;
        }
        self.ensure_started(output);
        if self.text_started {
            push_event(
                output,
                "content_block_stop",
                &json!({ "type": "content_block_stop", "index": 0 }),
            );
        }
        for block_index in &self.open_tools {
            push_event(
                output,
                "content_block_stop",
                &json!({ "type": "content_block_stop", "index": block_index }),
            );
        }
        push_event(
            output,
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": { "stop_reason": stop_reason, "stop_sequence": null },
                "usage": anthropic_usage(self.usage.as_ref())
            }),
        );
        push_event(output, "message_stop", &json!({ "type": "message_stop" }));
        self.stopped = true;
    }

    fn finish(&mut self) -> Vec<u8> {
        if self.stopped {
            return Vec::new();
        }
        let mut output = String::new();
        let reason = self.stop_reason.clone().unwrap_or_else(|| {
            if self.open_tools.is_empty() {
                "end_turn".to_string()
            } else {
                "tool_use".to_string()
            }
        });
        self.complete(&mut output, &reason);
        output.into_bytes()
    }
}

impl AnthropicToChatState {
    fn process(&mut self, data: &str) -> Result<Vec<u8>, String> {
        if self.finished || data == "[DONE]" {
            return Ok(Vec::new());
        }
        let value: Value = serde_json::from_str(data).map_err(|error| error.to_string())?;
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut output = String::new();
        match event_type {
            "message_start" => self.start_message(&value, &mut output),
            "content_block_start" => self.start_content_block(&value, &mut output),
            "content_block_delta" => self.push_content_delta(&value, &mut output),
            "message_delta" => self.capture_completion(&value),
            "message_stop" => output.push_str(
                &String::from_utf8(self.finish()).expect("generated SSE is always UTF-8"),
            ),
            "error" => {
                push_data(&mut output, &value);
                push_done(&mut output);
                self.finished = true;
            }
            _ => {}
        }
        Ok(output.into_bytes())
    }

    fn start_message(&mut self, value: &Value, output: &mut String) {
        let message = value.get("message").unwrap_or(&Value::Null);
        self.response_id = message
            .get("id")
            .and_then(Value::as_str)
            .map(|id| id.replacen("msg", "chatcmpl", 1))
            .unwrap_or_else(|| self.response_id.clone());
        if let Some(model) = message.get("model") {
            self.model = model.clone();
        }
        if let Some(usage) = message.get("usage") {
            self.usage = Some(usage.clone());
        }
        self.ensure_role(output);
    }

    fn start_content_block(&mut self, value: &Value, output: &mut String) {
        let block = value.get("content_block").unwrap_or(&Value::Null);
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            return;
        }
        self.ensure_role(output);
        let block_index = value.get("index").and_then(Value::as_u64).unwrap_or(1);
        let tool_index = self.tool_indexes.len() as u64;
        self.tool_indexes.insert(block_index, tool_index);
        push_chat_chunk(
            output,
            &self.response_id,
            &self.model,
            json!({ "tool_calls": [{
                "index": tool_index,
                "id": block.get("id").cloned().unwrap_or_else(|| json!(format!("call_mcp_link_{tool_index}"))),
                "type": "function",
                "function": { "name": block.get("name").cloned().unwrap_or_else(|| json!("tool")), "arguments": "" }
            }] }),
            Value::Null,
            None,
        );
    }

    fn push_content_delta(&mut self, value: &Value, output: &mut String) {
        self.ensure_role(output);
        let delta = value.get("delta").unwrap_or(&Value::Null);
        if delta.get("type").and_then(Value::as_str) == Some("input_json_delta") {
            let block_index = value.get("index").and_then(Value::as_u64).unwrap_or(1);
            let tool_index = self
                .tool_indexes
                .get(&block_index)
                .copied()
                .unwrap_or_else(|| block_index.saturating_sub(1));
            push_chat_chunk(
                output,
                &self.response_id,
                &self.model,
                json!({ "tool_calls": [{ "index": tool_index, "function": { "arguments": delta.get("partial_json").cloned().unwrap_or_else(|| json!("")) } }] }),
                Value::Null,
                None,
            );
        } else if let Some(text) = delta.get("text").and_then(Value::as_str) {
            push_chat_chunk(
                output,
                &self.response_id,
                &self.model,
                json!({ "content": text }),
                Value::Null,
                None,
            );
        }
    }

    fn capture_completion(&mut self, value: &Value) {
        if let Some(usage) = value.get("usage") {
            self.usage = Some(usage.clone());
        }
        self.stop_reason = value
            .get("delta")
            .and_then(|delta| delta.get("stop_reason"))
            .and_then(Value::as_str)
            .map(|reason| match reason {
                "tool_use" => json!("tool_calls"),
                "max_tokens" => json!("length"),
                _ => json!("stop"),
            });
    }

    fn ensure_role(&mut self, output: &mut String) {
        if self.role_emitted {
            return;
        }
        push_chat_chunk(
            output,
            &self.response_id,
            &self.model,
            json!({ "role": "assistant" }),
            Value::Null,
            None,
        );
        self.role_emitted = true;
    }

    fn finish(&mut self) -> Vec<u8> {
        if self.finished {
            return Vec::new();
        }
        let mut output = String::new();
        self.ensure_role(&mut output);
        let reason = self.stop_reason.clone().unwrap_or_else(|| {
            if self.tool_indexes.is_empty() {
                json!("stop")
            } else {
                json!("tool_calls")
            }
        });
        push_chat_chunk(
            &mut output,
            &self.response_id,
            &self.model,
            json!({}),
            reason,
            self.usage.as_ref(),
        );
        push_done(&mut output);
        self.finished = true;
        output.into_bytes()
    }
}

fn anthropic_usage(usage: Option<&Value>) -> Value {
    let input = usage
        .and_then(|usage| {
            usage
                .get("input_tokens")
                .or_else(|| usage.get("prompt_tokens"))
        })
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .and_then(|usage| {
            usage
                .get("output_tokens")
                .or_else(|| usage.get("completion_tokens"))
        })
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({ "input_tokens": input, "output_tokens": output })
}
