use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    platform::agents::{
        install_native_skill, list_skill_targets, remove_native_skill, resolve_skill_target,
    },
    state::{find_entity_mut, save_store, DesktopState},
    util::{
        json::{merge_value_object, required_string, value_id},
        time::now_millis,
    },
};

const MANAGED_MARKER: &str = ".mcp-link-managed";

pub(crate) fn list_skills_with_installations(state: &DesktopState) -> Result<Value, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    let skills = store
        .skills
        .iter()
        .cloned()
        .map(|mut skill| {
            let id = value_id(&skill).unwrap_or_default();
            skill["installations"] = Value::Array(
                store
                    .skill_installations
                    .iter()
                    .filter(|installation| {
                        installation.get("skillId").and_then(Value::as_str) == Some(id)
                    })
                    .cloned()
                    .collect(),
            );
            skill
        })
        .collect();
    Ok(Value::Array(skills))
}

pub(crate) fn create_skill_files(
    state: &DesktopState,
    input: Option<&Value>,
    source_dir: Option<&Path>,
) -> Result<Value, String> {
    let input = input.cloned().unwrap_or_else(|| json!({}));
    let name = safe_skill_name(input.get("name").and_then(Value::as_str).unwrap_or("skill"))?;
    let id = Uuid::new_v4().to_string();
    let skill_dir = skills_root(state).join(&name);
    if skill_dir.exists() {
        return Err(format!("Skill already exists: {name}"));
    }
    fs::create_dir_all(&skill_dir).map_err(|error| error.to_string())?;
    let result = (|| {
        if let Some(source_dir) = source_dir {
            copy_directory(source_dir, &skill_dir)?;
        }
        let content = input.get("content").and_then(Value::as_str).unwrap_or("");
        let skill_file = skill_dir.join("SKILL.md");
        if !skill_file.exists() || !content.is_empty() {
            fs::write(&skill_file, content).map_err(|error| error.to_string())?;
        }
        fs::write(skill_dir.join(MANAGED_MARKER), &id).map_err(|error| error.to_string())?;
        let now = now_millis();
        let skill = json!({
            "id": id,
            "name": name,
            "enabled": true,
            "path": skill_dir.to_string_lossy(),
            "content": fs::read_to_string(&skill_file).unwrap_or_default(),
            "createdAt": now,
            "updatedAt": now,
            "installations": []
        });
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        store.skills.push(skill.clone());
        if let Err(error) = save_store(&state.store_path, &store) {
            store
                .skills
                .retain(|item| value_id(item) != Some(id.as_str()));
            return Err(error);
        }
        Ok(skill)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(skill_dir);
    }
    result
}

pub(crate) fn update_skill_files(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let updates = args.get(1).cloned().unwrap_or_else(|| json!({}));
    let (original, installations, mut updated) = {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        let original = store
            .skills
            .iter()
            .find(|skill| value_id(skill) == Some(id.as_str()))
            .cloned()
            .ok_or_else(|| format!("Skill not found: {id}"))?;
        let installations = store
            .skill_installations
            .iter()
            .filter(|installation| {
                installation.get("skillId").and_then(Value::as_str) == Some(id.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut updated = original.clone();
        merge_value_object(&mut updated, updates);
        (original, installations, updated)
    };

    let old_name = original
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("skill");
    let new_name = safe_skill_name(
        updated
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(old_name),
    )?;
    let old_path = skill_path(state, &original);
    let new_path = skills_root(state).join(&new_name);
    if old_path != new_path {
        if new_path.exists() {
            return Err(format!("Skill already exists: {new_name}"));
        }
        fs::rename(&old_path, &new_path).map_err(|error| error.to_string())?;
    }
    if let Some(content) = updated.get("content").and_then(Value::as_str) {
        fs::create_dir_all(&new_path).map_err(|error| error.to_string())?;
        fs::write(new_path.join("SKILL.md"), content).map_err(|error| error.to_string())?;
    }
    fs::write(new_path.join(MANAGED_MARKER), &id).map_err(|error| error.to_string())?;
    updated["name"] = Value::String(new_name);
    updated["path"] = Value::String(new_path.to_string_lossy().into_owned());
    updated["content"] =
        Value::String(fs::read_to_string(new_path.join("SKILL.md")).unwrap_or_default());
    updated["updatedAt"] = json!(now_millis());
    updated["installations"] = Value::Array(Vec::new());

    let mut updated_installations = Vec::new();
    for mut installation in installations {
        let result = refresh_installation_target_path(state, &updated, &mut installation)
            .and_then(|_| sync_installation(state, &updated, &mut installation));
        set_installation_result(&mut installation, result);
        installation["updatedAt"] = json!(now_millis());
        updated_installations.push(installation);
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    *find_entity_mut(&mut store.skills, &id)? = updated.clone();
    store.skill_installations.retain(|installation| {
        installation.get("skillId").and_then(Value::as_str) != Some(id.as_str())
    });
    store
        .skill_installations
        .extend(updated_installations.clone());
    save_store(&state.store_path, &store)?;
    updated["installations"] = Value::Array(updated_installations);
    Ok(updated)
}

pub(crate) fn delete_skill_files(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let (skill, installations) = {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        let skill = store
            .skills
            .iter()
            .find(|skill| value_id(skill) == Some(id.as_str()))
            .cloned()
            .ok_or_else(|| format!("Skill not found: {id}"))?;
        let installations = store
            .skill_installations
            .iter()
            .filter(|installation| {
                installation.get("skillId").and_then(Value::as_str) == Some(id.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        (skill, installations)
    };
    for installation in &installations {
        remove_installation(state, installation)?;
    }
    let path = skill_path(state, &skill);
    if managed_marker_matches(&path, &id) {
        fs::remove_dir_all(path).map_err(|error| error.to_string())?;
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    store
        .skills
        .retain(|skill| value_id(skill) != Some(id.as_str()));
    store.skill_installations.retain(|installation| {
        installation.get("skillId").and_then(Value::as_str) != Some(id.as_str())
    });
    save_store(&state.store_path, &store)?;
    Ok(Value::Bool(true))
}

pub(crate) fn set_skill_installation(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let input = args
        .first()
        .and_then(Value::as_object)
        .ok_or_else(|| "Skill installation input is required".to_string())?;
    let skill_id = input
        .get("skillId")
        .and_then(Value::as_str)
        .ok_or_else(|| "skillId is required".to_string())?;
    let agent_id = input
        .get("agentId")
        .and_then(Value::as_str)
        .ok_or_else(|| "agentId is required".to_string())?;
    let target_id = input
        .get("targetId")
        .and_then(Value::as_str)
        .ok_or_else(|| "targetId is required".to_string())?;
    let project_path = input
        .get("projectPath")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let (target, target_root) =
        resolve_skill_target(state, agent_id, target_id, project_path.as_deref())?;
    let mode = input
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or(&target.mode)
        .to_string();
    if !matches!(mode.as_str(), "copy" | "symlink" | "native") {
        return Err(format!("Unsupported skill installation mode: {mode}"));
    }
    if (target.mode == "native") != (mode == "native") {
        return Err(format!(
            "Skill target {} requires installation mode {}",
            target.id, target.mode
        ));
    }
    let installation_id = installation_id(skill_id, agent_id, target_id, project_path.as_deref());
    let (skill, previous_installation) = {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        let skill = store
            .skills
            .iter()
            .find(|skill| value_id(skill) == Some(skill_id))
            .cloned()
            .ok_or_else(|| format!("Skill not found: {skill_id}"))?;
        let previous = store
            .skill_installations
            .iter()
            .find(|installation| value_id(installation) == Some(installation_id.as_str()))
            .cloned();
        (skill, previous)
    };
    let skill_name = skill.get("name").and_then(Value::as_str).unwrap_or("skill");
    let installed_path = target_root.join(skill_name);
    let mut installation = json!({
        "id": installation_id,
        "skillId": skill_id,
        "agentId": agent_id,
        "targetId": target_id,
        "scope": target.scope,
        "projectPath": project_path.as_ref().map(|path| path.to_string_lossy().into_owned()),
        "mode": mode,
        "status": "synced",
        "installedPath": if mode == "native" { Value::Null } else { json!(installed_path.to_string_lossy()) },
        "error": null,
        "updatedAt": now_millis()
    });
    if mode == "native" {
        if let Some(previous) = previous_installation {
            installation["nativeReference"] = previous
                .get("nativeReference")
                .cloned()
                .unwrap_or(Value::Null);
            installation["installedPath"] = previous
                .get("installedPath")
                .cloned()
                .unwrap_or(Value::Null);
        }
    }
    let result = sync_installation(state, &skill, &mut installation);
    set_installation_result(&mut installation, result);
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    store.skill_installations.retain(|item| {
        item.get("id").and_then(Value::as_str) != installation.get("id").and_then(Value::as_str)
    });
    store.skill_installations.push(installation.clone());
    save_store(&state.store_path, &store)?;
    Ok(installation)
}

pub(crate) fn remove_skill_installation(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let installation = {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        store
            .skill_installations
            .iter()
            .find(|installation| value_id(installation) == Some(id.as_str()))
            .cloned()
            .ok_or_else(|| format!("Skill installation not found: {id}"))?
    };
    remove_installation(state, &installation)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    store
        .skill_installations
        .retain(|installation| value_id(installation) != Some(id.as_str()));
    save_store(&state.store_path, &store)?;
    Ok(Value::Bool(true))
}

pub(crate) fn initialize_skill_files(state: &DesktopState) {
    let (skills, installations) = state
        .store
        .lock()
        .map(|store| (store.skills.clone(), store.skill_installations.clone()))
        .unwrap_or_default();
    let mut changed = Vec::new();
    for mut installation in installations {
        let Some(skill_id) = installation.get("skillId").and_then(Value::as_str) else {
            continue;
        };
        let Some(skill) = skills
            .iter()
            .find(|skill| value_id(skill) == Some(skill_id))
        else {
            continue;
        };
        let result = sync_installation(state, skill, &mut installation);
        set_installation_result(&mut installation, result);
        installation["updatedAt"] = json!(now_millis());
        changed.push(installation);
    }
    if let Ok(mut store) = state.store.lock() {
        store.skill_installations = changed;
        let _ = save_store(&state.store_path, &store);
    }
}

pub(crate) fn list_available_skill_targets(state: &DesktopState) -> Result<Value, String> {
    serde_json::to_value(list_skill_targets(state)).map_err(|error| error.to_string())
}

fn set_installation_result(installation: &mut Value, result: Result<(), String>) {
    match result {
        Ok(()) => {
            installation["status"] = json!("synced");
            installation["error"] = Value::Null;
        }
        Err(error) => {
            let (status, message) = if let Some(message) = error.strip_prefix("CONFLICT:") {
                ("conflict", message.to_string())
            } else if error.contains("plugin is disabled") {
                ("disabled", error)
            } else if error.contains("plugin not found") || error.contains("target not found") {
                ("missing-agent", error)
            } else if error.contains("does not support") || error.contains("Unsupported") {
                ("unsupported", error)
            } else {
                ("error", error)
            };
            installation["status"] = json!(status);
            installation["error"] = json!(message);
        }
    }
}

fn sync_installation(
    state: &DesktopState,
    skill: &Value,
    installation: &mut Value,
) -> Result<(), String> {
    if installation.get("mode").and_then(Value::as_str) == Some("native") {
        let agent_id = installation
            .get("agentId")
            .and_then(Value::as_str)
            .ok_or_else(|| "Skill installation Agent is missing".to_string())?;
        let target_id = installation
            .get("targetId")
            .and_then(Value::as_str)
            .ok_or_else(|| "Skill installation target is missing".to_string())?;
        let project_path = installation
            .get("projectPath")
            .and_then(Value::as_str)
            .map(Path::new);
        let result = install_native_skill(
            state,
            agent_id,
            target_id,
            skill,
            installation,
            project_path,
        )?;
        installation["nativeReference"] = result
            .get("nativeReference")
            .cloned()
            .unwrap_or_else(|| result.clone());
        installation["installedPath"] = result.get("installedPath").cloned().unwrap_or(Value::Null);
        return Ok(());
    }
    let source = skill
        .get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "Skill source path is missing".to_string())?;
    let target = installation
        .get("installedPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "Skill installation path is missing".to_string())?;
    let skill_id = skill.get("id").and_then(Value::as_str).unwrap_or_default();
    if target.exists() || fs::symlink_metadata(&target).is_ok() {
        if !managed_marker_matches(&target, skill_id) {
            return Err(format!("CONFLICT:{}", target.display()));
        }
        remove_path(&target)?;
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    match installation
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("copy")
    {
        "symlink" => create_directory_link(&source, &target),
        "copy" => copy_directory(&source, &target),
        mode => Err(format!("Unsupported skill installation mode: {mode}")),
    }
}

fn refresh_installation_target_path(
    state: &DesktopState,
    skill: &Value,
    installation: &mut Value,
) -> Result<(), String> {
    if installation.get("mode").and_then(Value::as_str) == Some("native") {
        return Ok(());
    }
    let agent_id = installation
        .get("agentId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Skill installation Agent is missing".to_string())?;
    let target_id = installation
        .get("targetId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Skill installation target is missing".to_string())?;
    let project_path = installation
        .get("projectPath")
        .and_then(Value::as_str)
        .map(Path::new);
    let (_, target_root) = resolve_skill_target(state, agent_id, target_id, project_path)?;
    let skill_name = skill.get("name").and_then(Value::as_str).unwrap_or("skill");
    let target = target_root.join(skill_name);
    let previous = installation
        .get("installedPath")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    if previous
        .as_ref()
        .is_some_and(|previous| previous != &target)
    {
        let previous = previous.unwrap();
        let skill_id = installation
            .get("skillId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if fs::symlink_metadata(&previous).is_ok() && managed_marker_matches(&previous, skill_id) {
            remove_path(&previous)?;
        }
    }
    installation["installedPath"] = json!(target.to_string_lossy());
    Ok(())
}

fn remove_installation(state: &DesktopState, installation: &Value) -> Result<(), String> {
    if installation.get("mode").and_then(Value::as_str) == Some("native") {
        let agent_id = installation
            .get("agentId")
            .and_then(Value::as_str)
            .ok_or_else(|| "Skill installation Agent is missing".to_string())?;
        return remove_native_skill(state, agent_id, installation);
    }
    let Some(target) = installation
        .get("installedPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return Ok(());
    };
    let skill_id = installation
        .get("skillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if fs::symlink_metadata(&target).is_ok() && managed_marker_matches(&target, skill_id) {
        remove_path(&target)?;
    }
    Ok(())
}

fn installation_id(
    skill_id: &str,
    agent_id: &str,
    target_id: &str,
    project_path: Option<&Path>,
) -> String {
    use sha2::{Digest, Sha256};
    let key = format!(
        "{skill_id}\0{agent_id}\0{target_id}\0{}",
        project_path
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default()
    );
    format!("skill-install-{:x}", Sha256::digest(key.as_bytes()))
}

fn skills_root(state: &DesktopState) -> PathBuf {
    state
        .store_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("skills")
}

fn skill_path(state: &DesktopState, skill: &Value) -> PathBuf {
    skill
        .get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            skills_root(state).join(skill.get("name").and_then(Value::as_str).unwrap_or("skill"))
        })
}

fn safe_skill_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains(':')
    {
        return Err("Skill name contains invalid path characters".to_string());
    }
    Ok(value.to_string())
}

fn managed_marker_matches(path: &Path, id: &str) -> bool {
    fs::read_to_string(path.join(MANAGED_MARKER))
        .map(|value| value.trim() == id)
        .unwrap_or(false)
}

fn remove_path(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        #[cfg(windows)]
        fs::remove_dir(path).map_err(|error| error.to_string())?;
        #[cfg(not(windows))]
        fs::remove_file(path).map_err(|error| error.to_string())?;
    } else if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())?;
    } else {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn create_directory_link(source: &Path, target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(source, target).map_err(|error| {
            format!(
                "Failed to create directory symlink (enable Windows Developer Mode or choose Copy): {error}"
            )
        })
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target).map_err(|error| error.to_string())
    }
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        let output = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &output)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), output).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_names_reject_paths() {
        assert!(safe_skill_name("../skill").is_err());
        assert!(safe_skill_name("folder/skill").is_err());
        assert_eq!(safe_skill_name("my-skill").unwrap(), "my-skill");
    }

    #[test]
    fn installation_ids_are_stable_and_scope_specific() {
        let first = installation_id("skill", "codex", "project", Some(Path::new("C:/one")));
        let same = installation_id("skill", "codex", "project", Some(Path::new("C:/one")));
        let other = installation_id("skill", "codex", "project", Some(Path::new("C:/two")));
        assert_eq!(first, same);
        assert_ne!(first, other);
    }
}
