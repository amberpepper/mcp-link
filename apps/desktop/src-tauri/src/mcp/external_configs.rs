use std::{fs, path::PathBuf};

use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    mcp::servers::validate_server,
    state::{normalize_server, save_store, DesktopState},
};

pub(crate) fn import_external_mcp_configs(state: &DesktopState) -> Result<usize, String> {
    let sources = external_config_paths();
    let mut candidates = Vec::new();
    for path in sources {
        if !path.is_file() {
            continue;
        }
        let body = match fs::read_to_string(&path) {
            Ok(body) => body,
            Err(error) => {
                eprintln!(
                    "Failed to read external MCP config {}: {error}",
                    path.display()
                );
                continue;
            }
        };
        let parsed: Value = match serde_json::from_str(&body) {
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!(
                    "Failed to parse external MCP config {}: {error}",
                    path.display()
                );
                continue;
            }
        };
        let Some(servers) = parsed
            .get("mcpServers")
            .or_else(|| parsed.get("servers"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for (name, config) in servers {
            if let Some(server) = external_server_config(name, config, &path) {
                candidates.push(server);
            }
        }
    }

    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    let mut imported = 0;
    for candidate in candidates {
        let name = candidate
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if store
            .servers
            .iter()
            .any(|server| server.get("name").and_then(Value::as_str) == Some(name))
        {
            continue;
        }
        let server = normalize_server(&candidate);
        if validate_server(&server).is_ok() {
            store.servers.push(server);
            imported += 1;
        }
    }
    if imported > 0 {
        save_store(&state.store_path, &store)?;
    }
    Ok(imported)
}

fn external_server_config(name: &str, config: &Value, source: &std::path::Path) -> Option<Value> {
    let remote_url = config
        .get("url")
        .or_else(|| config.get("remoteUrl"))
        .and_then(Value::as_str);
    let bearer_token = config
        .get("bearerToken")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            config
                .get("headers")
                .and_then(Value::as_object)
                .and_then(|headers| headers.get("Authorization"))
                .and_then(Value::as_str)
                .and_then(|value| value.strip_prefix("Bearer "))
                .map(str::to_string)
        });
    let source = source.to_string_lossy();
    if let Some(remote_url) = remote_url {
        return Some(json!({
            "id": Uuid::new_v4().to_string(),
            "name": name,
            "serverType": if config.get("type").and_then(Value::as_str) == Some("sse") { "remote" } else { "remote-streamable" },
            "remoteUrl": remote_url,
            "bearerToken": bearer_token,
            "env": {},
            "args": [],
            "autoStart": false,
            "disabled": false,
            "importSource": source
        }));
    }

    let command = config.get("command").and_then(Value::as_str)?;
    Some(json!({
        "id": Uuid::new_v4().to_string(),
        "name": name,
        "serverType": "local",
        "command": command,
        "args": config.get("args").and_then(Value::as_array).cloned().unwrap_or_default(),
        "env": config.get("env").and_then(Value::as_object).cloned().unwrap_or_default(),
        "autoStart": false,
        "disabled": false,
        "importSource": source
    }))
}

fn external_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(app_data) = std::env::var_os("APPDATA") {
        let app_data = PathBuf::from(app_data);
        paths.push(app_data.join("Claude").join("claude_desktop_config.json"));
        paths.push(app_data.join("Code").join("User").join("mcp.json"));
        paths.push(
            app_data
                .join("Code")
                .join("User")
                .join("globalStorage")
                .join("saoudrizwan.claude-dev")
                .join("settings")
                .join("cline_mcp_settings.json"),
        );
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);
        paths.push(home.join(".cursor").join("mcp.json"));
        paths.push(
            home.join(".codeium")
                .join("windsurf")
                .join("mcp_config.json"),
        );
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_remote_config_extracts_bearer_token() {
        let config = json!({
            "url": "https://example.com/mcp",
            "headers": { "Authorization": "Bearer secret" }
        });
        let server = external_server_config("remote", &config, std::path::Path::new("config.json"))
            .expect("remote config should parse");
        assert_eq!(
            server.get("bearerToken").and_then(Value::as_str),
            Some("secret")
        );
    }
}
