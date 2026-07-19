pub(crate) mod model;

mod config_files;
mod export;
mod index;
mod management;
mod plugins;
mod util;

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    state::{save_store, DesktopState},
    util::json::required_string,
};

use self::{
    model::{
        AgentInstance, AgentPluginDescriptor, AgentSession, AgentSkillTarget, SessionAttachment,
        SessionExportOptions, SessionExportResult, SessionImportOptions, SessionOperationResult,
        SessionSummary, UserMessageNavItem,
    },
    plugins::ExternalPlugin,
};

include!(concat!(env!("OUT_DIR"), "/bundled_agent_plugins.rs"));

pub(crate) use config_files::{
    list_agent_config_files, read_agent_config_file, save_agent_config_file,
};
pub(crate) use management::{
    apply_agent_management_mutation, get_agent_management_descriptor, get_agent_management_section,
};

pub(crate) fn list_agent_plugins(state: &DesktopState) -> Result<Value, String> {
    serde_json::to_value(descriptors(state)).map_err(|error| error.to_string())
}

pub(crate) fn list_session_terminals() -> Result<Value, String> {
    util::list_session_terminals()
}

fn descriptors(state: &DesktopState) -> Vec<AgentPluginDescriptor> {
    let external = plugins::load_plugins(state);
    let mut result = Vec::new();
    result.extend(
        external
            .iter()
            .map(|plugin| attach_instances(state, plugins::descriptor(state, plugin))),
    );
    result.sort_by(|left, right| left.name.cmp(&right.name));
    result
}

pub(crate) fn install_bundled_agent_plugins(state: &DesktopState) {
    const HASH_FILE: &str = ".bundled-package.sha256";
    const PLUGIN_MARKER: &str = ".mcp-link-agent-plugin";
    let root = plugins::plugins_root(state);
    let bundled_ids = BUNDLED_AGENT_PLUGINS
        .iter()
        .map(|(id, _)| *id)
        .collect::<HashSet<_>>();

    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
            let id = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let was_bundled = path.join(HASH_FILE).is_file()
                && fs::read_to_string(path.join(PLUGIN_MARKER))
                    .is_ok_and(|marker| marker.trim() == id);
            if was_bundled && !bundled_ids.contains(id.as_str()) {
                if let Err(error) = fs::remove_dir_all(&path) {
                    eprintln!("Failed to remove retired bundled Agent plugin {id}: {error}");
                }
            }
        }
    }

    for &(id, package) in BUNDLED_AGENT_PLUGINS {
        let digest = format!("{:x}", Sha256::digest(package));
        let installed_hash = fs::read_to_string(root.join(id).join(HASH_FILE)).ok();
        if installed_hash.as_deref().map(str::trim) == Some(digest.as_str()) {
            continue;
        }
        match plugins::install_plugin_package(state, package.to_vec()) {
            Ok(_) => {
                if let Err(error) = fs::write(root.join(id).join(HASH_FILE), &digest) {
                    eprintln!("Failed to record bundled Agent plugin {id} version: {error}");
                }
            }
            Err(error) => {
                eprintln!("Failed to install bundled Agent plugin {id}: {error}");
            }
        }
    }
}

fn attach_instances(
    state: &DesktopState,
    mut descriptor: AgentPluginDescriptor,
) -> AgentPluginDescriptor {
    let instances = configured_instances(state, &descriptor.id, false);
    descriptor.instances = instances.clone();
    descriptor.session_roots = instances
        .iter()
        .filter_map(|instance| instance.session_root.clone())
        .collect();
    descriptor
        .skill_targets
        .extend(instances.iter().filter_map(|instance| {
            instance.skill_root.as_ref().map(|path| AgentSkillTarget {
                id: format!("{}-instance-{}-global", descriptor.id, instance.id),
                agent_id: descriptor.id.clone(),
                label: format!("{} · Global Skills", instance.label),
                scope: "global".to_string(),
                path_template: path.clone(),
                resolved_path: Some(path.clone()),
                mode: "copy".to_string(),
                format: "agents-skill".to_string(),
                project_path_required: false,
            })
        }));
    descriptor
}

pub(crate) fn configured_instances(
    state: &DesktopState,
    agent_id: &str,
    enabled_only: bool,
) -> Vec<AgentInstance> {
    state
        .store
        .lock()
        .map(|store| {
            store
                .agent_instances
                .iter()
                .filter_map(|value| serde_json::from_value::<AgentInstance>(value.clone()).ok())
                .filter(|instance| instance.agent_id == agent_id)
                .filter(|instance| instance.cli_root.is_some())
                .filter(|instance| !enabled_only || instance.enabled)
                .map(|instance| instance)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn list_skill_targets(state: &DesktopState) -> Vec<AgentSkillTarget> {
    descriptors(state)
        .into_iter()
        .filter(|descriptor| descriptor.enabled)
        .flat_map(|descriptor| descriptor.skill_targets)
        .collect()
}

pub(crate) fn list_agent_sessions(
    state: &DesktopState,
    input: Option<&Value>,
) -> Result<Value, String> {
    let input = input.and_then(Value::as_object);
    let requested_agent = input
        .and_then(|input| input.get("agentId"))
        .and_then(Value::as_str);
    let query = input
        .and_then(|input| input.get("query"))
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let cwd = input
        .and_then(|input| input.get("cwd"))
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase());
    let limit = input
        .and_then(|input| input.get("limit"))
        .and_then(Value::as_u64)
        .unwrap_or(500)
        .clamp(1, 5000) as usize;
    let refresh = input
        .and_then(|input| input.get("refresh"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let descriptors = descriptors(state);
    let known_agents = descriptors
        .iter()
        .map(|descriptor| descriptor.id.clone())
        .collect::<Vec<_>>();
    let allowed_agents = descriptors
        .iter()
        .filter(|descriptor| {
            descriptor.enabled
                && descriptor
                    .capabilities
                    .iter()
                    .any(|item| item == "sessions.list")
        })
        .map(|descriptor| descriptor.id.clone())
        .collect::<Vec<_>>();
    if let Err(error) = index::remove_unknown_agents(state, &known_agents) {
        eprintln!("Failed to prune removed Agent session indexes: {error}");
    }
    if requested_agent
        .is_some_and(|requested| !allowed_agents.iter().any(|agent| agent == requested))
    {
        return Ok(Value::Array(Vec::new()));
    }
    if refresh {
        for descriptor in descriptors {
            if !descriptor.enabled
                || !descriptor
                    .capabilities
                    .iter()
                    .any(|item| item == "sessions.list")
            {
                continue;
            }
            if requested_agent.is_some_and(|id| id != descriptor.id) {
                continue;
            }
            let adapter_sessions =
                list_sessions_for(state, &descriptor.id).unwrap_or_else(|error| {
                    eprintln!("Failed to list {} sessions: {error}", descriptor.id);
                    Vec::new()
                });
            let native_ids = adapter_sessions
                .iter()
                .map(|summary| summary.native_id.clone())
                .collect::<Vec<_>>();
            for summary in adapter_sessions {
                if index::needs_refresh(state, &summary).unwrap_or(true) {
                    if let Err(error) = index::upsert_summary(state, &summary) {
                        eprintln!(
                            "Failed to index {} session {}: {error}",
                            summary.agent_id, summary.native_id
                        );
                    }
                }
            }
            if let Err(error) = index::remove_missing(state, &descriptor.id, &native_ids) {
                eprintln!("Failed to prune {} session index: {error}", descriptor.id);
            }
        }
    }
    let query_limit = if requested_agent.is_some() {
        limit
    } else {
        5000
    };
    let mut sessions = index::query(
        state,
        requested_agent,
        query.as_deref(),
        cwd.as_deref(),
        query_limit,
    )?;
    sessions.retain(|session| {
        allowed_agents
            .iter()
            .any(|agent| agent == &session.agent_id)
    });
    sessions.truncate(limit);
    serde_json::to_value(sessions).map_err(|error| error.to_string())
}

pub(crate) fn get_agent_session(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    require_capability(state, &agent_id, "sessions.read")?;
    let plugin = external_plugin(state, &agent_id)?;
    let default_page = json!({ "limit": 50 });
    let page = args
        .get(2)
        .filter(|value| value.is_object())
        .unwrap_or(&default_page);
    let session = plugins::load_session_page(state, &plugin, &native_id, Some(page))?;
    let _ = index::upsert_summary(state, &session.summary);
    serde_json::to_value(session).map_err(|error| error.to_string())
}

pub(crate) fn get_agent_session_stats(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    require_capability(state, &agent_id, "sessions.stats")?;
    let stats =
        plugins::load_session_stats(state, &external_plugin(state, &agent_id)?, &native_id)?;
    serde_json::to_value(stats).map_err(|error| error.to_string())
}

pub(crate) fn get_agent_session_user_messages(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    const INDEX_PAGE_SIZE: u64 = 200;
    const MAX_INDEX_PAGES: usize = 10_000;

    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    require_capability(state, &agent_id, "sessions.read")?;
    let plugin = external_plugin(state, &agent_id)?;
    let mut before = None;
    let mut pages = Vec::new();

    for _ in 0..MAX_INDEX_PAGES {
        let page_options = match before {
            Some(cursor) => json!({ "before": cursor, "limit": INDEX_PAGE_SIZE }),
            None => json!({ "limit": INDEX_PAGE_SIZE }),
        };
        let session = plugins::load_session_page(state, &plugin, &native_id, Some(&page_options))?;
        let next_cursor = session.message_cursor;
        let has_more = session.has_more_messages;
        pages.push(session.messages);
        if !has_more {
            break;
        }
        let Some(next_cursor) = next_cursor else {
            break;
        };
        if before == Some(next_cursor) {
            break;
        }
        before = Some(next_cursor);
    }

    pages.reverse();
    let mut seen = HashSet::new();
    let mut original_index = 0;
    let mut items = Vec::new();
    for page in pages {
        for message in page {
            if !seen.insert(message.id.clone()) {
                continue;
            }
            if message.role == "user" && message.kind == "text" {
                if let Some(text) = message.text.as_deref().and_then(nav_message_text) {
                    items.push(UserMessageNavItem {
                        message_id: message.id,
                        original_index,
                        text,
                        timestamp: message.timestamp,
                    });
                }
            }
            original_index += 1;
        }
    }
    serde_json::to_value(items).map_err(|error| error.to_string())
}

fn nav_message_text(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    let mut chars = normalized.chars();
    let preview = chars.by_ref().take(240).collect::<String>();
    Some(if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    })
}

pub(crate) fn get_agent_session_attachment(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    let _message_id = required_string(args, 2)?;
    require_capability(state, &agent_id, "sessions.read")?;
    let attachment_value = args
        .get(3)
        .ok_or_else(|| "Session attachment is required".to_string())?;
    let attachment = if attachment_value.is_object() {
        serde_json::from_value::<SessionAttachment>(attachment_value.clone())
            .map_err(|error| format!("Invalid session attachment: {error}"))?
    } else {
        let attachment_id = attachment_value
            .as_str()
            .ok_or_else(|| "Session attachment is invalid".to_string())?;
        let session = load_session_for(state, &agent_id, &native_id)?;
        session
            .messages
            .iter()
            .flat_map(|message| message.attachments.iter())
            .find(|attachment| attachment.id == attachment_id)
            .cloned()
            .ok_or_else(|| format!("Session attachment not found: {attachment_id}"))?
    };
    let data = plugins::load_attachment(
        state,
        &external_plugin(state, &agent_id)?,
        &native_id,
        &attachment,
    )?;
    serde_json::to_value(data).map_err(|error| error.to_string())
}

pub(crate) fn resume_agent_session(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    require_capability(state, &agent_id, "sessions.resume")?;
    let session = load_session_for(state, &agent_id, &native_id)?;
    let _ = index::upsert_summary(state, &session.summary);
    let plugin = external_plugin(state, &agent_id)?;
    let command = plugins::resume_command(state, &plugin, &session)?;
    util::launch_terminal(
        &command,
        session.summary.cwd.as_deref(),
        Some(session_terminal(state).as_str()),
    )?;
    serde_json::to_value(SessionOperationResult {
        ok: true,
        agent_id,
        native_id: Some(native_id),
        command: Some(command),
        source_native_id: None,
        warnings: Vec::new(),
        backup_path: None,
    })
    .map_err(|error| error.to_string())
}

pub(crate) fn duplicate_agent_session(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    let until_message = args
        .get(2)
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    require_capability(
        state,
        &agent_id,
        if until_message.is_some() {
            "sessions.branch"
        } else {
            "sessions.duplicate"
        },
    )?;
    let session = load_session_for(state, &agent_id, &native_id)?;
    let result = plugins::duplicate_session(
        state,
        &external_plugin(state, &agent_id)?,
        &session,
        until_message,
    )?;
    refresh_result_session_index(state, &result);
    serde_json::to_value(result).map_err(|error| error.to_string())
}

pub(crate) fn delete_agent_session(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    require_capability(state, &agent_id, "sessions.delete")?;
    let session = load_session_for(state, &agent_id, &native_id)?;
    let result = plugins::delete_session(state, &external_plugin(state, &agent_id)?, &session)?;
    index::delete_indexed_session(state, &agent_id, &native_id)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

pub(crate) fn rename_agent_session(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    let title = required_string(args, 2)?;
    require_capability(state, &agent_id, "sessions.rename")?;
    let session = load_session_for(state, &agent_id, &native_id)?;
    let result =
        plugins::rename_session(state, &external_plugin(state, &agent_id)?, &session, &title)?;
    index::rename_indexed_session(state, &agent_id, &native_id, &title)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

pub(crate) fn export_agent_session(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    serde_json::to_value(build_agent_session_export(state, args)?)
        .map_err(|error| error.to_string())
}

pub(crate) fn build_agent_session_export(
    state: &DesktopState,
    args: &[Value],
) -> Result<SessionExportResult, String> {
    let agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    let options = args
        .get(2)
        .cloned()
        .map(serde_json::from_value::<SessionExportOptions>)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    require_capability(state, &agent_id, "sessions.read")?;
    if options.format == "native" {
        require_capability(state, &agent_id, "sessions.export-native")?;
    }
    let mut session = load_session_for(state, &agent_id, &native_id)?;
    if options.format != "native" {
        hydrate_session_attachments(state, &agent_id, &mut session);
    }
    let native = if options.format == "native" {
        Some(plugins::export_native(
            state,
            &external_plugin(state, &agent_id)?,
            &session,
        )?)
    } else {
        None
    };
    export::export_session(&session, &options, native)
}

pub(crate) fn import_agent_session(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let source_agent_id = required_string(args, 0)?;
    let native_id = required_string(args, 1)?;
    let options: SessionImportOptions =
        serde_json::from_value(args.get(2).cloned().unwrap_or_else(|| json!({})))
            .map_err(|error| error.to_string())?;
    require_capability(state, &source_agent_id, "sessions.read")?;
    let mut session = load_session_for(state, &source_agent_id, &native_id)?;
    let target = options.target_agent_id.clone();
    let duplicate_in_place = target == source_agent_id
        && options
            .target_instance_id
            .as_deref()
            .is_none_or(|instance_id| {
                Some(instance_id) == session.summary.source_instance_id.as_deref()
            });
    if duplicate_in_place {
        return duplicate_agent_session(
            state,
            &[json!(source_agent_id), json!(native_id), Value::Null],
        );
    }
    hydrate_session_attachments(state, &source_agent_id, &mut session);
    require_capability(state, &target, "sessions.import")?;
    let mut result = plugins::import_session(
        state,
        &external_plugin(state, &target)?,
        options.target_instance_id.as_deref(),
        &session,
        options.title.as_deref(),
        options.cwd.as_deref(),
    )?;
    if options.open_after_import {
        if let Some(target_id) = result.native_id.as_deref() {
            let open_result = (|| {
                let target_session = load_session_for(state, &target, target_id)?;
                let command = plugins::resume_command(
                    state,
                    &external_plugin(state, &target)?,
                    &target_session,
                )?;
                util::launch_terminal(
                    &command,
                    target_session.summary.cwd.as_deref(),
                    Some(session_terminal(state).as_str()),
                )?;
                Ok::<_, String>(command)
            })();
            match open_result {
                Ok(command) => result.command = Some(command),
                Err(error) => result
                    .warnings
                    .push(format!("openAfterImportFailed::{error}")),
            }
        }
    }
    refresh_result_session_index(state, &result);
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn session_terminal(state: &DesktopState) -> String {
    state
        .store
        .lock()
        .ok()
        .and_then(|store| {
            store
                .settings
                .get("sessionTerminal")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "auto".to_string())
}

fn hydrate_session_attachments(state: &DesktopState, agent_id: &str, session: &mut AgentSession) {
    let Ok(plugin) = external_plugin(state, agent_id) else {
        return;
    };
    let native_id = session.summary.native_id.clone();
    for message_index in 0..session.messages.len() {
        for attachment_index in 0..session.messages[message_index].attachments.len() {
            let attachment = session.messages[message_index].attachments[attachment_index].clone();
            match plugins::load_attachment(state, &plugin, &native_id, &attachment) {
                Ok(data) => {
                    session.messages[message_index].attachments[attachment_index].data_url =
                        Some(data.data_url);
                }
                Err(error) => eprintln!(
                    "Failed to load {agent_id} session attachment {}: {error}",
                    attachment.id
                ),
            }
        }
    }
}

fn refresh_result_session_index(state: &DesktopState, result: &SessionOperationResult) {
    let Some(native_id) = result.native_id.as_deref() else {
        return;
    };
    match load_session_for(state, &result.agent_id, native_id) {
        Ok(session) => {
            if let Err(error) = index::upsert_summary(state, &session.summary) {
                eprintln!(
                    "Failed to index {} session {}: {error}",
                    result.agent_id, native_id
                );
            }
        }
        Err(error) => eprintln!(
            "Failed to load {} session {} for indexing: {error}",
            result.agent_id, native_id
        ),
    }
}

fn list_sessions_for(state: &DesktopState, agent_id: &str) -> Result<Vec<SessionSummary>, String> {
    plugins::list_sessions(state, &external_plugin(state, agent_id)?)
}

fn load_session_for(
    state: &DesktopState,
    agent_id: &str,
    native_id: &str,
) -> Result<AgentSession, String> {
    if !plugin_enabled(state, agent_id) {
        return Err(format!("Agent plugin is disabled: {agent_id}"));
    }
    plugins::load_session(state, &external_plugin(state, agent_id)?, native_id)
}

fn external_plugin(state: &DesktopState, id: &str) -> Result<ExternalPlugin, String> {
    let plugin = plugins::load_plugins(state)
        .into_iter()
        .find(|plugin| plugin.manifest.id == id)
        .ok_or_else(|| format!("Agent plugin not found: {id}"))?;
    Ok(plugin)
}

fn agent_id_for_instance(state: &DesktopState, instance_id: &str) -> Result<String, String> {
    descriptors(state)
        .into_iter()
        .find_map(|descriptor| {
            descriptor
                .instances
                .iter()
                .any(|instance| instance.id == instance_id)
                .then_some(descriptor.id)
        })
        .ok_or_else(|| format!("Agent instance not found: {instance_id}"))
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

fn require_capability(
    state: &DesktopState,
    agent_id: &str,
    capability: &str,
) -> Result<(), String> {
    let descriptor = descriptors(state)
        .into_iter()
        .find(|descriptor| descriptor.id == agent_id)
        .ok_or_else(|| format!("Agent plugin not found: {agent_id}"))?;
    if !descriptor.enabled {
        return Err(format!("Agent plugin is disabled: {agent_id}"));
    }
    if !descriptor
        .capabilities
        .iter()
        .any(|item| item == capability)
    {
        return Err(format!(
            "Agent plugin does not support {capability}: {agent_id}"
        ));
    }
    Ok(())
}

pub(crate) fn set_agent_plugin_enabled(
    state: &DesktopState,
    args: &[Value],
) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let enabled = args
        .get(1)
        .and_then(Value::as_bool)
        .ok_or_else(|| "Plugin enabled state is required".to_string())?;
    if enabled {
        external_plugin(state, &id)?;
    }
    plugins::set_plugin_enabled(state, &id, enabled)?;
    Ok(Value::Bool(true))
}

pub(crate) fn remove_agent_plugin(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    if !configured_instances(state, &id, false).is_empty() {
        return Err("Remove this CLI plugin's instances first".to_string());
    }
    plugins::remove_plugin(state, &id)?;
    Ok(Value::Bool(true))
}

pub(crate) fn install_agent_plugin_bytes(
    state: &DesktopState,
    bytes: Vec<u8>,
) -> Result<Value, String> {
    serde_json::to_value(plugins::install_plugin_package(state, bytes)?)
        .map_err(|error| error.to_string())
}

pub(crate) fn create_agent_instance(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let input = args
        .first()
        .and_then(Value::as_object)
        .ok_or_else(|| "Agent instance input is required".to_string())?;
    let agent_id = input
        .get("agentId")
        .and_then(Value::as_str)
        .ok_or_else(|| "agentId is required".to_string())?;
    let config_root = input
        .get("configRoot")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "CLI configuration directory is required".to_string())?;
    if configured_instances(state, agent_id, false)
        .iter()
        .filter_map(|instance| instance.cli_root.as_deref())
        .any(|existing| config_root_key(existing) == config_root_key(config_root))
    {
        return Err("This CLI configuration directory has already been added".to_string());
    }
    let instance = build_agent_instance(state, input, uuid::Uuid::new_v4().to_string())?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    store
        .agent_instances
        .push(serde_json::to_value(&instance).map_err(|error| error.to_string())?);
    save_store(&state.store_path, &store)?;
    serde_json::to_value(instance).map_err(|error| error.to_string())
}

pub(crate) fn remove_agent_instance(state: &DesktopState, args: &[Value]) -> Result<Value, String> {
    let id = required_string(args, 0)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Failed to lock desktop state".to_string())?;
    let instance: AgentInstance = store
        .agent_instances
        .iter()
        .find(|value| value.get("id").and_then(Value::as_str) == Some(id.as_str()))
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Agent instance not found: {id}"))?;
    let target_id = format!("{}-instance-{}-global", instance.agent_id, instance.id);
    if store.skill_installations.iter().any(|installation| {
        installation.get("targetId").and_then(Value::as_str) == Some(target_id.as_str())
    }) {
        return Err("Remove this instance's Skill installations first".to_string());
    }
    store
        .agent_instances
        .retain(|value| value.get("id").and_then(Value::as_str) != Some(id.as_str()));
    save_store(&state.store_path, &store)?;
    drop(store);
    index::delete_instance_sessions(state, &instance.agent_id, &id)?;
    Ok(Value::Bool(true))
}

fn build_agent_instance(
    state: &DesktopState,
    input: &serde_json::Map<String, Value>,
    id: String,
) -> Result<AgentInstance, String> {
    let agent_id = input
        .get("agentId")
        .and_then(Value::as_str)
        .ok_or_else(|| "agentId is required".to_string())?;
    let descriptor = descriptors(state)
        .into_iter()
        .find(|descriptor| descriptor.id == agent_id)
        .ok_or_else(|| format!("Agent plugin not found: {agent_id}"))?;
    let config_root = input
        .get("configRoot")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "CLI configuration directory is required".to_string())?;
    let config = &descriptor.instance_config;
    let selected = resolve_selected_config(config_root, config.home_levels_up)?;
    let session_root = resolve_instance_path(
        if selected.is_wsl {
            config
                .wsl_session_path_template
                .as_deref()
                .or(config.session_path_template.as_deref())
        } else {
            config.session_path_template.as_deref()
        },
        &selected,
    )
    .ok_or_else(|| "This CLI plugin does not define a session path".to_string())?;
    let skill_root = resolve_instance_path(config.skill_path_template.as_deref(), &selected);
    let command = config.command.as_deref().map(|command| {
        if let Some(distribution) = selected.distribution.as_deref() {
            format!(
                "wsl.exe -d {} -- {command}",
                util::shell_quote(distribution)
            )
        } else {
            command.to_string()
        }
    });
    let resume_command = command
        .as_deref()
        .and_then(|command| build_resume_command(command, &config.resume_arguments));
    Ok(AgentInstance {
        id,
        agent_id: agent_id.to_string(),
        label: format!("{} · {}", descriptor.name, selected.environment_label),
        cli_root: Some(selected.root),
        session_root: Some(session_root),
        skill_root,
        resume_command,
        enabled: true,
    })
}

struct SelectedConfig {
    root: String,
    home: String,
    data_home: String,
    is_wsl: bool,
    distribution: Option<String>,
    environment_label: String,
}

fn resolve_selected_config(value: &str, home_levels_up: usize) -> Result<SelectedConfig, String> {
    let value = value.trim();
    let wsl_path = value.replace('/', "\\");
    let wsl_prefix = ["\\\\wsl.localhost\\", "\\\\wsl$\\"]
        .into_iter()
        .find(|prefix| wsl_path.to_ascii_lowercase().starts_with(prefix));
    let root = if wsl_prefix.is_some() {
        wsl_path
    } else {
        value.to_string()
    };
    if !Path::new(&root).is_dir() {
        return Err(format!(
            "CLI configuration directory was not found: {value}"
        ));
    }

    let mut home_path = PathBuf::from(&root);
    for _ in 0..home_levels_up {
        home_path = home_path
            .parent()
            .ok_or_else(|| format!("Unable to derive the home directory from: {value}"))?
            .to_path_buf();
    }
    let home = home_path.to_string_lossy().into_owned();

    if let Some(prefix) = wsl_prefix {
        let relative = &root[prefix.len()..];
        let (distribution, config_relative) = relative
            .split_once('\\')
            .ok_or_else(|| "Invalid WSL configuration directory".to_string())?;
        if distribution.trim().is_empty() || config_relative.trim().is_empty() {
            return Err("Invalid WSL configuration directory".to_string());
        }
        let distribution = distribution.to_string();
        return Ok(SelectedConfig {
            root,
            data_home: format!("{}\\.local\\share", home),
            home,
            is_wsl: true,
            distribution: Some(distribution.clone()),
            environment_label: distribution,
        });
    }

    let data_home = if cfg!(windows) {
        PathBuf::from(&home).join("AppData").join("Local")
    } else {
        PathBuf::from(&home).join(".local").join("share")
    }
    .to_string_lossy()
    .into_owned();
    Ok(SelectedConfig {
        root,
        home,
        data_home,
        is_wsl: false,
        distribution: None,
        environment_label: if cfg!(windows) {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        }
        .to_string(),
    })
}

fn resolve_instance_path(template: Option<&str>, selected: &SelectedConfig) -> Option<String> {
    template.map(|template| {
        let value = template
            .replace("${ROOT}", &selected.root)
            .replace("${HOME}", &selected.home)
            .replace("${LOCALAPPDATA}", &selected.data_home);
        if selected.is_wsl {
            value.replace('/', "\\")
        } else {
            value
        }
    })
}

fn config_root_key(value: &str) -> String {
    value
        .trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

fn build_resume_command(command: &str, arguments: &[String]) -> Option<String> {
    if arguments.is_empty() {
        return None;
    }
    let arguments = arguments
        .iter()
        .map(|argument| match argument.as_str() {
            "{sessionId}" | "{cwd}" => argument.clone(),
            _ => util::shell_quote(argument),
        })
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!("{command} {arguments}"))
}

pub(crate) fn resolve_skill_target(
    state: &DesktopState,
    agent_id: &str,
    target_id: &str,
    project_path: Option<&Path>,
) -> Result<(AgentSkillTarget, PathBuf), String> {
    let target = descriptors(state)
        .into_iter()
        .flat_map(|descriptor| descriptor.skill_targets)
        .find(|target| target.agent_id == agent_id && target.id == target_id)
        .ok_or_else(|| format!("Skill target not found: {agent_id}/{target_id}"))?;
    if target.project_path_required && project_path.is_none() {
        return Err("This skill target requires a project path".to_string());
    }
    let path = util::expand_path_template(&target.path_template, project_path);
    Ok((target, path))
}

pub(crate) fn install_native_skill(
    state: &DesktopState,
    agent_id: &str,
    target_id: &str,
    skill: &Value,
    installation: &Value,
    project_path: Option<&Path>,
) -> Result<Value, String> {
    if !plugin_enabled(state, agent_id) {
        return Err(format!("Agent plugin is disabled: {agent_id}"));
    }
    plugins::install_skill(
        &external_plugin(state, agent_id)?,
        target_id,
        skill,
        installation,
        project_path,
    )
}

pub(crate) fn remove_native_skill(
    state: &DesktopState,
    agent_id: &str,
    installation: &Value,
) -> Result<(), String> {
    if !plugin_enabled(state, agent_id) {
        return Err(format!("Agent plugin is disabled: {agent_id}"));
    }
    plugins::remove_skill(&external_plugin(state, agent_id)?, installation)
}

#[cfg(test)]
mod bundled_plugin_tests {
    use super::*;

    #[test]
    fn removes_retired_bundled_plugins_without_touching_user_plugins() {
        let root = std::env::temp_dir().join(format!(
            "mcp-link-bundled-agent-test-{}",
            uuid::Uuid::new_v4()
        ));
        let state = DesktopState::load(root.join("mcp.db"));
        let plugins_root = plugins::plugins_root(&state);
        let retired = plugins_root.join("retired-test-plugin");
        let user = plugins_root.join("user-test-plugin");
        fs::create_dir_all(&retired).unwrap();
        fs::create_dir_all(&user).unwrap();
        fs::write(
            retired.join(".mcp-link-agent-plugin"),
            "retired-test-plugin",
        )
        .unwrap();
        fs::write(retired.join(".bundled-package.sha256"), "old-hash").unwrap();
        fs::write(user.join(".mcp-link-agent-plugin"), "user-test-plugin").unwrap();

        install_bundled_agent_plugins(&state);

        assert!(!retired.exists());
        assert!(user.exists());
        let _ = fs::remove_dir_all(root);
    }
}
