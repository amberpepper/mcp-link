use super::model::{AgentConfigFileDefinition, AgentInstance, AgentPluginDescriptor};
use super::{descriptors, resolve_instance_path, resolve_selected_config, util, SelectedConfig};
use crate::{state::DesktopState, util::json::required_string};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use crate::util::time::now_millis;

const MAX_AGENT_CONFIG_FILE_SIZE: u64 = 8 * 1024 * 1024;
const MAX_CONFIG_BACKUPS: usize = 20;

pub(crate) fn list_agent_config_files(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let instance_id = required_string(args, 0)?;
    let (descriptor, selected) = config_context(state, &instance_id, false)?;
    let files = descriptor
        .config_files
        .iter()
        .filter_map(|definition| {
            resolve_instance_path(Some(&definition.path_template), &selected)
                .map(|path| config_file_summary(definition, Path::new(&path)))
        })
        .collect::<Vec<_>>();
    Ok(Value::Array(files))
}

pub(crate) fn read_agent_config_file(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let instance_id = required_string(args, 0)?;
    let file_id = required_string(args, 1)?;
    let (definition, path) = config_file_context(state, &instance_id, &file_id, false)?;
    read_config_document(&definition, &path)
}

pub(crate) fn save_agent_config_file(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let instance_id = required_string(args, 0)?;
    let file_id = required_string(args, 1)?;
    let content = required_string(args, 2)?;
    let expected_revision = args.get(3).and_then(Value::as_str);
    if content.len() as u64 > MAX_AGENT_CONFIG_FILE_SIZE {
        return Err("Agent configuration file is too large".to_string());
    }
    let (definition, path) = config_file_context(state, &instance_id, &file_id, true)?;
    if path.exists() && !path.is_file() {
        return Err(format!(
            "Agent configuration path is not a file: {}",
            path.to_string_lossy()
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Agent configuration path has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let current_revision = config_revision(&path)?;
    if let Some(expected_revision) = expected_revision {
        if current_revision != expected_revision {
            return Err(format!(
                "CONFIG_CONFLICT: configuration changed on disk (expected {expected_revision}, found {current_revision})"
            ));
        }
    }
    atomic_write(&path, &content)?;
    read_config_document(&definition, &path)
}

fn config_context(
    state: &DesktopState,
    instance_id: &str,
    write: bool,
) -> Result<(AgentPluginDescriptor, SelectedConfig), String> {
    let instance = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?
        .agent_instances
        .iter()
        .find(|value| value.get("id").and_then(Value::as_str) == Some(instance_id))
        .cloned()
        .map(serde_json::from_value::<AgentInstance>)
        .transpose()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Agent instance not found: {instance_id}"))?;
    let descriptor = descriptors(state)
        .into_iter()
        .find(|descriptor| descriptor.id == instance.agent_id)
        .ok_or_else(|| format!("Agent plugin not found: {}", instance.agent_id))?;
    let capability = if write { "config.write" } else { "config.read" };
    if !descriptor
        .capabilities
        .iter()
        .any(|item| item == capability)
    {
        return Err(format!("Agent plugin does not support {capability}"));
    }
    let root = instance
        .cli_root
        .as_deref()
        .ok_or_else(|| "CLI configuration directory is required".to_string())?;
    let selected = resolve_selected_config(root, descriptor.instance_config.home_levels_up)?;
    Ok((descriptor, selected))
}

fn config_file_context(
    state: &DesktopState,
    instance_id: &str,
    file_id: &str,
    write: bool,
) -> Result<(AgentConfigFileDefinition, PathBuf), String> {
    let (descriptor, selected) = config_context(state, instance_id, write)?;
    let definition = descriptor
        .config_files
        .into_iter()
        .find(|definition| definition.id == file_id)
        .ok_or_else(|| format!("Agent configuration file not found: {file_id}"))?;
    let path = resolve_instance_path(Some(&definition.path_template), &selected)
        .map(PathBuf::from)
        .ok_or_else(|| format!("Unable to resolve agent configuration file: {file_id}"))?;
    Ok((definition, path))
}

fn config_file_summary(definition: &AgentConfigFileDefinition, path: &Path) -> Value {
    json!({
        "id": definition.id,
        "label": definition.label,
        "path": path.to_string_lossy(),
        "language": definition.language,
        "kind": definition.kind,
        "exists": path.is_file(),
        "modifiedAt": util::modified_millis(path),
    })
}

fn read_config_document(
    definition: &AgentConfigFileDefinition,
    path: &Path,
) -> Result<Value, String> {
    let exists = path.is_file();
    if path.exists() && !exists {
        return Err(format!(
            "Agent configuration path is not a file: {}",
            path.to_string_lossy()
        ));
    }
    let content = if exists {
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        if metadata.len() > MAX_AGENT_CONFIG_FILE_SIZE {
            return Err("Agent configuration file is too large".to_string());
        }
        fs::read_to_string(path).map_err(|error| error.to_string())?
    } else {
        definition.default_content.clone().unwrap_or_default()
    };
    let mut value = config_file_summary(definition, path);
    value
        .as_object_mut()
        .expect("config file summary is an object")
        .insert("content".to_string(), Value::String(content));
    value
        .as_object_mut()
        .expect("config file summary is an object")
        .insert(
            "revision".to_string(),
            Value::String(config_revision(path)?),
        );
    Ok(value)
}

fn config_revision(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok("missing".to_string());
    }
    if !path.is_file() {
        return Err(format!(
            "Agent configuration path is not a file: {}",
            path.display()
        ));
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Agent configuration path has no parent directory".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    let stamp = now_millis();
    let nonce = Uuid::new_v4().simple();
    let temporary = parent.join(format!(".{file_name}.mcp-link-{stamp}-{nonce}.tmp"));
    fs::write(&temporary, content).map_err(|error| error.to_string())?;
    let backup = if path.is_file() {
        let backup_root = parent.join(".mcp-link-backups");
        fs::create_dir_all(&backup_root).map_err(|error| error.to_string())?;
        let backup = backup_root.join(format!("{file_name}-{stamp}-{nonce}.bak"));
        fs::rename(path, &backup).map_err(|error| error.to_string())?;
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(&temporary, path) {
        if let Some(backup) = backup.as_ref() {
            let _ = fs::rename(backup, path);
        }
        let _ = fs::remove_file(temporary);
        return Err(error.to_string());
    }
    prune_config_backups(&parent.join(".mcp-link-backups"), file_name);
    Ok(())
}

fn prune_config_backups(root: &Path, file_name: &str) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let prefix = format!("{file_name}-");
    let mut backups = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".bak"))
        })
        .collect::<Vec<_>>();
    backups.sort();
    let remove_count = backups.len().saturating_sub(MAX_CONFIG_BACKUPS);
    for backup in backups.into_iter().take(remove_count) {
        let _ = fs::remove_file(backup);
    }
}
