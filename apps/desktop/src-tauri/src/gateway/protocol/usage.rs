use serde_json::Value;

use super::{sse::SseDecoder, GatewayFormat};
use crate::gateway::logs::GatewayTokenUsage;

pub(in crate::gateway) fn usage_from_json(body: &[u8], format: GatewayFormat) -> GatewayTokenUsage {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return GatewayTokenUsage::default();
    };
    usage_from_value(&value, format)
}

pub(in crate::gateway) struct StreamUsageTracker {
    decoder: SseDecoder,
    format: GatewayFormat,
    usage: GatewayTokenUsage,
    saw_content: bool,
}

impl StreamUsageTracker {
    pub(in crate::gateway) fn new(format: GatewayFormat) -> Self {
        Self {
            decoder: SseDecoder::default(),
            format,
            usage: GatewayTokenUsage::default(),
            saw_content: false,
        }
    }

    pub(in crate::gateway) fn push(&mut self, chunk: &[u8]) {
        let Ok(events) = self.decoder.push(chunk) else {
            return;
        };
        self.process(events);
    }

    pub(in crate::gateway) fn finish(&mut self) {
        if let Ok(events) = self.decoder.finish() {
            self.process(events);
        }
    }

    pub(in crate::gateway) fn usage(&self) -> &GatewayTokenUsage {
        &self.usage
    }

    pub(in crate::gateway) fn saw_content(&self) -> bool {
        self.saw_content
    }

    fn process(&mut self, events: Vec<String>) {
        for data in events {
            if data == "[DONE]" {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            self.usage.merge(usage_from_value(&value, self.format));
            self.saw_content |= contains_stream_content(&value, self.format);
        }
    }
}

impl GatewayTokenUsage {
    fn merge(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.max(other.input_tokens);
        self.output_tokens = self.output_tokens.max(other.output_tokens);
        self.cache_read_tokens = self.cache_read_tokens.max(other.cache_read_tokens);
        self.cache_write_tokens = self.cache_write_tokens.max(other.cache_write_tokens);
        self.total_tokens = self
            .total_tokens
            .max(other.total_tokens)
            .max(self.input_tokens + self.output_tokens);
    }
}

fn usage_from_value(value: &Value, format: GatewayFormat) -> GatewayTokenUsage {
    let usage = match format {
        GatewayFormat::OpenAiCompatible => value.get("usage"),
        GatewayFormat::OpenAiResponses => value
            .get("response")
            .and_then(|response| response.get("usage"))
            .or_else(|| value.get("usage")),
        GatewayFormat::Anthropic => value
            .get("message")
            .and_then(|message| message.get("usage"))
            .or_else(|| value.get("usage")),
    };
    let Some(usage) = usage else {
        return GatewayTokenUsage::default();
    };
    let input_tokens = number(usage, &["input_tokens", "prompt_tokens"]);
    let output_tokens = number(usage, &["output_tokens", "completion_tokens"]);
    let cache_read_tokens =
        number(usage, &["cache_read_input_tokens", "cache_read_tokens"]).max(nested_number(
            usage,
            &["input_tokens_details", "prompt_tokens_details"],
            "cached_tokens",
        ));
    let cache_write_tokens = number(
        usage,
        &["cache_creation_input_tokens", "cache_write_tokens"],
    );
    let explicit_total = number(usage, &["total_tokens"]);
    GatewayTokenUsage {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        total_tokens: if explicit_total > 0 {
            explicit_total
        } else {
            input_tokens + output_tokens + cache_read_tokens + cache_write_tokens
        },
    }
}

fn contains_stream_content(value: &Value, format: GatewayFormat) -> bool {
    match format {
        GatewayFormat::OpenAiCompatible => value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta"))
            .is_some_and(|delta| {
                nonempty_string(delta.get("content"))
                    || delta
                        .get("tool_calls")
                        .and_then(Value::as_array)
                        .is_some_and(|calls| !calls.is_empty())
            }),
        GatewayFormat::OpenAiResponses => match value.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") | Some("response.function_call_arguments.delta") => {
                nonempty_string(value.get("delta"))
            }
            Some("response.output_item.added") => {
                value
                    .get("item")
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    == Some("function_call")
            }
            _ => false,
        },
        GatewayFormat::Anthropic => match value.get("type").and_then(Value::as_str) {
            Some("content_block_delta") => value.get("delta").is_some_and(|delta| {
                nonempty_string(delta.get("text")) || nonempty_string(delta.get("partial_json"))
            }),
            Some("content_block_start") => {
                value
                    .get("content_block")
                    .and_then(|block| block.get("type"))
                    .and_then(Value::as_str)
                    == Some("tool_use")
            }
            _ => false,
        },
    }
}

fn number(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
}

fn nested_number(value: &Value, parents: &[&str], key: &str) -> u64 {
    parents
        .iter()
        .find_map(|parent| {
            value
                .get(*parent)
                .and_then(|value| value.get(key))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0)
}

fn nonempty_string(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_usage_from_all_protocols() {
        let chat = usage_from_value(
            &serde_json::json!({ "usage": {
                "prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12,
                "prompt_tokens_details": { "cached_tokens": 4 }
            } }),
            GatewayFormat::OpenAiCompatible,
        );
        assert_eq!(chat.total_tokens, 12);
        assert_eq!(chat.cache_read_tokens, 4);

        let responses = usage_from_value(
            &serde_json::json!({ "type": "response.completed", "response": { "usage": {
                "input_tokens": 7, "output_tokens": 3,
                "input_tokens_details": { "cached_tokens": 2 }
            } } }),
            GatewayFormat::OpenAiResponses,
        );
        assert_eq!(responses.input_tokens, 7);
        assert_eq!(responses.total_tokens, 12);

        let anthropic = usage_from_value(
            &serde_json::json!({ "usage": {
                "input_tokens": 5, "output_tokens": 1,
                "cache_read_input_tokens": 9, "cache_creation_input_tokens": 4
            } }),
            GatewayFormat::Anthropic,
        );
        assert_eq!(anthropic.total_tokens, 19);
    }

    #[test]
    fn streaming_tracker_handles_split_frames_and_content() {
        let mut tracker = StreamUsageTracker::new(GatewayFormat::OpenAiCompatible);
        let stream = b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n";
        for chunk in stream.chunks(3) {
            tracker.push(chunk);
        }
        tracker.finish();
        assert!(tracker.saw_content());
        assert_eq!(tracker.usage().total_tokens, 4);
    }
}
