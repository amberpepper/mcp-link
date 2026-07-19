use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentConfigFileDefinition {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) path_template: String,
    pub(crate) language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) default_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSkillTarget {
    pub(crate) id: String,
    pub(crate) agent_id: String,
    pub(crate) label: String,
    pub(crate) scope: String,
    pub(crate) path_template: String,
    pub(crate) resolved_path: Option<String>,
    pub(crate) mode: String,
    pub(crate) format: String,
    #[serde(default)]
    pub(crate) project_path_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentPluginDescriptor {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) icon: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) capabilities: Vec<String>,
    pub(crate) instance_config: AgentInstanceConfig,
    #[serde(default)]
    pub(crate) config_files: Vec<AgentConfigFileDefinition>,
    #[serde(default)]
    pub(crate) instances: Vec<AgentInstance>,
    pub(crate) session_roots: Vec<String>,
    pub(crate) skill_targets: Vec<AgentSkillTarget>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInstanceConfig {
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

fn default_directory() -> String {
    "directory".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInstance {
    pub(crate) id: String,
    pub(crate) agent_id: String,
    pub(crate) label: String,
    pub(crate) cli_root: Option<String>,
    pub(crate) session_root: Option<String>,
    pub(crate) skill_root: Option<String>,
    pub(crate) resume_command: Option<String>,
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionAttachment {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) name: Option<String>,
    pub(crate) mime_type: Option<String>,
    pub(crate) size: Option<usize>,
    #[serde(default)]
    pub(crate) reference: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionAttachmentData {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) mime_type: Option<String>,
    pub(crate) data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionMessage {
    pub(crate) id: String,
    pub(crate) role: String,
    pub(crate) kind: String,
    pub(crate) text: Option<String>,
    pub(crate) tool_name: Option<String>,
    pub(crate) tool_input: Option<Value>,
    pub(crate) tool_output: Option<Value>,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) timestamp: Option<i64>,
    pub(crate) raw_type: Option<String>,
    #[serde(default)]
    pub(crate) attachments: Vec<SessionAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserMessageNavItem {
    pub(crate) message_id: String,
    pub(crate) original_index: usize,
    pub(crate) text: String,
    pub(crate) timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionSummary {
    pub(crate) id: String,
    pub(crate) agent_id: String,
    pub(crate) native_id: String,
    pub(crate) native_session_id: Option<String>,
    pub(crate) source_instance_id: Option<String>,
    pub(crate) source_label: Option<String>,
    pub(crate) title: String,
    pub(crate) cwd: Option<String>,
    pub(crate) repository: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) created_at: Option<i64>,
    pub(crate) updated_at: Option<i64>,
    pub(crate) message_count: usize,
    pub(crate) source_ref: String,
    pub(crate) parent_native_id: Option<String>,
    #[serde(default)]
    pub(crate) active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSession {
    #[serde(flatten)]
    pub(crate) summary: SessionSummary,
    pub(crate) messages: Vec<SessionMessage>,
    #[serde(default)]
    pub(crate) message_cursor: Option<u64>,
    #[serde(default)]
    pub(crate) has_more_messages: bool,
    #[serde(default)]
    pub(crate) raw_metadata: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionStats {
    #[serde(default)]
    pub(crate) input_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) output_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) cached_input_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) cache_write_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) reasoning_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) total_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) cost: Option<f64>,
    #[serde(default)]
    pub(crate) context_window: Option<u64>,
    pub(crate) source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionOperationResult {
    pub(crate) ok: bool,
    pub(crate) agent_id: String,
    pub(crate) native_id: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) source_native_id: Option<String>,
    #[serde(default)]
    pub(crate) warnings: Vec<String>,
    pub(crate) backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionExportOptions {
    pub(crate) format: String,
    #[serde(default = "default_true")]
    pub(crate) include_reasoning: bool,
    #[serde(default = "default_true")]
    pub(crate) include_tool_calls: bool,
    #[serde(default = "default_true")]
    pub(crate) include_tool_results: bool,
    #[serde(default = "default_true")]
    pub(crate) sanitize: bool,
    pub(crate) from_message: Option<usize>,
    pub(crate) to_message: Option<usize>,
}

impl Default for SessionExportOptions {
    fn default() -> Self {
        Self {
            format: "json".to_string(),
            include_reasoning: true,
            include_tool_calls: true,
            include_tool_results: true,
            sanitize: true,
            from_message: None,
            to_message: None,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionExportResult {
    pub(crate) file_name: String,
    pub(crate) mime_type: String,
    pub(crate) content: String,
    pub(crate) encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionImportOptions {
    pub(crate) target_agent_id: String,
    pub(crate) target_instance_id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) cwd: Option<String>,
    #[serde(default)]
    pub(crate) open_after_import: bool,
}
