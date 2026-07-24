use std::{convert::Infallible, fs, sync::Arc, time::Duration};

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::header,
    response::Response,
    routing::post,
    Json, Router,
};
use futures::StreamExt;
use serde_json::{json, Value};

use super::*;
use crate::{
    gateway::{logs::list_call_logs, server::build_model_gateway},
    state::DesktopState,
};

#[derive(Clone, Default)]
struct MockUpstream {
    requests: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
}

async fn mock_responses_upstream(
    State(state): State<MockUpstream>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    state
        .requests
        .lock()
        .unwrap()
        .push(("responses".to_string(), payload));
    Json(json!({
        "id": "resp_upstream", "object": "response", "status": "completed", "model": "upstream",
        "output": [{ "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "responses-ok" }] }],
        "output_text": "responses-ok",
        "usage": { "input_tokens": 2, "output_tokens": 1, "total_tokens": 3 }
    }))
}

async fn mock_chat_upstream(
    State(state): State<MockUpstream>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    state
        .requests
        .lock()
        .unwrap()
        .push(("chat".to_string(), payload));
    Json(json!({
        "id": "chatcmpl_upstream", "object": "chat.completion", "model": "upstream",
        "choices": [{ "index": 0, "finish_reason": "stop", "message": { "role": "assistant", "content": "chat-ok" } }],
        "usage": { "prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3 }
    }))
}

async fn mock_anthropic_stream_upstream(Json(_payload): Json<Value>) -> Response {
    let stream = futures::stream::unfold(0_u8, |step| async move {
        let chunk = match step {
            0 => concat!(
                "event: message_start\n",
                "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_live\",\"model\":\"upstream\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n"
            ),
            1 => {
                tokio::time::sleep(Duration::from_millis(600)).await;
                concat!(
                    "event: content_block_start\n",
                    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                    "event: content_block_delta\n",
                    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"live-ok\"}}\n\n",
                    "event: message_delta\n",
                    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
                    "event: message_stop\n",
                    "data: {\"type\":\"message_stop\"}\n\n"
                )
            }
            _ => return None,
        };
        Some((
            Ok::<Bytes, Infallible>(Bytes::from_static(chunk.as_bytes())),
            step + 1,
        ))
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(stream))
        .unwrap()
}

#[test]
fn chat_and_responses_requests_preserve_tools_images_and_results() {
    let chat = json!({
        "model": "alias",
        "messages": [
            { "role": "system", "content": "Be precise" },
            { "role": "user", "content": [
                { "type": "text", "text": "inspect" },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,AA==" } }
            ]},
            { "role": "assistant", "content": null, "tool_calls": [{
                "id": "call_1", "type": "function",
                "function": { "name": "read", "arguments": "{\"path\":\"a\"}" }
            }]},
            { "role": "tool", "tool_call_id": "call_1", "content": "ok" }
        ],
        "tools": [{ "type": "function", "function": {
            "name": "read", "description": "read file",
            "parameters": { "type": "object" }, "strict": true
        }}],
        "tool_choice": { "type": "function", "function": { "name": "read" } },
        "max_completion_tokens": 123,
        "reasoning_effort": "high",
        "stream": true
    });
    let responses = openai_chat_to_responses(chat).unwrap();
    assert_eq!(responses["instructions"], "Be precise");
    assert_eq!(responses["max_output_tokens"], 123);
    assert_eq!(responses["reasoning"]["effort"], "high");
    assert!(responses["input"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["type"] == "function_call_output"));
    assert_eq!(responses["tools"][0]["name"], "read");
    assert_eq!(responses["tool_choice"]["name"], "read");

    let round_trip = openai_responses_to_chat(responses).unwrap();
    assert_eq!(round_trip["messages"][0]["role"], "system");
    assert!(round_trip["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| message["role"] == "tool"));
    assert_eq!(round_trip["tools"][0]["function"]["name"], "read");
    assert_eq!(round_trip["max_completion_tokens"], 123);
}

#[test]
fn chat_and_responses_results_preserve_tool_calls_and_usage() {
    let chat = json!({
        "id": "chatcmpl_1", "object": "chat.completion", "created": 7, "model": "m",
        "choices": [{ "index": 0, "finish_reason": "tool_calls", "message": {
            "role": "assistant", "content": "checking",
            "tool_calls": [{ "id": "call_1", "type": "function", "function": {
                "name": "read", "arguments": "{\"path\":\"a\"}"
            }}]
        }}],
        "usage": { "prompt_tokens": 10, "completion_tokens": 4, "total_tokens": 14 }
    });
    let responses = openai_chat_response_to_responses(&chat);
    assert_eq!(responses["object"], "response");
    assert!(responses["output"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["type"] == "function_call"));
    assert_eq!(responses["usage"]["total_tokens"], 14);

    let round_trip = openai_responses_response_to_chat(&responses);
    assert_eq!(round_trip["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(
        round_trip["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
        "read"
    );
    assert_eq!(round_trip["usage"]["total_tokens"], 14);
}

#[test]
fn responses_only_features_are_preserved_or_rejected_explicitly() {
    let payload = json!({
        "model": "m",
        "input": "search",
        "tools": [{ "type": "web_search_preview" }]
    });
    assert_eq!(
        convert_request_payload(
            payload.clone(),
            GatewayFormat::OpenAiResponses,
            GatewayFormat::OpenAiResponses
        )
        .unwrap(),
        payload
    );
    assert!(openai_responses_to_chat(payload).is_err());

    let stateful = json!({
        "model": "m",
        "input": "continue",
        "previous_response_id": "resp_previous"
    });
    assert!(openai_responses_to_chat(stateful).is_err());
}

#[test]
fn upstream_errors_are_returned_in_the_client_format() {
    let anthropic =
        br#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#;
    let openai = convert_error_body(
        anthropic,
        GatewayFormat::Anthropic,
        GatewayFormat::OpenAiResponses,
    )
    .unwrap();
    let openai: Value = serde_json::from_slice(&openai).unwrap();
    assert_eq!(openai["error"]["type"], "rate_limit_error");
    assert_eq!(openai["error"]["message"], "slow down");

    let compatible = br#"{"error":{"type":"invalid_request_error","message":"bad input"}}"#;
    let anthropic = convert_error_body(
        compatible,
        GatewayFormat::OpenAiCompatible,
        GatewayFormat::Anthropic,
    )
    .unwrap();
    let anthropic: Value = serde_json::from_slice(&anthropic).unwrap();
    assert_eq!(anthropic["type"], "error");
    assert_eq!(anthropic["error"]["message"], "bad input");
}

#[test]
fn all_request_and_response_format_pairs_are_convertible() {
    let chat_request = json!({ "model": "m", "messages": [{ "role": "user", "content": "hi" }] });
    let responses_request = openai_chat_to_responses(chat_request.clone()).unwrap();
    let anthropic_request = openai_to_anthropic_messages(chat_request.clone()).unwrap();
    let requests = [
        (GatewayFormat::OpenAiCompatible, chat_request),
        (GatewayFormat::OpenAiResponses, responses_request),
        (GatewayFormat::Anthropic, anthropic_request),
    ];
    for (from, payload) in &requests {
        for to in [
            GatewayFormat::OpenAiCompatible,
            GatewayFormat::OpenAiResponses,
            GatewayFormat::Anthropic,
        ] {
            assert!(convert_request_payload(payload.clone(), *from, to).is_ok());
        }
    }

    let chat_response = json!({
        "id": "chatcmpl_1", "model": "m",
        "choices": [{ "message": { "role": "assistant", "content": "ok" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    });
    let responses_response = openai_chat_response_to_responses(&chat_response);
    let anthropic_response = openai_response_to_anthropic(&chat_response);
    let responses = [
        (GatewayFormat::OpenAiCompatible, chat_response),
        (GatewayFormat::OpenAiResponses, responses_response),
        (GatewayFormat::Anthropic, anthropic_response),
    ];
    for (from, payload) in &responses {
        for to in [
            GatewayFormat::OpenAiCompatible,
            GatewayFormat::OpenAiResponses,
            GatewayFormat::Anthropic,
        ] {
            assert!(convert_response_payload(payload.clone(), *from, to).is_ok());
        }
    }
}

#[test]
fn chat_and_responses_streams_convert_text_tools_and_completion() {
    let chat = concat!(
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
            "data: [DONE]\n\n"
        );
    let responses = convert_stream_body(
        chat.as_bytes(),
        GatewayFormat::OpenAiCompatible,
        GatewayFormat::OpenAiResponses,
    )
    .unwrap();
    let responses_text = String::from_utf8(responses.clone()).unwrap();
    assert!(responses_text.contains("response.output_text.delta"));
    assert!(responses_text.contains("response.function_call_arguments.delta"));
    assert!(responses_text.contains("response.completed"));
    assert!(responses_text.contains("\"input_tokens\":3"));
    assert!(responses_text.contains("\"sequence_number\":"));

    let anthropic = convert_stream_body(
        chat.as_bytes(),
        GatewayFormat::OpenAiCompatible,
        GatewayFormat::Anthropic,
    )
    .unwrap();
    let anthropic = String::from_utf8(anthropic).unwrap();
    assert!(anthropic.contains("\"input_tokens\":3"));
    assert!(anthropic.contains("message_stop"));

    let round_trip = convert_stream_body(
        &responses,
        GatewayFormat::OpenAiResponses,
        GatewayFormat::OpenAiCompatible,
    )
    .unwrap();
    let round_trip = String::from_utf8(round_trip).unwrap();
    assert!(round_trip.contains("hello"));
    assert!(round_trip.contains("tool_calls"));
    assert!(round_trip.contains("data: [DONE]"));
}

#[test]
fn incremental_openai_stream_converter_handles_arbitrary_network_chunks() {
    let chat = concat!(
        "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"first\"},\"finish_reason\":null}]}\r\n\r\n",
        "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"second\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let mut converter = GatewayStreamConverter::new(
        GatewayFormat::OpenAiCompatible,
        GatewayFormat::OpenAiResponses,
    )
    .unwrap();
    let mut output = Vec::new();
    let mut emitted_before_finish = false;
    for byte in chat.as_bytes().chunks(1) {
        let converted = converter.push(byte).unwrap();
        emitted_before_finish |= String::from_utf8_lossy(&converted).contains("first");
        output.extend(converted);
    }
    assert!(emitted_before_finish);
    output.extend(converter.finish().unwrap());
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("first"));
    assert!(output.contains("second"));
    assert!(output.contains("response.completed"));

    let mut reverse = GatewayStreamConverter::new(
        GatewayFormat::OpenAiResponses,
        GatewayFormat::OpenAiCompatible,
    )
    .unwrap();
    let mut chat_output = Vec::new();
    for chunk in output.as_bytes().chunks(3) {
        chat_output.extend(reverse.push(chunk).unwrap());
    }
    chat_output.extend(reverse.finish().unwrap());
    let chat_output = String::from_utf8(chat_output).unwrap();
    assert!(chat_output.contains("first"));
    assert!(chat_output.contains("second"));
    assert!(chat_output.contains("data: [DONE]"));
}

#[test]
fn all_stream_format_pairs_convert_without_dropping_completion() {
    let chat = concat!(
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"m\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        )
        .as_bytes()
        .to_vec();
    let responses = convert_stream_body(
        &chat,
        GatewayFormat::OpenAiCompatible,
        GatewayFormat::OpenAiResponses,
    )
    .unwrap();
    let anthropic = convert_stream_body(
        &chat,
        GatewayFormat::OpenAiCompatible,
        GatewayFormat::Anthropic,
    )
    .unwrap();
    let streams = [
        (GatewayFormat::OpenAiCompatible, chat),
        (GatewayFormat::OpenAiResponses, responses),
        (GatewayFormat::Anthropic, anthropic),
    ];
    for (from, stream) in &streams {
        for to in [
            GatewayFormat::OpenAiCompatible,
            GatewayFormat::OpenAiResponses,
            GatewayFormat::Anthropic,
        ] {
            let converted = convert_stream_body(stream, *from, to).unwrap();
            let converted = String::from_utf8(converted).unwrap();
            assert!(
                converted.contains("hello"),
                "missing text for {from:?} -> {to:?}: {converted}"
            );
            match to {
                GatewayFormat::OpenAiCompatible => {
                    assert!(converted.contains("data: [DONE]"));
                }
                GatewayFormat::OpenAiResponses => {
                    assert!(converted.contains("response.completed"));
                }
                GatewayFormat::Anthropic => {
                    assert!(converted.contains("message_stop"));
                }
            }
        }
    }
}

#[test]
fn anthropic_streaming_tool_calls_survive_both_openai_formats() {
    let anthropic = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"read\",\"input\":{}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\\\"a\\\"}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
    );
    let chat = convert_stream_body(
        anthropic.as_bytes(),
        GatewayFormat::Anthropic,
        GatewayFormat::OpenAiCompatible,
    )
    .unwrap();
    let chat_text = String::from_utf8(chat).unwrap();
    assert!(chat_text.contains("tool_calls"));
    assert!(chat_text.contains("read"));
    assert!(chat_text.contains("path"));
    assert!(chat_text.contains("finish_reason\":\"tool_calls"));

    let responses = convert_stream_body(
        anthropic.as_bytes(),
        GatewayFormat::Anthropic,
        GatewayFormat::OpenAiResponses,
    )
    .unwrap();
    let responses = String::from_utf8(responses).unwrap();
    assert!(responses.contains("response.output_item.added"));
    assert!(responses.contains("response.function_call_arguments.delta"));
    assert!(responses.contains("response.function_call_arguments.done"));
    assert!(responses.contains("response.completed"));
}

#[tokio::test]
async fn http_gateway_converts_between_compatible_and_responses_upstreams() {
    let upstream_state = MockUpstream::default();
    let upstream_router = Router::new()
        .route("/v1/responses", post(mock_responses_upstream))
        .route("/v1/chat/completions", post(mock_chat_upstream))
        .with_state(upstream_state.clone());
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_address = upstream_listener.local_addr().unwrap();
    let upstream_task = tokio::spawn(async move {
        axum::serve(upstream_listener, upstream_router)
            .await
            .unwrap();
    });

    let root = std::env::temp_dir().join(format!(
        "mcp-link-gateway-protocol-test-{}",
        uuid::Uuid::new_v4()
    ));
    let state = Arc::new(DesktopState::load(root.join("mcp.db")));
    let key = state
        .store
        .lock()
        .unwrap()
        .settings
        .get("modelGatewayAccessKey")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    {
        let mut store = state.store.lock().unwrap();
        store.gateway_providers = vec![json!({
            "id": "provider", "name": "Mock", "protocol": "openai-responses",
            "baseUrl": format!("http://{upstream_address}/v1"), "apiKey": "", "models": ["m"],
            "enabled": true, "createdAt": "a", "updatedAt": "b"
        })];
        store.settings.insert(
            "modelGatewayActiveProviderId".to_string(),
            json!("provider"),
        );
    }
    let gateway_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_address = gateway_listener.local_addr().unwrap();
    let gateway = build_model_gateway(state.clone()).with_state(state.clone());
    let gateway_task = tokio::spawn(async move {
        axum::serve(gateway_listener, gateway).await.unwrap();
    });
    let client = reqwest::Client::new();
    let chat_response: Value = client
        .post(format!(
            "http://{gateway_address}/openai/v1/chat/completions"
        ))
        .bearer_auth(&key)
        .json(&json!({ "model": "m", "messages": [{ "role": "user", "content": "hello" }] }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(chat_response["object"], "chat.completion");
    assert_eq!(
        chat_response["choices"][0]["message"]["content"],
        "responses-ok"
    );
    assert_eq!(upstream_state.requests.lock().unwrap()[0].0, "responses");
    assert!(upstream_state.requests.lock().unwrap()[0].1["input"].is_array());

    state.store.lock().unwrap().gateway_providers[0]["protocol"] = json!("openai-compatible");
    let responses_response: Value = client
        .post(format!("http://{gateway_address}/openai/v1/responses"))
        .bearer_auth(&key)
        .json(&json!({ "model": "m", "input": "hello" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(responses_response["object"], "response");
    assert_eq!(responses_response["output_text"], "chat-ok");
    assert_eq!(upstream_state.requests.lock().unwrap()[1].0, "chat");
    assert!(upstream_state.requests.lock().unwrap()[1].1["messages"].is_array());

    let logs = list_call_logs(&state, None).unwrap();
    assert_eq!(logs["items"].as_array().unwrap().len(), 2);
    assert_eq!(logs["items"][0]["status"], "succeeded");
    assert_eq!(logs["items"][0]["clientProtocol"], "openai-responses");
    assert_eq!(logs["items"][0]["upstreamProtocol"], "openai-compatible");
    assert_eq!(logs["items"][0]["totalTokens"], 3);
    assert!(logs["items"][0]["requestId"].as_str().unwrap().starts_with("gw_"));

    gateway_task.abort();
    upstream_task.abort();
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn http_gateway_streams_chained_anthropic_conversion_before_upstream_finishes() {
    let upstream_router = Router::new().route("/v1/messages", post(mock_anthropic_stream_upstream));
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_address = upstream_listener.local_addr().unwrap();
    let upstream_task = tokio::spawn(async move {
        axum::serve(upstream_listener, upstream_router)
            .await
            .unwrap();
    });

    let root = std::env::temp_dir().join(format!(
        "mcp-link-gateway-live-stream-test-{}",
        uuid::Uuid::new_v4()
    ));
    let state = Arc::new(DesktopState::load(root.join("mcp.db")));
    let key = state
        .store
        .lock()
        .unwrap()
        .settings
        .get("modelGatewayAccessKey")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    {
        let mut store = state.store.lock().unwrap();
        store.gateway_providers = vec![json!({
            "id": "provider", "name": "Mock Anthropic", "protocol": "anthropic",
            "baseUrl": format!("http://{upstream_address}/v1"), "apiKey": "", "models": ["m"],
            "enabled": true, "createdAt": "a", "updatedAt": "b"
        })];
        store.settings.insert(
            "modelGatewayActiveProviderId".to_string(),
            json!("provider"),
        );
    }
    let gateway_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_address = gateway_listener.local_addr().unwrap();
    let gateway = build_model_gateway(state.clone()).with_state(state.clone());
    let gateway_task = tokio::spawn(async move {
        axum::serve(gateway_listener, gateway).await.unwrap();
    });

    let response = reqwest::Client::new()
        .post(format!("http://{gateway_address}/openai/v1/responses"))
        .bearer_auth(key)
        .json(&json!({ "model": "m", "input": "hello", "stream": true }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert!(response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("gw_")));
    let mut stream = response.bytes_stream();
    let first = tokio::time::timeout(Duration::from_millis(250), stream.next())
        .await
        .expect("the converted first event must not wait for upstream completion")
        .expect("stream must contain a first event")
        .unwrap();
    let first = String::from_utf8(first.to_vec()).unwrap();
    assert!(first.contains("response.created"), "{first}");

    let mut remainder = Vec::new();
    while let Some(chunk) = stream.next().await {
        remainder.extend(chunk.unwrap());
    }
    let remainder = String::from_utf8(remainder).unwrap();
    assert!(remainder.contains("live-ok"), "{remainder}");
    assert!(remainder.contains("response.completed"), "{remainder}");

    let logs = list_call_logs(&state, None).unwrap();
    assert_eq!(logs["items"][0]["status"], "succeeded");
    assert_eq!(logs["items"][0]["inputTokens"], 3);
    assert_eq!(logs["items"][0]["outputTokens"], 2);
    assert!(logs["items"][0]["firstTokenMs"].as_u64().unwrap() >= 500);

    gateway_task.abort();
    upstream_task.abort();
    let _ = fs::remove_dir_all(root);
}
