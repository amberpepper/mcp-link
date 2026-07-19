use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc};
use tauri_plugin_dialog::DialogExt;

use crate::{platform::skills::create_skill_files, state::DesktopState, util::json::value_id};

pub(crate) async fn select_path(
    app: tauri::AppHandle,
    options: Option<&Value>,
) -> Result<Value, String> {
    let mode = options
        .and_then(|value| value.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("file");
    let title = options
        .and_then(|value| value.get("title"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    match pick_path(app, mode, title).await? {
        Some(path) => Ok(json!({
            "success": true,
            "path": path
        })),
        None => Ok(json!({
            "success": true,
            "canceled": true
        })),
    }
}

async fn pick_path(
    app: tauri::AppHandle,
    mode: &str,
    title: Option<String>,
) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file();
    if let Some(title) = title {
        dialog = dialog.set_title(title);
    }

    if mode == "directory" {
        dialog.pick_folder(move |path| {
            let _ = tx.send(path.map(|path| path.to_string()));
        });
    } else {
        dialog.pick_file(move |path| {
            let _ = tx.send(path.map(|path| path.to_string()));
        });
    }

    rx.await.map_err(|error| error.to_string())
}

pub(crate) async fn open_skill_folder(
    _app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DesktopState>>,
    args: Vec<Value>,
) -> Result<Value, String> {
    let skills_dir = state
        .store_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("skills");
    fs::create_dir_all(&skills_dir).map_err(|error| error.to_string())?;

    let path = if let Some(skill_id) = args.first().and_then(Value::as_str) {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        store
            .skills
            .iter()
            .find(|skill| value_id(skill) == Some(skill_id))
            .and_then(|skill| skill.get("path").and_then(Value::as_str))
            .map(PathBuf::from)
            .unwrap_or_else(|| skills_dir.join(skill_id))
    } else {
        skills_dir
    };

    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    tauri_plugin_opener::open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

pub(crate) async fn import_skill(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<Value, String> {
    let Some(path) = pick_path(app, "directory", Some("Import Skill".to_string())).await? else {
        return Err("No folder selected".to_string());
    };
    let path_buf = PathBuf::from(&path);
    let name = path_buf
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("imported-skill")
        .to_string();
    let content = fs::read_to_string(path_buf.join("SKILL.md")).unwrap_or_default();
    let input = json!({
        "name": name,
        "path": path,
        "content": content
    });

    create_skill_files(&state, Some(&input), Some(&path_buf))
}
