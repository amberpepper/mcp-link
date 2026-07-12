use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
};

use serde_json::{json, Map, Value};
use uuid::Uuid;
use zip::ZipArchive;

use crate::{
    mcp::servers::validate_server,
    state::{normalize_server, save_store, DesktopState},
};

const MAX_BUNDLE_SIZE: usize = 256 * 1024 * 1024;
const MAX_MANIFEST_SIZE: u64 = 2 * 1024 * 1024;
const BUNDLE_MARKER: &str = ".mcp-link-bundle";

pub(crate) fn import_dxt_server(state: &DesktopState, input: &Value) -> Result<Value, String> {
    let bytes = input
        .get("dxtFile")
        .cloned()
        .ok_or_else(|| "DXT file data is required".to_string())
        .and_then(|value| serde_json::from_value::<Vec<u8>>(value).map_err(|e| e.to_string()))?;
    if bytes.is_empty() {
        return Err("DXT file is empty".to_string());
    }
    if bytes.len() > MAX_BUNDLE_SIZE {
        return Err("DXT file exceeds the 256 MB limit".to_string());
    }

    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Invalid DXT archive: {error}"))?;
    let manifest = read_manifest(&mut archive)?;
    let server_id = Uuid::new_v4().to_string();
    let install_dir = state
        .store_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("extensions")
        .join(&server_id);
    fs::create_dir_all(&install_dir).map_err(|error| error.to_string())?;

    if let Err(error) = extract_archive(&mut archive, &install_dir) {
        let _ = fs::remove_dir_all(&install_dir);
        return Err(error);
    }
    fs::write(install_dir.join(BUNDLE_MARKER), &server_id).map_err(|error| {
        let _ = fs::remove_dir_all(&install_dir);
        error.to_string()
    })?;

    let config = match manifest_to_server_config(&manifest, &server_id, &install_dir) {
        Ok(config) => config,
        Err(error) => {
            let _ = fs::remove_dir_all(&install_dir);
            return Err(error);
        }
    };
    let server = normalize_server(&config);
    if let Err(error) = validate_server(&server) {
        let _ = fs::remove_dir_all(&install_dir);
        return Err(error);
    }
    let result = {
        let mut store = match state.store.lock() {
            Ok(store) => store,
            Err(_) => {
                let _ = fs::remove_dir_all(&install_dir);
                return Err("Failed to lock desktop state".to_string());
            }
        };
        store.servers.push(server.clone());
        if let Err(error) = save_store(&state.store_path, &store) {
            store
                .servers
                .retain(|item| item.get("id") != server.get("id"));
            let _ = fs::remove_dir_all(&install_dir);
            return Err(error);
        }
        server
    };
    Ok(result)
}

pub(crate) fn remove_installed_bundle(state: &DesktopState, server: &Value) -> Result<(), String> {
    let Some(server_id) = server.get("id").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(path) = server.get("bundlePath").and_then(Value::as_str) else {
        return Ok(());
    };
    let path = std::path::PathBuf::from(path);
    let expected_root = state
        .store_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("extensions");
    if path.parent() != Some(expected_root.as_path()) {
        return Err("Refusing to remove a bundle outside the extensions directory".to_string());
    }
    let marker = path.join(BUNDLE_MARKER);
    let marker_id = fs::read_to_string(&marker).unwrap_or_default();
    if marker_id.trim() != server_id {
        return Err("Refusing to remove an unverified bundle directory".to_string());
    }
    fs::remove_dir_all(path).map_err(|error| error.to_string())
}

fn read_manifest(archive: &mut ZipArchive<Cursor<Vec<u8>>>) -> Result<Value, String> {
    let mut file = archive
        .by_name("manifest.json")
        .map_err(|_| "DXT archive does not contain manifest.json".to_string())?;
    if file.size() > MAX_MANIFEST_SIZE {
        return Err("DXT manifest exceeds the 2 MB limit".to_string());
    }
    let mut body = String::new();
    file.read_to_string(&mut body)
        .map_err(|error| format!("Failed to read DXT manifest: {error}"))?;
    serde_json::from_str(&body).map_err(|error| format!("Invalid DXT manifest: {error}"))
}

fn extract_archive(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    install_dir: &Path,
) -> Result<(), String> {
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe path in DXT archive: {}", entry.name()))?;
        let output = install_dir.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut file = fs::File::create(&output).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn manifest_to_server_config(
    manifest: &Value,
    server_id: &str,
    install_dir: &Path,
) -> Result<Value, String> {
    let server = manifest
        .get("server")
        .and_then(Value::as_object)
        .ok_or_else(|| "DXT manifest is missing server configuration".to_string())?;
    let mcp_config = server
        .get("mcp_config")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let server_type = server.get("type").and_then(Value::as_str).unwrap_or("node");
    let entry_point = server
        .get("entry_point")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expand = |value: &str| expand_manifest_template(value, install_dir);

    let command = mcp_config
        .get("command")
        .and_then(Value::as_str)
        .map(&expand)
        .or_else(|| match server_type {
            "node" => Some("node".to_string()),
            "python" | "uv" => Some("python".to_string()),
            "binary" if !entry_point.is_empty() => {
                Some(install_dir.join(entry_point).to_string_lossy().into_owned())
            }
            _ => None,
        })
        .ok_or_else(|| "DXT manifest does not define a runnable command".to_string())?;

    let mut args = mcp_config
        .get("args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(&expand)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if args.is_empty() && !entry_point.is_empty() && matches!(server_type, "node" | "python") {
        args.push(install_dir.join(entry_point).to_string_lossy().into_owned());
    }

    let env = mcp_config
        .get("env")
        .and_then(Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(|value| (key.clone(), Value::String(expand(value))))
                })
                .collect::<Map<_, _>>()
        })
        .unwrap_or_default();
    let input_params = manifest
        .get("user_config")
        .and_then(Value::as_object)
        .map(|params| {
            params
                .iter()
                .map(|(key, param)| {
                    let mut param = param.clone();
                    if let Some(default) = param.get_mut("default") {
                        expand_default_value(default, install_dir);
                    }
                    (key.clone(), param)
                })
                .collect::<Map<_, _>>()
        })
        .unwrap_or_default();

    Ok(json!({
        "id": server_id,
        "name": manifest.get("display_name").and_then(Value::as_str)
            .or_else(|| manifest.get("name").and_then(Value::as_str))
            .unwrap_or("MCP Bundle"),
        "description": manifest.get("description").and_then(Value::as_str).unwrap_or(""),
        "serverType": "local",
        "command": command,
        "args": args,
        "env": env,
        "inputParams": input_params,
        "autoStart": false,
        "disabled": false,
        "version": manifest.get("version").cloned().unwrap_or(Value::Null),
        "setupInstructions": manifest.get("documentation").cloned()
            .or_else(|| manifest.get("homepage").cloned())
            .unwrap_or(Value::Null),
        "bundlePath": install_dir.to_string_lossy()
    }))
}

fn expand_manifest_template(value: &str, install_dir: &Path) -> String {
    let mut value = value.replace("${__dirname}", install_dir.to_string_lossy().as_ref());
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = home.to_string_lossy();
        value = value.replace("${HOME}", home.as_ref());
        value = value.replace("${USERPROFILE}", home.as_ref());
    }
    value
}

fn expand_default_value(value: &mut Value, install_dir: &Path) {
    match value {
        Value::String(text) => *text = expand_manifest_template(text, install_dir),
        Value::Array(values) => {
            for value in values {
                expand_default_value(value, install_dir);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_builds_local_server_config() {
        let manifest = json!({
            "manifest_version": "0.3",
            "name": "sample",
            "display_name": "Sample Bundle",
            "version": "1.0.0",
            "server": {
                "type": "node",
                "entry_point": "server/index.js",
                "mcp_config": {
                    "command": "node",
                    "args": ["${__dirname}/server/index.js"],
                    "env": { "API_KEY": "${user_config.api_key}" }
                }
            },
            "user_config": {
                "api_key": { "type": "string", "required": true, "sensitive": true }
            }
        });
        let config = manifest_to_server_config(&manifest, "bundle-1", Path::new("C:/bundle"))
            .expect("manifest should convert");
        assert_eq!(
            config.get("name").and_then(Value::as_str),
            Some("Sample Bundle")
        );
        assert_eq!(
            config
                .get("args")
                .and_then(Value::as_array)
                .and_then(|args| args.first())
                .and_then(Value::as_str),
            Some("C:/bundle/server/index.js")
        );
    }
}
