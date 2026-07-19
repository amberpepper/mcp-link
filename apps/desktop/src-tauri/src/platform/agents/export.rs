use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::model::{
    AgentSession, SessionExportOptions, SessionExportResult, SessionMessage, SessionSummary,
};

pub(crate) fn export_session(
    session: &AgentSession,
    options: &SessionExportOptions,
    native: Option<(String, Vec<u8>)>,
) -> Result<SessionExportResult, String> {
    if options.format == "native" {
        let (file_name, bytes) =
            native.ok_or_else(|| "Native export is not supported".to_string())?;
        return Ok(SessionExportResult {
            file_name,
            mime_type: "application/octet-stream".to_string(),
            content: STANDARD.encode(bytes),
            encoding: "base64".to_string(),
        });
    }
    let mut exported_summary = session.summary.clone();
    if options.sanitize {
        exported_summary.title = sanitize(&exported_summary.title);
        exported_summary.cwd = exported_summary.cwd.map(|value| sanitize(&value));
        exported_summary.repository = exported_summary.repository.map(|value| sanitize(&value));
        exported_summary.model = exported_summary.model.map(|value| sanitize(&value));
    }
    let messages = filtered_messages(session, options);
    let safe_name = safe_file_name(&exported_summary.title);
    match options.format.as_str() {
        "html" => Ok(SessionExportResult {
            file_name: format!("{safe_name}.html"),
            mime_type: "text/html;charset=utf-8".to_string(),
            content: render_html(&exported_summary, &messages),
            encoding: "utf8".to_string(),
        }),
        "markdown" | "md" => Ok(SessionExportResult {
            file_name: format!("{safe_name}.md"),
            mime_type: "text/markdown;charset=utf-8".to_string(),
            content: render_markdown(&exported_summary, &messages),
            encoding: "utf8".to_string(),
        }),
        "json" => {
            let source_agent_id = exported_summary.agent_id.clone();
            let source_native_id = exported_summary
                .native_session_id
                .clone()
                .unwrap_or_else(|| exported_summary.native_id.clone());
            Ok(SessionExportResult {
                file_name: format!("{safe_name}.json"),
                mime_type: "application/json;charset=utf-8".to_string(),
                content: serde_json::to_string_pretty(&json!({
                    "schemaVersion": 1,
                    "source": {
                        "agentId": source_agent_id,
                        "nativeSessionId": source_native_id
                    },
                    "metadata": exported_summary,
                    "messages": messages.iter().map(export_message_value).collect::<Vec<_>>()
                }))
                .map_err(|error| error.to_string())?,
                encoding: "utf8".to_string(),
            })
        }
        other => Err(format!("Unsupported export format: {other}")),
    }
}

fn filtered_messages(
    session: &AgentSession,
    options: &SessionExportOptions,
) -> Vec<SessionMessage> {
    let from = options.from_message.unwrap_or(0);
    let to = options
        .to_message
        .unwrap_or(session.messages.len())
        .min(session.messages.len());
    session
        .messages
        .iter()
        .enumerate()
        .filter(|(index, item)| {
            *index >= from
                && *index < to
                && (options.include_reasoning || item.kind != "reasoning")
                && (options.include_tool_calls || item.kind != "tool-call")
                && (options.include_tool_results || item.kind != "tool-result")
        })
        .map(|(_, item)| {
            let mut item = item.clone();
            if options.sanitize {
                item.text = item.text.map(|text| sanitize(&text));
                item.tool_input = item.tool_input.map(sanitize_value);
                item.tool_output = item.tool_output.map(sanitize_value);
            }
            item
        })
        .collect()
}

fn sanitize_value(value: Value) -> Value {
    match value {
        Value::String(value) => Value::String(sanitize(&value)),
        Value::Array(values) => Value::Array(values.into_iter().map(sanitize_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    let redacted = matches!(
                        key.to_ascii_lowercase().as_str(),
                        "apikey" | "api_key" | "token" | "password" | "secret" | "authorization"
                    );
                    (
                        key,
                        if redacted {
                            Value::String("[REDACTED]".to_string())
                        } else {
                            sanitize_value(value)
                        },
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

fn sanitize(value: &str) -> String {
    sanitize_patterns()
        .iter()
        .fold(value.to_string(), |text, regex| {
            regex.replace_all(&text, "${1}[REDACTED]").into_owned()
        })
}

fn sanitize_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?i)(authorization\s*[:=]\s*bearer\s+)[A-Za-z0-9._~+/-]+",
            r#"(?i)((?:api[_-]?key|token|password|secret)\s*[:=]\s*)[^\s,;\"']+"#,
            r"\b(?:sk|mcpr)_[A-Za-z0-9_-]{16,}\b",
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("session export regex must be valid"))
        .collect()
    })
}

fn render_markdown(summary: &SessionSummary, messages: &[SessionMessage]) -> String {
    let mut output = format!(
        "---\ntitle: {}\nagent: {}\nnative_session_id: {}\nworkspace: {}\nmodel: {}\n---\n\n",
        yaml_string(&summary.title),
        yaml_string(&summary.agent_id),
        yaml_string(&summary.native_id),
        yaml_string(summary.cwd.as_deref().unwrap_or("")),
        yaml_string(summary.model.as_deref().unwrap_or("")),
    );
    for item in messages {
        let heading = match item.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "tool" => "Tool",
            _ => "System",
        };
        output.push_str(&format!("## {heading}\n\n"));
        match item.kind.as_str() {
            "tool-call" => output.push_str(&format!(
                "<details>\n<summary>Tool call: {}</summary>\n\n```json\n{}\n```\n\n</details>\n\n",
                item.tool_name.as_deref().unwrap_or("tool"),
                pretty_value(item.tool_input.as_ref())
            )),
            "tool-result" => output.push_str(&format!(
                "<details>\n<summary>Tool result: {}</summary>\n\n```text\n{}\n```\n\n</details>\n\n",
                item.tool_name.as_deref().unwrap_or("tool"),
                value_text(item.tool_output.as_ref())
            )),
            "reasoning" => output.push_str(&format!(
                "<details>\n<summary>Reasoning</summary>\n\n{}\n\n</details>\n\n",
                item.text.as_deref().unwrap_or("")
            )),
            _ => output.push_str(item.text.as_deref().unwrap_or("")),
        }
        for attachment in &item.attachments {
            let Some(data_url) = attachment.data_url.as_deref() else {
                continue;
            };
            let name = attachment.name.as_deref().unwrap_or("attachment");
            if attachment.kind == "image"
                || attachment
                    .mime_type
                    .as_deref()
                    .is_some_and(|mime| mime.starts_with("image/"))
            {
                output.push_str(&format!("\n\n![{}]({})", markdown_alt(name), data_url));
            } else {
                output.push_str(&format!("\n\n[{}]({})", markdown_alt(name), data_url));
            }
        }
        output.push_str("\n\n");
    }
    output
}

fn render_html(summary: &SessionSummary, messages: &[SessionMessage]) -> String {
    let cards = messages
        .iter()
        .map(|item| {
            let mut body = match item.kind.as_str() {
                "tool-call" => format!(
                    "<details><summary>{}</summary><pre>{}</pre></details>",
                    escape_html(item.tool_name.as_deref().unwrap_or("Tool call")),
                    escape_html(&pretty_value(item.tool_input.as_ref()))
                ),
                "tool-result" => format!(
                    "<details><summary>{}</summary><pre>{}</pre></details>",
                    escape_html(item.tool_name.as_deref().unwrap_or("Tool result")),
                    escape_html(&value_text(item.tool_output.as_ref()))
                ),
                "reasoning" => format!(
                    "<details><summary>Reasoning</summary><div class=\"text\">{}</div></details>",
                    escape_html(item.text.as_deref().unwrap_or(""))
                ),
                _ => format!(
                    "<div class=\"text\">{}</div>",
                    escape_html(item.text.as_deref().unwrap_or(""))
                ),
            };
            for attachment in &item.attachments {
                let Some(data_url) = attachment.data_url.as_deref() else {
                    continue;
                };
                let name = attachment.name.as_deref().unwrap_or("attachment");
                if attachment.kind == "image"
                    || attachment
                        .mime_type
                        .as_deref()
                        .is_some_and(|mime| mime.starts_with("image/"))
                {
                    body.push_str(&format!(
                        "<a class=\"attachment\" href=\"{}\" target=\"_blank\"><img src=\"{}\" alt=\"{}\"></a>",
                        escape_html(data_url),
                        escape_html(data_url),
                        escape_html(name)
                    ));
                } else {
                    body.push_str(&format!(
                        "<a class=\"file\" href=\"{}\" download=\"{}\">{}</a>",
                        escape_html(data_url),
                        escape_html(name),
                        escape_html(name)
                    ));
                }
            }
            format!(
                "<article class=\"message {} {}\"><header>{}</header>{}</article>",
                escape_html(&item.role),
                escape_html(&item.kind),
                escape_html(&format!("{} · {}", item.role, item.kind)),
                body
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{title}</title><style>
        :root{{color-scheme:light dark;--bg:#0b1020;--card:#141b2d;--muted:#92a0b8;--border:#26324a;--user:#1e3a5f;--assistant:#17233b}}
        *{{box-sizing:border-box}}body{{margin:0;background:var(--bg);color:#e8edf7;font:14px/1.6 system-ui,sans-serif}}main{{max-width:980px;margin:auto;padding:32px 18px}}h1{{margin:0 0 6px}}.meta{{color:var(--muted);margin-bottom:22px}}input{{width:100%;padding:10px 12px;margin-bottom:18px;border:1px solid var(--border);border-radius:8px;background:var(--card);color:inherit}}.message{{padding:14px 16px;margin:10px 0;border:1px solid var(--border);border-radius:10px;background:var(--card)}}.message.user{{background:var(--user)}}.message.assistant{{background:var(--assistant)}}header{{font-size:12px;text-transform:uppercase;color:var(--muted);margin-bottom:8px}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;background:#080c16;padding:12px;border-radius:8px}}.text{{white-space:pre-wrap;overflow-wrap:anywhere}}details summary{{cursor:pointer}}.attachment{{display:block;margin-top:10px}}.attachment img{{display:block;max-width:100%;max-height:680px;border-radius:8px;object-fit:contain}}.file{{display:inline-block;margin-top:10px;color:#9ec5ff}}
        </style></head><body><main><h1>{title}</h1><div class=\"meta\">{agent} · {workspace} · {count} messages</div><input id=\"q\" placeholder=\"Search messages\"><section id=\"messages\">{cards}</section></main><script>const q=document.getElementById('q');q.addEventListener('input',()=>{{const s=q.value.toLowerCase();document.querySelectorAll('.message').forEach(x=>x.hidden=!x.textContent.toLowerCase().includes(s));}});</script></body></html>",
        title = escape_html(&summary.title),
        agent = escape_html(&summary.agent_id),
        workspace = escape_html(summary.cwd.as_deref().unwrap_or("")),
        count = messages.len(),
        cards = cards
    )
}

fn export_message_value(item: &SessionMessage) -> Value {
    let mut value = serde_json::to_value(item).unwrap_or(Value::Null);
    let attachments = item
        .attachments
        .iter()
        .map(|attachment| {
            json!({
                "id": attachment.id,
                "kind": attachment.kind,
                "name": attachment.name,
                "mimeType": attachment.mime_type,
                "size": attachment.size,
                "dataUrl": attachment.data_url,
            })
        })
        .collect::<Vec<_>>();
    if let Some(object) = value.as_object_mut() {
        object.insert("attachments".to_string(), Value::Array(attachments));
    }
    value
}

fn markdown_alt(value: &str) -> String {
    value.replace('[', "\\[").replace(']', "\\]")
}

fn pretty_value(value: Option<&Value>) -> String {
    value
        .and_then(|value| serde_json::to_string_pretty(value).ok())
        .unwrap_or_default()
}

fn value_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(value) => serde_json::to_string_pretty(value).unwrap_or_default(),
        None => String::new(),
    }
}

fn yaml_string(value: &str) -> String {
    format!("{:?}", value)
}

fn safe_file_name(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let value = value.trim();
    if value.is_empty() {
        "session".to_string()
    } else {
        value.chars().take(80).collect()
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_redacts_common_secrets() {
        assert_eq!(
            sanitize("Authorization: Bearer abcdef123456"),
            "Authorization: Bearer [REDACTED]"
        );
        assert_eq!(sanitize("api_key=secret-value"), "api_key=[REDACTED]");
    }
}
