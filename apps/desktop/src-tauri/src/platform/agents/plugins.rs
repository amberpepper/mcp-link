use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use zip::ZipArchive;

use crate::state::DesktopState;

use super::{
    model::{
        AgentConfigFileDefinition, AgentInstance, AgentInstanceConfig, AgentPluginDescriptor,
        AgentSession, AgentSkillTarget, SessionOperationResult, SessionStats, SessionSummary,
    },
    util::expand_path_template,
};

#[path = "host.rs"]
mod host;
#[cfg(feature = "wasm-plugins")]
#[path = "wasm.rs"]
mod wasm;

const PLUGIN_MARKER: &str = ".mcp-link-agent-plugin";
const MAX_PLUGIN_SIZE: usize = 128 * 1024 * 1024;
const MAX_PLUGIN_FILES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExternalPluginManifest {
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) icon: Option<String>,
    #[serde(default)]
    pub(crate) capabilities: Vec<String>,
    #[serde(default)]
    pub(crate) instance_config: ManifestInstanceConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) config_files: Vec<AgentConfigFileDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) databases: Vec<PluginDatabase>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) files: Vec<PluginFileResource>,
    #[serde(default)]
    pub(crate) skill_targets: Vec<ManifestSkillTarget>,
    pub(crate) runtime: Option<PluginRuntime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestInstanceConfig {
    #[serde(default = "default_directory")]
    pub(crate) session_path_kind: String,
    #[serde(default)]
    pub(crate) home_levels_up: usize,
    pub(crate) session_path_template: Option<String>,
    pub(crate) wsl_session_path_template: Option<String>,
    pub(crate) skill_path_template: Option<String>,
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) resume_arguments: Vec<String>,
    #[serde(default)]
    pub(crate) path_hints: Vec<String>,
}

impl Default for ManifestInstanceConfig {
    fn default() -> Self {
        Self {
            session_path_kind: default_directory(),
            home_levels_up: 0,
            session_path_template: None,
            wsl_session_path_template: None,
            skill_path_template: None,
            command: None,
            resume_arguments: Vec::new(),
            path_hints: Vec::new(),
        }
    }
}

fn default_directory() -> String {
    "directory".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestSkillTarget {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) scope: String,
    pub(crate) path_template: String,
    #[serde(default = "default_copy")]
    pub(crate) mode: String,
    #[serde(default = "default_skill_format")]
    pub(crate) format: String,
    #[serde(default)]
    pub(crate) project_path_required: bool,
}

fn default_copy() -> String {
    "copy".to_string()
}

fn default_skill_format() -> String {
    "agents-skill".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PluginRuntime {
    pub(crate) kind: String,
    pub(crate) entry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginDatabase {
    pub(crate) id: String,
    pub(crate) path_template: String,
    #[serde(default = "default_database_access")]
    pub(crate) access: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginFileResource {
    pub(crate) id: String,
    pub(crate) path_template: String,
    #[serde(default = "default_database_access")]
    pub(crate) access: String,
}

fn default_database_access() -> String {
    "read-only".to_string()
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalPlugin {
    pub(crate) root: PathBuf,
    pub(crate) manifest: ExternalPluginManifest,
}

pub(crate) fn plugins_root(state: &DesktopState) -> PathBuf {
    state
        .store_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("agent-plugins")
}

pub(crate) fn load_plugins(state: &DesktopState) -> Vec<ExternalPlugin> {
    let root = plugins_root(state);
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|entry| {
            let directory_name = entry.file_name().to_string_lossy().into_owned();
            load_plugin(entry.path())
                .ok()
                .filter(|plugin| plugin.manifest.id == directory_name)
        })
        .collect()
}

fn load_plugin(root: PathBuf) -> Result<ExternalPlugin, String> {
    if !root.join(PLUGIN_MARKER).exists() {
        return Err("Plugin marker is missing".to_string());
    }
    let body = fs::read_to_string(root.join("manifest.json")).map_err(|error| error.to_string())?;
    let manifest: ExternalPluginManifest =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    validate_manifest(&manifest)?;
    validate_runtime_files(&root, &manifest)?;
    Ok(ExternalPlugin { root, manifest })
}

fn validate_manifest(manifest: &ExternalPluginManifest) -> Result<(), String> {
    if manifest.schema_version != 2 {
        return Err(format!(
            "Unsupported agent plugin schema version: {}",
            manifest.schema_version
        ));
    }
    if manifest.id.is_empty()
        || !manifest
            .id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Plugin ID contains invalid characters".to_string());
    }
    if manifest.name.trim().is_empty() || manifest.version.trim().is_empty() {
        return Err("Plugin name and version are required".to_string());
    }
    if manifest.runtime.is_none() {
        return Err("External agent plugin does not define a runtime".to_string());
    }
    if !matches!(
        manifest.instance_config.session_path_kind.as_str(),
        "directory" | "file"
    ) {
        return Err("instanceConfig.sessionPathKind must be directory or file".to_string());
    }
    let mut config_file_ids = HashSet::new();
    for config_file in &manifest.config_files {
        if config_file.id.trim().is_empty()
            || !config_file.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err("Agent config file ID contains invalid characters".to_string());
        }
        if !config_file_ids.insert(config_file.id.as_str()) {
            return Err(format!(
                "Duplicate agent config file ID: {}",
                config_file.id
            ));
        }
        if config_file.label.trim().is_empty() || config_file.path_template.trim().is_empty() {
            return Err("Agent config file label and pathTemplate are required".to_string());
        }
        if !["${ROOT}", "${HOME}", "${LOCALAPPDATA}"]
            .iter()
            .any(|placeholder| config_file.path_template.contains(placeholder))
        {
            return Err(format!(
                "Agent config pathTemplate must use a supported root placeholder: {}",
                config_file.id
            ));
        }
        if !matches!(
            config_file.language.as_str(),
            "json" | "jsonc" | "toml" | "yaml" | "markdown" | "text"
        ) {
            return Err(format!(
                "Unsupported agent config language: {}",
                config_file.language
            ));
        }
        if config_file
            .kind
            .as_deref()
            .is_some_and(|kind| !matches!(kind, "config" | "prompt"))
        {
            return Err(format!(
                "Unsupported agent config file kind: {}",
                config_file.kind.as_deref().unwrap_or_default()
            ));
        }
    }
    let mut database_ids = HashSet::new();
    for database in &manifest.databases {
        if database.id.trim().is_empty()
            || !database.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err("Agent plugin database ID contains invalid characters".to_string());
        }
        if !database_ids.insert(database.id.as_str()) {
            return Err(format!(
                "Duplicate agent plugin database ID: {}",
                database.id
            ));
        }
        if database.path_template.trim().is_empty()
            || !["${ROOT}", "${HOME}", "${LOCALAPPDATA}", "${SESSION_ROOT}"]
                .iter()
                .any(|placeholder| database.path_template.contains(placeholder))
        {
            return Err(format!(
                "Agent plugin database pathTemplate is invalid: {}",
                database.id
            ));
        }
        if !matches!(database.access.as_str(), "read-only" | "read-write") {
            return Err(format!(
                "Agent plugin database access must be read-only or read-write: {}",
                database.id
            ));
        }
    }
    let mut file_ids = HashSet::new();
    for file in &manifest.files {
        if file.id.trim().is_empty()
            || !file.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err("Agent plugin file resource ID contains invalid characters".to_string());
        }
        if !file_ids.insert(file.id.as_str()) {
            return Err(format!(
                "Duplicate agent plugin file resource ID: {}",
                file.id
            ));
        }
        if file.path_template.trim().is_empty()
            || !["${ROOT}", "${HOME}", "${LOCALAPPDATA}", "${SESSION_ROOT}"]
                .iter()
                .any(|placeholder| file.path_template.contains(placeholder))
        {
            return Err(format!(
                "Agent plugin file pathTemplate is invalid: {}",
                file.id
            ));
        }
        if !matches!(file.access.as_str(), "read-only" | "read-write") {
            return Err(format!(
                "Agent plugin file access must be read-only or read-write: {}",
                file.id
            ));
        }
    }
    let runtime = manifest
        .runtime
        .as_ref()
        .expect("runtime was checked above");
    if runtime.kind != "wasm" {
        return Err(format!(
            "Unsupported plugin runtime kind: {}. Only wasm is supported",
            runtime.kind
        ));
    }
    if !is_safe_relative_path(&runtime.entry) || !runtime.entry.ends_with(".wasm") {
        return Err("WASM plugin entry must be a safe .wasm path".to_string());
    }
    if manifest
        .icon
        .as_deref()
        .is_some_and(|icon| !is_safe_relative_path(icon))
    {
        return Err("Plugin icon must be a safe relative path".to_string());
    }
    Ok(())
}

fn validate_runtime_files(root: &Path, manifest: &ExternalPluginManifest) -> Result<(), String> {
    let runtime = manifest
        .runtime
        .as_ref()
        .ok_or_else(|| "Plugin runtime is missing".to_string())?;
    let path = root.join(&runtime.entry);
    if !path.is_file() {
        return Err(format!(
            "WASM plugin entry was not found: {}",
            path.display()
        ));
    }
    Ok(())
}

fn is_safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.trim().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

pub(crate) fn descriptor(state: &DesktopState, plugin: &ExternalPlugin) -> AgentPluginDescriptor {
    let enabled = plugin_enabled(state, &plugin.manifest.id);
    AgentPluginDescriptor {
        id: plugin.manifest.id.clone(),
        name: plugin.manifest.name.clone(),
        version: plugin.manifest.version.clone(),
        description: plugin.manifest.description.clone(),
        icon: plugin
            .manifest
            .icon
            .as_ref()
            .and_then(|icon| plugin_icon_data_url(&plugin.root.join(icon))),
        enabled,
        capabilities: plugin.manifest.capabilities.clone(),
        instance_config: AgentInstanceConfig {
            session_path_kind: plugin.manifest.instance_config.session_path_kind.clone(),
            home_levels_up: plugin.manifest.instance_config.home_levels_up,
            session_path_template: plugin
                .manifest
                .instance_config
                .session_path_template
                .as_deref()
                .map(str::to_string),
            wsl_session_path_template: plugin
                .manifest
                .instance_config
                .wsl_session_path_template
                .clone(),
            skill_path_template: plugin
                .manifest
                .instance_config
                .skill_path_template
                .as_deref()
                .or_else(|| {
                    plugin
                        .manifest
                        .skill_targets
                        .iter()
                        .find(|target| target.scope == "global" && !target.project_path_required)
                        .map(|target| target.path_template.as_str())
                })
                .map(str::to_string),
            command: plugin.manifest.instance_config.command.clone(),
            resume_arguments: plugin.manifest.instance_config.resume_arguments.clone(),
            path_hints: plugin.manifest.instance_config.path_hints.clone(),
        },
        config_files: plugin.manifest.config_files.clone(),
        instances: Vec::new(),
        session_roots: Vec::new(),
        skill_targets: plugin
            .manifest
            .skill_targets
            .iter()
            .map(|target| AgentSkillTarget {
                id: target.id.clone(),
                agent_id: plugin.manifest.id.clone(),
                label: target.label.clone(),
                scope: target.scope.clone(),
                path_template: target.path_template.clone(),
                resolved_path: if target.project_path_required {
                    None
                } else {
                    Some(
                        expand_path_template(&target.path_template, None)
                            .to_string_lossy()
                            .into_owned(),
                    )
                },
                mode: target.mode.clone(),
                format: target.format.clone(),
                project_path_required: target.project_path_required,
            })
            .collect(),
        error: None,
    }
}

fn plugin_icon_data_url(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let mime = match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

pub(crate) fn list_sessions(
    state: &DesktopState,
    plugin: &ExternalPlugin,
) -> Result<Vec<SessionSummary>, String> {
    let mut sessions = Vec::new();
    for instance in super::configured_instances(state, &plugin.manifest.id, true) {
        let mut values: Vec<SessionSummary> = serde_json::from_value(invoke_plugin(
            plugin,
            "listSessions",
            json!({ "instance": instance }),
        )?)
        .map_err(|error| error.to_string())?;
        for summary in &mut values {
            normalize_summary(&plugin.manifest.id, &instance, summary);
        }
        sessions.extend(values);
    }
    Ok(sessions)
}

pub(crate) fn load_session(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    native_id: &str,
) -> Result<AgentSession, String> {
    load_session_page(state, plugin, native_id, None)
}

pub(crate) fn load_session_page(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    native_id: &str,
    page: Option<&Value>,
) -> Result<AgentSession, String> {
    let (instance_id, native_session_id) = split_native_id(native_id)?;
    let instance = find_instance(state, &plugin.manifest.id, instance_id)?;
    let before = page
        .and_then(|value| value.get("before"))
        .and_then(Value::as_u64);
    let limit = page
        .and_then(|value| value.get("limit"))
        .and_then(Value::as_u64);
    let response = invoke_plugin(
        plugin,
        "loadSession",
        json!({
            "instance": instance,
            "nativeId": native_session_id,
            "before": before,
            "limit": limit,
        }),
    )
    .map_err(|error| format!("Agent plugin loadSession failed: {error}"))?;
    let mut session: AgentSession = serde_json::from_value(response)
        .map_err(|error| format!("Invalid Agent plugin session response: {error}"))?;
    normalize_summary(&plugin.manifest.id, &instance, &mut session.summary);
    Ok(session)
}

pub(crate) fn load_session_stats(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    native_id: &str,
) -> Result<Option<SessionStats>, String> {
    let (instance_id, native_session_id) = split_native_id(native_id)?;
    let instance = find_instance(state, &plugin.manifest.id, instance_id)?;
    let stats: Option<SessionStats> = serde_json::from_value(invoke_plugin(
        plugin,
        "loadSessionStats",
        json!({
            "instance": instance,
            "nativeId": native_session_id,
        }),
    )?)
    .map_err(|error| format!("Invalid Agent plugin session stats response: {error}"))?;
    if stats
        .as_ref()
        .is_some_and(|stats| stats.source != "reported")
    {
        return Err("Agent plugin session stats must use source=reported".to_string());
    }
    Ok(stats)
}

pub(crate) fn describe_management(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    instance_id: &str,
) -> Result<Value, String> {
    let instance = find_instance(state, &plugin.manifest.id, instance_id)?;
    invoke_plugin(
        plugin,
        "describeManagement",
        json!({ "instance": instance }),
    )
    .map_err(|error| format!("Agent plugin management description failed: {error}"))
}

pub(crate) fn load_management_section(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    instance_id: &str,
    section: &str,
) -> Result<Value, String> {
    let instance = find_instance(state, &plugin.manifest.id, instance_id)?;
    invoke_plugin(
        plugin,
        "loadManagementSection",
        json!({ "instance": instance, "section": section }),
    )
    .map_err(|error| format!("Agent plugin management section failed: {error}"))
}

pub(crate) fn mutate_management_section(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    instance_id: &str,
    mutation: &Value,
    dry_run: bool,
) -> Result<Value, String> {
    let instance = find_instance(state, &plugin.manifest.id, instance_id)?;
    invoke_plugin(
        plugin,
        "mutateManagementSection",
        json!({
            "instance": instance,
            "mutation": mutation,
            "dryRun": dry_run,
        }),
    )
    .map_err(|error| format!("Agent plugin management mutation failed: {error}"))
}

pub(crate) fn load_attachment(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    native_id: &str,
    attachment: &super::model::SessionAttachment,
) -> Result<super::model::SessionAttachmentData, String> {
    let (instance_id, native_session_id) = split_native_id(native_id)?;
    let instance = find_instance(state, &plugin.manifest.id, instance_id)?;
    serde_json::from_value(invoke_plugin(
        plugin,
        "loadAttachment",
        json!({
            "instance": instance,
            "nativeId": native_session_id,
            "attachment": attachment,
        }),
    )?)
    .map_err(|error| format!("Invalid Agent plugin attachment response: {error}"))
}

pub(crate) fn resume_command(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    session: &AgentSession,
) -> Result<String, String> {
    let instance = session_instance(state, &plugin.manifest.id, session)?;
    let native_session_id = session_native_id(session)?;
    if let Some(template) = instance.resume_command.as_deref() {
        return Ok(template
            .replace("{sessionId}", &super::util::shell_quote(native_session_id))
            .replace(
                "{cwd}",
                &super::util::shell_quote(session.summary.cwd.as_deref().unwrap_or(".")),
            ));
    }
    let value = invoke_plugin(
        plugin,
        "resumeCommand",
        json!({ "instance": instance, "nativeId": native_session_id }),
    )?;
    value
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Plugin did not return a resume command".to_string())
}

pub(crate) fn duplicate_session(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    session: &AgentSession,
    until_message: Option<usize>,
) -> Result<SessionOperationResult, String> {
    let instance = session_instance(state, &plugin.manifest.id, session)?;
    let mut result: SessionOperationResult = serde_json::from_value(invoke_plugin(
        plugin,
        "duplicateSession",
        json!({ "instance": instance, "nativeId": session_native_id(session)?, "untilMessage": until_message }),
    )?)
    .map_err(|error| error.to_string())?;
    normalize_operation(&plugin.manifest.id, &instance, &mut result);
    Ok(result)
}

pub(crate) fn delete_session(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    session: &AgentSession,
) -> Result<SessionOperationResult, String> {
    let instance = session_instance(state, &plugin.manifest.id, session)?;
    let mut result: SessionOperationResult = serde_json::from_value(invoke_plugin(
        plugin,
        "deleteSession",
        json!({ "instance": instance, "nativeId": session_native_id(session)? }),
    )?)
    .map_err(|error| error.to_string())?;
    result.native_id = Some(session.summary.native_id.clone());
    Ok(result)
}

pub(crate) fn rename_session(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    session: &AgentSession,
    title: &str,
) -> Result<SessionOperationResult, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("Session title cannot be empty".to_string());
    }
    let instance = session_instance(state, &plugin.manifest.id, session)?;
    let mut result: SessionOperationResult = serde_json::from_value(invoke_plugin(
        plugin,
        "renameSession",
        json!({ "instance": instance, "nativeId": session_native_id(session)?, "title": trimmed }),
    )?)
    .map_err(|error| error.to_string())?;
    result.native_id = Some(session.summary.native_id.clone());
    Ok(result)
}

pub(crate) fn import_session(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    target_instance_id: Option<&str>,
    session: &AgentSession,
    title: Option<&str>,
    cwd: Option<&str>,
) -> Result<SessionOperationResult, String> {
    let instance_id = target_instance_id
        .ok_or_else(|| format!("Choose a target {} instance", plugin.manifest.name))?;
    let instance = find_instance(state, &plugin.manifest.id, instance_id)?;
    let mut result: SessionOperationResult = serde_json::from_value(invoke_plugin(
        plugin,
        "importSession",
        json!({ "instance": instance, "session": session, "title": title, "cwd": cwd }),
    )?)
    .map_err(|error| error.to_string())?;
    normalize_operation(&plugin.manifest.id, &instance, &mut result);
    Ok(result)
}

pub(crate) fn export_native(
    state: &DesktopState,
    plugin: &ExternalPlugin,
    session: &AgentSession,
) -> Result<(String, Vec<u8>), String> {
    let instance = session_instance(state, &plugin.manifest.id, session)?;
    let value = invoke_plugin(
        plugin,
        "exportNative",
        json!({ "instance": instance, "nativeId": session_native_id(session)? }),
    )?;
    let file_name = value
        .get("fileName")
        .and_then(Value::as_str)
        .unwrap_or("session.bin")
        .to_string();
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let bytes = if value.get("encoding").and_then(Value::as_str) == Some("base64") {
        base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|error| error.to_string())?
    } else {
        content.as_bytes().to_vec()
    };
    Ok((file_name, bytes))
}

fn join_native_id(instance_id: &str, native_session_id: &str) -> String {
    format!("{instance_id}::{native_session_id}")
}

fn split_native_id(value: &str) -> Result<(&str, &str), String> {
    value
        .split_once("::")
        .ok_or_else(|| format!("Invalid plugin session identifier: {value}"))
}

fn find_instance(
    state: &DesktopState,
    agent_id: &str,
    instance_id: &str,
) -> Result<AgentInstance, String> {
    super::configured_instances(state, agent_id, true)
        .into_iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| format!("Agent instance not found: {agent_id}/{instance_id}"))
}

fn session_instance(
    state: &DesktopState,
    agent_id: &str,
    session: &AgentSession,
) -> Result<AgentInstance, String> {
    find_instance(
        state,
        agent_id,
        session
            .summary
            .source_instance_id
            .as_deref()
            .ok_or_else(|| "Session instance is missing".to_string())?,
    )
}

fn session_native_id(session: &AgentSession) -> Result<&str, String> {
    session
        .summary
        .native_session_id
        .as_deref()
        .ok_or_else(|| "Native session ID is missing".to_string())
}

fn normalize_summary(agent_id: &str, instance: &AgentInstance, summary: &mut SessionSummary) {
    let native_session_id = summary
        .native_session_id
        .clone()
        .unwrap_or_else(|| summary.native_id.clone());
    let native_id = join_native_id(&instance.id, &native_session_id);
    summary.id = format!("{agent_id}:{native_id}");
    summary.agent_id = agent_id.to_string();
    summary.native_id = native_id;
    summary.native_session_id = Some(native_session_id);
    summary.source_instance_id = Some(instance.id.clone());
    summary.source_label = Some(instance.label.clone());
}

fn normalize_operation(
    agent_id: &str,
    instance: &AgentInstance,
    result: &mut SessionOperationResult,
) {
    result.agent_id = agent_id.to_string();
    if let Some(native_session_id) = result.native_id.take() {
        result.native_id = Some(join_native_id(&instance.id, &native_session_id));
    }
}

pub(crate) fn install_skill(
    plugin: &ExternalPlugin,
    target_id: &str,
    skill: &Value,
    installation: &Value,
    project_path: Option<&Path>,
) -> Result<Value, String> {
    invoke_plugin(
        plugin,
        "installSkill",
        json!({
            "targetId": target_id,
            "projectPath": project_path.map(|path| path.to_string_lossy().into_owned()),
            "skill": skill,
            "installation": installation,
        }),
    )
}

pub(crate) fn remove_skill(plugin: &ExternalPlugin, installation: &Value) -> Result<(), String> {
    invoke_plugin(
        plugin,
        "removeSkill",
        json!({ "installation": installation }),
    )?;
    Ok(())
}

fn invoke_plugin(plugin: &ExternalPlugin, method: &str, params: Value) -> Result<Value, String> {
    #[cfg(feature = "wasm-plugins")]
    {
        plugin_response(wasm::invoke(plugin, method, params)?)
    }
    #[cfg(not(feature = "wasm-plugins"))]
    {
        let _ = (plugin, method, params);
        Err("This MCP Link build does not include WASM plugin support".to_string())
    }
}

fn plugin_response(response: Value) -> Result<Value, String> {
    if let Some(error) = response.get("error") {
        return Err(error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.as_str())
            .unwrap_or("Agent plugin call failed")
            .to_string());
    }
    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

fn plugin_enabled(state: &DesktopState, id: &str) -> bool {
    state
        .store
        .lock()
        .ok()
        .and_then(|store| store.settings.get("agentPluginEnabled").cloned())
        .and_then(|value| value.get(id).and_then(Value::as_bool))
        .unwrap_or(false)
}

pub(crate) fn set_plugin_enabled(
    state: &DesktopState,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    let settings = store
        .settings
        .entry("agentPluginEnabled".to_string())
        .or_insert_with(|| json!({}));
    let object = settings
        .as_object_mut()
        .ok_or_else(|| "agentPluginEnabled setting is invalid".to_string())?;
    object.insert(id.to_string(), Value::Bool(enabled));
    crate::state::save_store(&state.store_path, &store)
}

pub(crate) fn install_plugin_package(
    state: &DesktopState,
    bytes: Vec<u8>,
) -> Result<AgentPluginDescriptor, String> {
    if bytes.is_empty() || bytes.len() > MAX_PLUGIN_SIZE {
        return Err("Agent plugin package is empty or exceeds 128 MB".to_string());
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Invalid agent plugin package: {error}"))?;
    let manifest = {
        let mut file = archive
            .by_name("manifest.json")
            .map_err(|_| "Agent plugin package does not contain manifest.json".to_string())?;
        let mut body = String::new();
        file.read_to_string(&mut body)
            .map_err(|error| error.to_string())?;
        serde_json::from_str::<ExternalPluginManifest>(&body).map_err(|error| error.to_string())?
    };
    validate_manifest(&manifest)?;
    if archive.len() > MAX_PLUGIN_FILES {
        return Err(format!(
            "Agent plugin package exceeds {MAX_PLUGIN_FILES} files"
        ));
    }
    let root = plugins_root(state);
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let destination = root.join(&manifest.id);
    let staging = root.join(format!(
        ".install-{}-{}",
        manifest.id,
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let extraction = (|| {
        let mut extracted_size = 0_u64;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            extracted_size = extracted_size.saturating_add(entry.size());
            if extracted_size > MAX_PLUGIN_SIZE as u64 {
                return Err("Expanded Agent plugin package exceeds 128 MB".to_string());
            }
            let relative = entry
                .enclosed_name()
                .ok_or_else(|| format!("Unsafe path in plugin package: {}", entry.name()))?;
            let output = staging.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(&output).map_err(|error| error.to_string())?;
            } else {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                let mut file = fs::File::create(&output).map_err(|error| error.to_string())?;
                std::io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
                #[cfg(unix)]
                if let Some(mode) = entry.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&output, fs::Permissions::from_mode(mode & 0o777))
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        fs::write(staging.join(PLUGIN_MARKER), &manifest.id).map_err(|error| error.to_string())?;
        load_plugin(staging.clone())?;
        Ok(())
    })();
    if let Err(error) = extraction {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let backup = root.join(format!(
        ".backup-{}-{}",
        manifest.id,
        uuid::Uuid::new_v4().simple()
    ));
    if destination.exists() {
        let marker = fs::read_to_string(destination.join(PLUGIN_MARKER)).unwrap_or_default();
        if marker.trim() != manifest.id {
            let _ = fs::remove_dir_all(&staging);
            return Err("Refusing to replace an unverified Agent plugin directory".to_string());
        }
        fs::rename(&destination, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        if backup.exists() {
            let _ = fs::rename(&backup, &destination);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(error.to_string());
    }
    if backup.exists() {
        let _ = fs::remove_dir_all(backup);
    }
    let plugin = load_plugin(destination)?;
    set_plugin_enabled(state, &manifest.id, true)?;
    Ok(descriptor(state, &plugin))
}

pub(crate) fn remove_plugin(state: &DesktopState, id: &str) -> Result<(), String> {
    let path = plugins_root(state).join(id);
    let has_installations = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?
        .skill_installations
        .iter()
        .any(|installation| installation.get("agentId").and_then(Value::as_str) == Some(id));
    if has_installations {
        return Err(
            "Remove this Agent's Skill installations before removing the plugin".to_string(),
        );
    }
    remove_plugin_files(&path, id)
}

fn remove_plugin_files(path: &Path, id: &str) -> Result<(), String> {
    let marker = fs::read_to_string(path.join(PLUGIN_MARKER)).unwrap_or_default();
    if marker.trim() != id {
        return Err("Refusing to remove an unverified agent plugin directory".to_string());
    }
    fs::remove_dir_all(path).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
