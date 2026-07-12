use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    state::{find_entity_mut, save_store, DesktopState},
    util::{
        json::{merge_value_object, required_string, value_id},
        time::now_millis,
    },
};

const MANAGED_MARKER: &str = ".mcp-link-managed";

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
            "updatedAt": now
        });
        sync_skill(state, &skill)?;
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
        cleanup_created_skill(state, &name, &id, &skill_dir);
    }
    result
}

pub(crate) fn update_skill_files(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let updates = args.get(1).cloned().unwrap_or_else(|| json!({}));
    let (original, mut updated) = {
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
        let mut updated = original.clone();
        merge_value_object(&mut updated, updates);
        (original, updated)
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
        remove_synced_skill(state, &original)?;
        fs::rename(&old_path, &new_path).map_err(|error| error.to_string())?;
    }
    if let Some(content) = updated.get("content").and_then(Value::as_str) {
        fs::create_dir_all(&new_path).map_err(|error| error.to_string())?;
        fs::write(new_path.join("SKILL.md"), content).map_err(|error| error.to_string())?;
    }
    if !new_path.join(MANAGED_MARKER).exists() {
        fs::write(new_path.join(MANAGED_MARKER), &id).map_err(|error| error.to_string())?;
    }
    updated["name"] = Value::String(new_name);
    updated["path"] = Value::String(new_path.to_string_lossy().into_owned());
    updated["content"] =
        Value::String(fs::read_to_string(new_path.join("SKILL.md")).unwrap_or_default());
    updated["updatedAt"] = json!(now_millis());
    if updated.get("enabled").and_then(Value::as_bool) == Some(true) {
        sync_skill(state, &updated)?;
    } else {
        remove_synced_skill(state, &updated)?;
    }

    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    *find_entity_mut(&mut store.skills, &id)? = updated.clone();
    save_store(&state.store_path, &store)?;
    Ok(updated)
}

pub(crate) fn delete_skill_files(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let skill = {
        let store = state
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        store
            .skills
            .iter()
            .find(|skill| value_id(skill) == Some(id.as_str()))
            .cloned()
            .ok_or_else(|| format!("Skill not found: {id}"))?
    };
    remove_synced_skill(state, &skill)?;
    let path = skill_path(state, &skill);
    if path.join(MANAGED_MARKER).exists() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())?;
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    store
        .skills
        .retain(|skill| value_id(skill) != Some(id.as_str()));
    save_store(&state.store_path, &store)?;
    Ok(Value::Bool(true))
}

pub(crate) fn initialize_skill_files(state: &DesktopState) {
    let skills = state
        .store
        .lock()
        .map(|store| store.skills.clone())
        .unwrap_or_default();
    for skill in skills {
        let Some(id) = value_id(&skill).map(str::to_string) else {
            continue;
        };
        let updates = json!({
            "name": skill.get("name").cloned().unwrap_or_else(|| json!("skill")),
            "enabled": skill.get("enabled").cloned().unwrap_or_else(|| json!(true)),
            "content": skill.get("content").cloned().unwrap_or_else(|| json!(""))
        });
        if let Err(error) = update_skill_files(state, &[Value::String(id.clone()), updates]) {
            eprintln!("Failed to initialize skill {id}: {error}");
        }
    }
}

pub(crate) fn reconfigure_skill_targets(state: &DesktopState, previous: Option<&Value>) {
    let previous_roots = agent_skill_roots_from_setting(previous);
    let skills = state
        .store
        .lock()
        .map(|store| store.skills.clone())
        .unwrap_or_default();
    for skill in &skills {
        let name = skill.get("name").and_then(Value::as_str).unwrap_or("skill");
        for root in &previous_roots {
            let target = root.join(name);
            if target.join(MANAGED_MARKER).exists() {
                if let Err(error) = fs::remove_dir_all(&target) {
                    eprintln!(
                        "Failed to remove old managed skill {}: {error}",
                        target.display()
                    );
                }
            }
        }
    }
    initialize_skill_files(state);
}

fn sync_skill(state: &DesktopState, skill: &Value) -> Result<(), String> {
    let source = skill_path(state, skill);
    let name = skill.get("name").and_then(Value::as_str).unwrap_or("skill");
    for root in agent_skill_roots(state) {
        let target = root.join(name);
        if target.exists() {
            if !target.join(MANAGED_MARKER).exists() {
                return Err(format!(
                    "Refusing to overwrite unmanaged skill: {}",
                    target.display()
                ));
            }
            fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
        }
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        copy_directory(&source, &target)?;
    }
    Ok(())
}

fn remove_synced_skill(state: &DesktopState, skill: &Value) -> Result<(), String> {
    let name = skill.get("name").and_then(Value::as_str).unwrap_or("skill");
    for root in agent_skill_roots(state) {
        let target = root.join(name);
        if target.join(MANAGED_MARKER).exists() {
            fs::remove_dir_all(target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn cleanup_created_skill(state: &DesktopState, name: &str, id: &str, skill_dir: &Path) {
    for root in agent_skill_roots(state) {
        let target = root.join(name);
        let marker_id = fs::read_to_string(target.join(MANAGED_MARKER)).unwrap_or_default();
        if marker_id.trim() == id {
            let _ = fs::remove_dir_all(target);
        }
    }
    let _ = fs::remove_dir_all(skill_dir);
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

fn agent_skill_roots(state: &DesktopState) -> Vec<PathBuf> {
    let configured = state
        .store
        .lock()
        .ok()
        .and_then(|store| store.settings.get("skillAgentPaths").cloned())
        .unwrap_or(Value::Null);
    agent_skill_roots_from_setting(Some(&configured))
}

fn agent_skill_roots_from_setting(value: Option<&Value>) -> Vec<PathBuf> {
    let configured = value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(PathBuf::from))
        .filter(|path| !path.as_os_str().is_empty())
        .collect::<Vec<_>>();
    if !configured.is_empty() {
        return configured;
    }
    let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) else {
        return Vec::new();
    };
    let home = PathBuf::from(home);
    vec![
        home.join(".codex").join("skills"),
        home.join(".claude").join("skills"),
    ]
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
}
