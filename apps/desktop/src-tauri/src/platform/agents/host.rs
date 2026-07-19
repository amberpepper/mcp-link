use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

use rusqlite::{
    types::{Value as SqlValue, ValueRef},
    Connection, OpenFlags,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::util::time::now_millis;

use super::super::model::AgentInstance;
use super::{ExternalPlugin, PluginDatabase, PluginFileResource};

const MAX_FILE_SIZE: u64 = 128 * 1024 * 1024;
const MAX_BACKUPS_PER_RESOURCE: usize = 20;
const MAX_QUERY_ROWS: usize = 10_000;

pub(crate) fn dispatch(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    request: &Value,
) -> Result<Value, String> {
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| "Host request method is required".to_string())?;
    let params = request.get("params").unwrap_or(&Value::Null);
    match method {
        "file.read" => file_read(plugin, instance, params),
        "file.write" => file_write(plugin, instance, params),
        "file.writeAtomic" => file_write_atomic(plugin, instance, params),
        "file.list" => file_list(plugin, instance, params),
        "file.remove" => file_remove(plugin, instance, params),
        "sqlite.query" => sqlite_query(plugin, instance, params),
        "sqlite.transaction" => sqlite_transaction(plugin, instance, params),
        _ => Err(format!("Unsupported plugin host method: {method}")),
    }
}

fn file_read(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    params: &Value,
) -> Result<Value, String> {
    let resource = file_resource(plugin, params, false)?;
    let root = resource_path(plugin, instance, &resource.path_template)?;
    let path = safe_child_path(&root, relative_param(params)?)?;
    let include_revision = params
        .get("includeRevision")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if include_revision && !path.exists() {
        return Ok(json!({
            "path": path.to_string_lossy(),
            "content": "",
            "start": 0,
            "end": 0,
            "fileSize": 0,
            "revision": "missing",
        }));
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(format!(
            "Plugin file path is not a file: {}",
            path.display()
        ));
    }
    let max_bytes = params
        .get("maxBytes")
        .and_then(Value::as_u64)
        .map(|value| value.min(MAX_FILE_SIZE) as usize);
    if max_bytes.is_none() && metadata.len() > MAX_FILE_SIZE {
        return Err("Plugin file is too large".to_string());
    }
    let (content, start, end) = if let Some(max_bytes) = max_bytes {
        let file_size = metadata.len();
        let requested_offset = params.get("offset").and_then(Value::as_u64);
        let requested_before = params
            .get("before")
            .map(|value| value.as_u64().unwrap_or(file_size));
        let (start, end, align_first_line) = if let Some(offset) = requested_offset {
            let start = offset.min(file_size);
            (
                start,
                start.saturating_add(max_bytes as u64).min(file_size),
                false,
            )
        } else if let Some(before) = requested_before {
            let end = before.min(file_size);
            (end.saturating_sub(max_bytes as u64), end, true)
        } else {
            (0, (max_bytes as u64).min(file_size), false)
        };
        let mut file = fs::File::open(&path).map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(start))
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity((end - start) as usize);
        file.take(end - start)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        let mut aligned_start = start;
        if align_first_line && start > 0 {
            if let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
                bytes.drain(..=index);
                aligned_start = start + index as u64 + 1;
            } else {
                bytes.clear();
                aligned_start = start;
            }
        }
        (
            String::from_utf8_lossy(&bytes).into_owned(),
            aligned_start,
            end,
        )
    } else {
        (
            fs::read_to_string(&path).map_err(|error| error.to_string())?,
            0,
            metadata.len(),
        )
    };
    let revision = include_revision.then(|| file_revision(&path)).transpose()?;
    Ok(json!({
        "path": path.to_string_lossy(),
        "content": content,
        "start": start,
        "end": end,
        "fileSize": metadata.len(),
        "revision": revision,
    }))
}

fn file_write(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    params: &Value,
) -> Result<Value, String> {
    let resource = file_resource(plugin, params, true)?;
    let path = resource_path(plugin, instance, &resource.path_template)?;
    let relative = relative_param(params)?;
    let target = safe_child_path(&path, relative)?;
    let content = params
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "file.write content is required".to_string())?;
    if content.len() as u64 > MAX_FILE_SIZE {
        return Err("Plugin file is too large".to_string());
    }
    if target.exists() && !target.is_file() {
        return Err(format!(
            "Plugin file path is not a file: {}",
            target.display()
        ));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&target, content).map_err(|error| error.to_string())?;
    Ok(json!({ "path": target.to_string_lossy(), "bytes": content.len() }))
}

fn file_write_atomic(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    params: &Value,
) -> Result<Value, String> {
    let resource = file_resource(plugin, params, true)?;
    let root = resource_path(plugin, instance, &resource.path_template)?;
    let target = safe_child_path(&root, relative_param(params)?)?;
    let content = params
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "file.writeAtomic content is required".to_string())?;
    if content.len() as u64 > MAX_FILE_SIZE {
        return Err("Plugin file is too large".to_string());
    }
    let expected_revision = params
        .get("expectedRevision")
        .and_then(Value::as_str)
        .ok_or_else(|| "file.writeAtomic expectedRevision is required".to_string())?;
    let (revision, backup) = write_atomic_path(&target, content, expected_revision)?;
    Ok(json!({
        "path": target.to_string_lossy(),
        "bytes": content.len(),
        "revision": revision,
        "backupPath": backup.map(|path| path.to_string_lossy().into_owned()),
    }))
}

fn write_atomic_path(
    target: &Path,
    content: &str,
    expected_revision: &str,
) -> Result<(String, Option<PathBuf>), String> {
    let current_revision = file_revision(target)?;
    if current_revision != expected_revision {
        return Err(format!(
            "CONFIG_CONFLICT: configuration changed on disk (expected {expected_revision}, found {current_revision})"
        ));
    }
    if target.exists() && !target.is_file() {
        return Err(format!(
            "Plugin file path is not a file: {}",
            target.display()
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| "Plugin file path has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    let stamp = now_millis();
    let nonce = Uuid::new_v4().simple();
    let temporary = parent.join(format!(".{file_name}.mcp-link-{stamp}-{nonce}.tmp"));
    fs::write(&temporary, content).map_err(|error| error.to_string())?;
    let backup = if target.is_file() {
        let backup_root = parent.join(".mcp-link-backups");
        fs::create_dir_all(&backup_root).map_err(|error| error.to_string())?;
        let backup = backup_root.join(format!("{file_name}-{stamp}-{nonce}.bak"));
        fs::rename(target, &backup).map_err(|error| error.to_string())?;
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(&temporary, target) {
        if let Some(backup) = backup.as_ref() {
            let _ = fs::rename(backup, target);
        }
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    prune_backups(&parent.join(".mcp-link-backups"), file_name);
    Ok((file_revision(target)?, backup))
}

fn prune_backups(root: &Path, file_name: &str) {
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
    let remove_count = backups.len().saturating_sub(MAX_BACKUPS_PER_RESOURCE);
    for backup in backups.into_iter().take(remove_count) {
        let _ = fs::remove_file(backup);
    }
}

fn file_revision(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok("missing".to_string());
    }
    if !path.is_file() {
        return Err(format!(
            "Plugin file path is not a file: {}",
            path.display()
        ));
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod atomic_write_tests {
    use super::*;

    fn test_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mcp-link-{label}-{}-{}",
            std::process::id(),
            now_millis()
        ))
    }

    #[test]
    fn atomic_write_creates_backup_and_updates_revision() {
        let root = test_directory("atomic-write");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("config.json");
        fs::write(&target, "old").unwrap();
        let expected = file_revision(&target).unwrap();
        let (revision, backup) = write_atomic_path(&target, "new", &expected).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_ne!(revision, expected);
        assert_eq!(
            fs::read_to_string(backup.expect("backup path")).unwrap(),
            "old"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_write_rejects_stale_revision_without_modifying_file() {
        let root = test_directory("atomic-conflict");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("config.json");
        fs::write(&target, "current").unwrap();
        let error = write_atomic_path(&target, "new", "sha256:stale").unwrap_err();
        assert!(error.starts_with("CONFIG_CONFLICT:"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "current");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_write_creates_missing_file() {
        let root = test_directory("atomic-create");
        let target = root.join("config.json");
        let (_, backup) = write_atomic_path(&target, "created", "missing").unwrap();
        assert!(backup.is_none());
        assert_eq!(fs::read_to_string(&target).unwrap(), "created");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_write_prunes_old_backups() {
        let root = test_directory("atomic-prune");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("config.json");
        fs::write(&target, "0").unwrap();
        for value in 1..=MAX_BACKUPS_PER_RESOURCE + 5 {
            let expected = file_revision(&target).unwrap();
            write_atomic_path(&target, &value.to_string(), &expected).unwrap();
        }
        let backup_count = fs::read_dir(root.join(".mcp-link-backups"))
            .unwrap()
            .flatten()
            .count();
        assert_eq!(backup_count, MAX_BACKUPS_PER_RESOURCE);
        fs::remove_dir_all(root).unwrap();
    }
}

fn file_list(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    params: &Value,
) -> Result<Value, String> {
    let resource = file_resource(plugin, params, false)?;
    let root = resource_path(plugin, instance, &resource.path_template)?;
    let relative = relative_param(params).unwrap_or_default();
    let directory = safe_child_path(&root, relative)?;
    let entries = fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| {
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| value.as_millis() as u64);
            Ok(json!({
                "name": entry.file_name().to_string_lossy(),
                "directory": metadata.is_dir(),
                "size": metadata.len(),
                "modifiedAt": modified_at,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Value::Array(entries))
}

fn file_remove(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    params: &Value,
) -> Result<Value, String> {
    let resource = file_resource(plugin, params, true)?;
    let root = resource_path(plugin, instance, &resource.path_template)?;
    let target = safe_child_path(&root, relative_param(params)?)?;
    if !target.is_file() {
        return Err(format!("Plugin file was not found: {}", target.display()));
    }
    let trash = root.join(".mcp-link-trash");
    fs::create_dir_all(&trash).map_err(|error| error.to_string())?;
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let backup = trash.join(format!("{file_name}-{}", now_millis()));
    fs::rename(&target, &backup).map_err(|error| error.to_string())?;
    Ok(json!({ "removed": true, "backupPath": backup.to_string_lossy() }))
}

fn sqlite_query(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    params: &Value,
) -> Result<Value, String> {
    let database = database(plugin, params, false)?;
    let path = database_path(plugin, instance, database)?;
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    let sql = required_sql(params)?;
    let sql_params = sql_params(params)?;
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let columns = statement
        .column_names()
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let rows = statement
        .query_map(rusqlite::params_from_iter(sql_params), |row| {
            let mut object = Map::new();
            for (index, column) in columns.iter().enumerate() {
                let value = sql_value_ref(row.get_ref(index)?)?;
                object.insert(column.clone(), value);
            }
            Ok(Value::Object(object))
        })
        .map_err(|error| error.to_string())?
        .take(MAX_QUERY_ROWS)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(Value::Array(rows))
}

fn sqlite_transaction(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    params: &Value,
) -> Result<Value, String> {
    let database = database(plugin, params, true)?;
    let path = database_path(plugin, instance, database)?;
    backup_database(&path)?;
    let mut connection = Connection::open(&path).map_err(|error| error.to_string())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let statements = params
        .get("statements")
        .and_then(Value::as_array)
        .ok_or_else(|| "sqlite.transaction statements are required".to_string())?;
    let mut affected = 0_u64;
    for statement in statements {
        let sql = required_sql(statement)?;
        let sql_params = sql_params(statement)?;
        affected = affected.saturating_add(
            transaction
                .execute(sql, rusqlite::params_from_iter(sql_params))
                .map_err(|error| error.to_string())? as u64,
        );
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(json!({ "affected": affected }))
}

fn file_resource<'a>(
    plugin: &'a ExternalPlugin,
    params: &Value,
    write: bool,
) -> Result<&'a PluginFileResource, String> {
    let id = params
        .get("resourceId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin file resourceId is required".to_string())?;
    let resource = plugin
        .manifest
        .files
        .iter()
        .find(|resource| resource.id == id)
        .ok_or_else(|| format!("Plugin file resource not found: {id}"))?;
    if write && resource.access != "read-write" {
        return Err(format!("Plugin file resource is read-only: {id}"));
    }
    Ok(resource)
}

fn database<'a>(
    plugin: &'a ExternalPlugin,
    params: &Value,
    write: bool,
) -> Result<&'a PluginDatabase, String> {
    let id = params
        .get("databaseId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin databaseId is required".to_string())?;
    let database = plugin
        .manifest
        .databases
        .iter()
        .find(|database| database.id == id)
        .ok_or_else(|| format!("Plugin database not found: {id}"))?;
    if write && database.access != "read-write" {
        return Err(format!("Plugin database is read-only: {id}"));
    }
    Ok(database)
}

fn resource_path(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    template: &str,
) -> Result<PathBuf, String> {
    let root = instance
        .cli_root
        .as_deref()
        .ok_or_else(|| "Plugin instance configuration root is missing".to_string())?;
    let session_root = instance.session_root.as_deref().unwrap_or(root);
    let mut home = PathBuf::from(root);
    for _ in 0..plugin.manifest.instance_config.home_levels_up {
        let Some(parent) = home.parent() else {
            break;
        };
        home = parent.to_path_buf();
    }
    let home = home.to_string_lossy();
    let normalized_root = root.replace('/', "\\").to_ascii_lowercase();
    let is_wsl = normalized_root.starts_with("\\\\wsl.localhost\\")
        || normalized_root.starts_with("\\\\wsl$\\");
    let local_app_data = if is_wsl {
        PathBuf::from(home.as_ref()).join(".local").join("share")
    } else if cfg!(windows) {
        PathBuf::from(home.as_ref()).join("AppData").join("Local")
    } else {
        PathBuf::from(home.as_ref()).join(".local").join("share")
    };
    let path = template
        .replace("${ROOT}", root)
        .replace("${SESSION_ROOT}", session_root)
        .replace("${HOME}", &home)
        .replace("${LOCALAPPDATA}", &local_app_data.to_string_lossy());
    Ok(PathBuf::from(path))
}

fn database_path(
    plugin: &ExternalPlugin,
    instance: &AgentInstance,
    database: &PluginDatabase,
) -> Result<PathBuf, String> {
    resource_path(plugin, instance, &database.path_template)
}

fn relative_param(params: &Value) -> Result<&str, String> {
    params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin file path is required".to_string())
}

fn safe_child_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() {
        return Ok(root.to_path_buf());
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Plugin file path must stay inside its resource root".to_string());
    }
    Ok(root.join(relative_path))
}

fn required_sql(value: &Value) -> Result<&str, String> {
    value
        .get("sql")
        .and_then(Value::as_str)
        .filter(|sql| !sql.trim().is_empty())
        .ok_or_else(|| "SQL statement is required".to_string())
}

fn sql_params(value: &Value) -> Result<Vec<SqlValue>, String> {
    let values = value
        .get("params")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    values
        .iter()
        .map(|value| match value {
            Value::Null => Ok(SqlValue::Null),
            Value::Bool(value) => Ok(SqlValue::Integer(i64::from(*value))),
            Value::Number(value) if value.is_i64() => {
                Ok(SqlValue::Integer(value.as_i64().unwrap_or_default()))
            }
            Value::Number(value) => Ok(SqlValue::Real(value.as_f64().unwrap_or_default())),
            Value::String(value) => Ok(SqlValue::Text(value.clone())),
            _ => Err("SQL parameters must be null, boolean, number, or string".to_string()),
        })
        .collect()
}

fn sql_value_ref(value: ValueRef<'_>) -> rusqlite::Result<Value> {
    Ok(match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => Value::String(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            value,
        )),
    })
}

fn backup_database(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "SQLite database has no parent directory".to_string())?
        .join(".mcp-link-backups");
    fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("database");
    let backup = parent.join(format!(
        "{file_name}-{}-{}.bak",
        now_millis(),
        Uuid::new_v4().simple()
    ));
    fs::copy(path, backup).map_err(|error| error.to_string())?;
    prune_backups(&parent, file_name);
    Ok(())
}
