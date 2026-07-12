use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use rmcp::{
    model::ClientInfo,
    service::ServiceExt,
    transport::{
        streamable_http_client::{
            StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
        },
        TokioChildProcess,
    },
};
use serde_json::Value;
use std::{future::Future, path::PathBuf, process::Stdio, time::Duration};

use crate::{
    mcp::transport::legacy_sse::LegacySseClientTransport,
    state::DesktopRuntime,
    util::json::{string_array, string_map},
};

const DEFAULT_MCP_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(45);

pub(crate) async fn connect_runtime(server: &Value) -> Result<DesktopRuntime, String> {
    match server
        .get("serverType")
        .and_then(Value::as_str)
        .unwrap_or("local")
    {
        "local" => connect_stdio_runtime(server).await,
        "remote-streamable" => connect_streamable_http_runtime(server).await,
        "remote" => connect_legacy_sse_runtime(server).await,
        other => Err(format!("Unsupported MCP server type: {other}")),
    }
}

async fn connect_stdio_runtime(server: &Value) -> Result<DesktopRuntime, String> {
    let command_name = server
        .get("command")
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .ok_or_else(|| "Command is required for local MCP servers".to_string())?;
    let args = string_array(server.get("args"))
        .into_iter()
        .map(|value| resolve_config_template(server, &value))
        .collect::<Result<Vec<_>, _>>()?;
    let env = string_map(server.get("env"))
        .into_iter()
        .map(|(key, value)| resolve_config_template(server, &value).map(|value| (key, value)))
        .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
    let resolved_command = resolve_stdio_command(command_name);
    let mut command = std::process::Command::new(&resolved_command);
    command.args(args);
    command.envs(env);
    command.stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let command = tokio::process::Command::from(command);
    let (transport, _stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to start MCP command '{command_name}': {error}"))?;
    let pid = transport.id();
    let client = with_initialize_timeout(
        ClientInfo::default().serve(transport),
        initialize_timeout(server),
    )
    .await?;
    Ok(DesktopRuntime { client, pid })
}

fn resolve_config_template(server: &Value, value: &str) -> Result<String, String> {
    let mut resolved = value.to_string();
    while let Some(start) = resolved.find("${user_config.") {
        let key_start = start + "${user_config.".len();
        let Some(relative_end) = resolved[key_start..].find('}') else {
            return Err(format!("Invalid user_config template: {value}"));
        };
        let key_end = key_start + relative_end;
        let key = &resolved[key_start..key_end];
        let replacement = server
            .get("env")
            .and_then(Value::as_object)
            .and_then(|env| env.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && !value.contains("${user_config."))
            .map(str::to_string)
            .or_else(|| {
                server
                    .get("inputParams")
                    .and_then(Value::as_object)
                    .and_then(|params| params.get(key))
                    .and_then(|param| param.get("default"))
                    .and_then(config_value_to_string)
                    .filter(|value| !value.trim().is_empty())
            })
            .ok_or_else(|| format!("Required DXT setting is missing: {key}"))?;
        resolved.replace_range(start..=key_end, &replacement);
    }
    Ok(resolved)
}

fn config_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(values) => Some(
            values
                .iter()
                .filter_map(config_value_to_string)
                .collect::<Vec<_>>()
                .join(";"),
        ),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn resolve_stdio_command(command: &str) -> String {
    resolve_windows_command(command).unwrap_or_else(|| command.to_string())
}

#[cfg(not(target_os = "windows"))]
fn resolve_stdio_command(command: &str) -> String {
    command.to_string()
}

#[cfg(target_os = "windows")]
fn resolve_windows_command(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    let path = PathBuf::from(command);
    if has_path_separator(command) {
        return resolve_windows_command_in_dir(path.parent(), path.file_name()?.to_str()?);
    }

    for dir in windows_search_dirs() {
        if let Some(found) = resolve_windows_command_in_dir(Some(&dir), command) {
            return Some(found);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn resolve_windows_command_in_dir(dir: Option<&std::path::Path>, command: &str) -> Option<String> {
    let has_extension = PathBuf::from(command).extension().is_some();
    let mut candidates = Vec::new();
    if has_extension {
        candidates.push(command.to_string());
    } else {
        candidates.extend(
            windows_command_extensions()
                .into_iter()
                .map(|ext| format!("{command}{ext}")),
        );
        candidates.push(command.to_string());
    }

    for candidate in candidates {
        let path = match dir {
            Some(dir) => dir.join(&candidate),
            None => PathBuf::from(&candidate),
        };
        if path.is_file() {
            return Some(path.to_string_lossy().into_owned());
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn windows_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(app_data) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(app_data).join("npm"));
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local_app_data).join("pnpm"));
    }
    dirs
}

#[cfg(target_os = "windows")]
fn windows_command_extensions() -> Vec<String> {
    std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter_map(|ext| {
                    let ext = ext.trim();
                    if ext.is_empty() {
                        None
                    } else if ext.starts_with('.') {
                        Some(ext.to_ascii_lowercase())
                    } else {
                        Some(format!(".{}", ext.to_ascii_lowercase()))
                    }
                })
                .collect::<Vec<_>>()
        })
        .filter(|extensions| !extensions.is_empty())
        .unwrap_or_else(|| {
            vec![
                ".com".to_string(),
                ".exe".to_string(),
                ".bat".to_string(),
                ".cmd".to_string(),
            ]
        })
}

#[cfg(target_os = "windows")]
fn has_path_separator(command: &str) -> bool {
    command.contains('\\') || command.contains('/')
}

async fn connect_streamable_http_runtime(server: &Value) -> Result<DesktopRuntime, String> {
    let remote_url = server
        .get("remoteUrl")
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| "Remote URL is required for remote MCP servers".to_string())?;
    let mut config = StreamableHttpClientTransportConfig::with_uri(remote_url.to_string());
    if let Some(token) = server
        .get("bearerToken")
        .and_then(Value::as_str)
        .filter(|token| !token.trim().is_empty())
    {
        config = config.auth_header(format!("Bearer {token}"));
    }
    let transport = StreamableHttpClientTransport::from_config(config);
    let client = with_initialize_timeout(
        ClientInfo::default().serve(transport),
        initialize_timeout(server),
    )
    .await?;
    Ok(DesktopRuntime { client, pid: None })
}

async fn connect_legacy_sse_runtime(server: &Value) -> Result<DesktopRuntime, String> {
    let remote_url = server
        .get("remoteUrl")
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| "Remote URL is required for remote MCP servers".to_string())?;
    let transport =
        LegacySseClientTransport::new(remote_url.to_string(), legacy_sse_headers(server)?);
    let client = with_initialize_timeout(
        ClientInfo::default().serve(transport),
        initialize_timeout(server),
    )
    .await?;
    Ok(DesktopRuntime { client, pid: None })
}

async fn with_initialize_timeout<T, E>(
    future: impl Future<Output = Result<T, E>>,
    timeout: Duration,
) -> Result<T, String>
where
    E: std::fmt::Display,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err(format!(
            "MCP initialization timed out after {} seconds",
            timeout.as_secs()
        )),
    }
}

fn initialize_timeout(server: &Value) -> Duration {
    timeout_from_server(server, "startupTimeoutSec", DEFAULT_MCP_INITIALIZE_TIMEOUT)
}

fn timeout_from_server(server: &Value, key: &str, default: Duration) -> Duration {
    server
        .get(key)
        .and_then(Value::as_u64)
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or(default)
}

fn legacy_sse_headers(server: &Value) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    if let Some(token) = server
        .get("bearerToken")
        .and_then(Value::as_str)
        .filter(|token| !token.trim().is_empty())
    {
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).map_err(|error| error.to_string())?,
        );
    }
    Ok(headers)
}
