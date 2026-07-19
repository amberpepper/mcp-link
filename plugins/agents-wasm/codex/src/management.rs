use mcp_link_agent_wasm_sdk::{management_section, management_section_descriptor, Host};
use serde_json::{json, Map, Value};
use toml_edit::{value, Array, DocumentMut, Item, Table};

use super::providers::{mutate as mutate_provider, providers, section_data as providers_section};

const RESOURCE: &str = "config";
const PATH: &str = "";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params.get("instance").ok_or("Codex instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "codex",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", true),
            form_section("general", "常规", "General", "Codex 的界面与日常行为。", "Interface and everyday behavior."),
            section("mcp", false),
            section("skills", false), section("prompts", false), section("providers", false),
            form_section("models", "模型与推理", "Models & Reasoning", "模型、推理强度与输出风格。", "Models, reasoning, and response style."),
            section("permissions", false),
            form_section("execution", "执行与安全", "Execution & Security", "审批、沙箱、网络和 Shell 环境。", "Approvals, sandboxing, network, and shell environment."),
            form_section("features", "功能开关", "Feature Flags", "Codex 可选功能与实验性能力。", "Optional and experimental Codex capabilities."),
            section("environment", true), section("raw-config", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let section_id = params
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Codex management section is required")?;
    let document = Host::config_read(RESOURCE, PATH)?;
    let config = parse(&document.content)?;
    let data = section_data(section_id, params, &config)?;
    Ok(management_section(section_id, &document.revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("Codex management mutation is required")?;
    let section_id = mutation
        .get("section")
        .and_then(Value::as_str)
        .ok_or("Codex mutation section is required")?;
    let action = mutation
        .get("action")
        .and_then(Value::as_str)
        .ok_or("Codex mutation action is required")?;
    let expected = mutation
        .get("expectedRevision")
        .and_then(Value::as_str)
        .ok_or("Codex expectedRevision is required")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let source = Host::config_read(RESOURCE, PATH)?;
    if source.revision != expected {
        return Err(format!(
            "CONFIG_CONFLICT: configuration changed on disk (expected {expected}, found {})",
            source.revision
        ));
    }
    let mut config = parse(&source.content)?;
    match section_id {
        "mcp" => mutate_mcp(&mut config, action, mutation)?,
        "providers" => mutate_provider(&mut config, action, mutation)?,
        "general" => mutate_general(&mut config, mutation)?,
        "models" => mutate_models(&mut config, mutation)?,
        "permissions" => mutate_permissions(&mut config, mutation)?,
        "execution" => mutate_execution(&mut config, mutation)?,
        "features" => mutate_features(&mut config, mutation)?,
        _ => return Err(format!("Codex section is read-only: {section_id}")),
    }
    let content = config.to_string();
    let revision = if dry_run {
        source.revision
    } else {
        Host::config_write_atomic(RESOURCE, PATH, &content, expected)?
            .get("revision")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or("Host file.writeAtomic returned no revision")?
    };
    Ok(json!({
        "section": section_id,
        "revision": revision,
        "changed": content != source.content,
        "changedResources": ["config.toml"],
        "restartRequired": false,
        "warnings": [],
    }))
}

fn parse(content: &str) -> Result<DocumentMut, String> {
    if content.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        content
            .parse::<DocumentMut>()
            .map_err(|error| format!("Invalid Codex TOML configuration: {error}"))
    }
}

fn section(id: &str, read_only: bool) -> Value {
    let source = if matches!(id, "skills" | "prompts" | "raw-config") {
        "host"
    } else {
        "plugin"
    };
    management_section_descriptor(id, id, source, read_only)
}

fn form_section(
    id: &str,
    label_zh: &str,
    label_en: &str,
    description_zh: &str,
    description_en: &str,
) -> Value {
    let mut section = management_section_descriptor(id, "form", "plugin", false);
    section["label"] = json!({ "zh": label_zh, "en": label_en });
    section["description"] = json!({ "zh": description_zh, "en": description_en });
    section
}

fn section_data(section_id: &str, params: &Value, config: &DocumentMut) -> Result<Value, String> {
    match section_id {
        "overview" => Ok(overview(params, config)),
        "general" => Ok(general(config)),
        "mcp" => Ok(json!({ "servers": mcp_servers(config) })),
        "providers" => Ok(providers_section(config)),
        "models" => Ok(models(config)),
        "permissions" => Ok(permissions(config)),
        "execution" => Ok(execution(config)),
        "features" => Ok(features(config)),
        "environment" => Ok(environment(params, config)),
        _ => Err(format!(
            "Unsupported Codex management section: {section_id}"
        )),
    }
}

fn root_str<'a>(config: &'a DocumentMut, key: &str) -> Option<&'a str> {
    config.get(key).and_then(Item::as_str)
}

fn root_bool(config: &DocumentMut, key: &str) -> Option<bool> {
    config.get(key).and_then(Item::as_bool)
}

fn localized(zh: &str, en: &str) -> Value {
    json!({ "zh": zh, "en": en })
}

fn text_field(key: &str, zh: &str, en: &str) -> Value {
    json!({ "key": key, "control": "text", "label": localized(zh, en) })
}

fn select_field(key: &str, zh: &str, en: &str, options: &[&str]) -> Value {
    json!({
        "key": key,
        "control": "select",
        "label": localized(zh, en),
        "options": options.iter().map(|value| json!({ "value": value })).collect::<Vec<_>>()
    })
}

fn switch_field(key: &str, zh: &str, en: &str) -> Value {
    json!({ "key": key, "control": "switch", "label": localized(zh, en) })
}

fn string_array_field(key: &str, zh: &str, en: &str) -> Value {
    json!({
        "key": key,
        "control": "textarea",
        "valueType": "string-array",
        "mono": true,
        "rows": 4,
        "label": localized(zh, en),
        "description": localized("每行一个值。", "One value per line.")
    })
}

fn form_data(groups: Vec<Value>, values: Value) -> Value {
    json!({ "schemaVersion": 1, "groups": groups, "values": values })
}

fn overview(params: &Value, config: &DocumentMut) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "Codex",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "defaultModel": root_str(config, "model"),
        "defaultProvider": root_str(config, "model_provider"),
        "mcpServerCount": mcp_servers(config).len(),
        "providerCount": providers(config).len(),
        "skillTargetCount": 1,
        "warnings": []
    })
}

fn general(config: &DocumentMut) -> Value {
    form_data(
        vec![json!({
            "id": "general",
            "columns": 2,
            "fields": [
                select_field("file_opener", "文件打开方式", "File opener", &["vscode", "vscode-insiders", "windsurf", "cursor", "none"]),
                switch_field("check_for_update_on_startup", "启动时检查更新", "Check for updates on startup"),
                switch_field("hide_agent_reasoning", "隐藏推理事件", "Hide reasoning events"),
                switch_field("show_raw_agent_reasoning", "显示原始推理内容", "Show raw reasoning content"),
                switch_field("disable_paste_burst", "关闭突发粘贴检测", "Disable paste burst detection")
            ]
        })],
        json!({
            "file_opener": root_str(config, "file_opener").unwrap_or("vscode"),
            "check_for_update_on_startup": root_bool(config, "check_for_update_on_startup").unwrap_or(true),
            "hide_agent_reasoning": root_bool(config, "hide_agent_reasoning").unwrap_or(false),
            "show_raw_agent_reasoning": root_bool(config, "show_raw_agent_reasoning").unwrap_or(false),
            "disable_paste_burst": root_bool(config, "disable_paste_burst").unwrap_or(false)
        }),
    )
}

fn mcp_servers(config: &DocumentMut) -> Vec<Value> {
    config
        .get("mcp_servers")
        .and_then(Item::as_table_like)
        .into_iter()
        .flat_map(|servers| servers.iter())
        .filter_map(|(id, item)| {
            let server = item.as_table_like()?;
            let url = server.get("url").and_then(Item::as_str);
            let command = server.get("command").and_then(Item::as_str);
            let args = server
                .get("args")
                .and_then(Item::as_array)
                .map(|items| items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>())
                .unwrap_or_default();
            Some(json!({
                "id": id,
                "name": id,
                "transport": if url.is_some() { "http" } else { "stdio" },
                "command": command,
                "args": args,
                "url": url,
                "env": table_to_json(server.get("env")),
                "headers": table_to_json(server.get("http_headers")),
                "enabled": !server.get("enabled").and_then(Item::as_bool).is_some_and(|enabled| !enabled),
                "scope": "global"
            }))
        })
        .collect()
}

fn models(config: &DocumentMut) -> Value {
    form_data(
        vec![
            json!({
                "id": "selection",
                "title": localized("模型选择", "Model selection"),
                "columns": 2,
                "fields": [
                    text_field("model", "默认模型", "Default model"),
                    text_field("model_provider", "默认提供商", "Default provider"),
                    text_field("review_model", "Review 模型", "Review model"),
                    text_field("service_tier", "服务层级", "Service tier")
                ]
            }),
            json!({
                "id": "reasoning",
                "title": localized("推理与输出", "Reasoning & output"),
                "columns": 2,
                "fields": [
                    select_field("model_reasoning_effort", "推理强度", "Reasoning effort", &["none", "minimal", "low", "medium", "high", "xhigh"]),
                    select_field("plan_mode_reasoning_effort", "计划模式推理强度", "Plan mode reasoning effort", &["none", "minimal", "low", "medium", "high", "xhigh"]),
                    select_field("model_reasoning_summary", "推理摘要", "Reasoning summary", &["auto", "concise", "detailed", "none"]),
                    select_field("model_verbosity", "输出详细度", "Output verbosity", &["low", "medium", "high"]),
                    select_field("personality", "沟通风格", "Personality", &["none", "friendly", "pragmatic"])
                ]
            }),
        ],
        json!({
            "model": root_str(config, "model"),
            "model_provider": root_str(config, "model_provider"),
            "review_model": root_str(config, "review_model"),
            "service_tier": root_str(config, "service_tier"),
            "model_reasoning_effort": root_str(config, "model_reasoning_effort"),
            "plan_mode_reasoning_effort": root_str(config, "plan_mode_reasoning_effort"),
            "model_reasoning_summary": root_str(config, "model_reasoning_summary"),
            "model_verbosity": root_str(config, "model_verbosity"),
            "personality": root_str(config, "personality")
        }),
    )
}

fn permissions(config: &DocumentMut) -> Value {
    let rules = config
        .get("projects")
        .and_then(Item::as_table_like)
        .into_iter()
        .flat_map(|items| items.iter())
        .filter_map(|(path, item)| {
            let project = item.as_table_like()?;
            let trust = project.get("trust_level").and_then(Item::as_str)?;
            Some(json!({
                "id": path,
                "decision": if trust == "trusted" { "allow" } else { "ask" },
                "target": path,
                "kind": "project"
            }))
        })
        .collect::<Vec<_>>();
    json!({
        "approvalMode": root_str(config, "approval_policy"),
        "sandboxMode": root_str(config, "sandbox_mode"),
        "rules": rules
    })
}

fn execution(config: &DocumentMut) -> Value {
    form_data(
        vec![
            json!({
                "id": "policy",
                "title": localized("审批与沙箱", "Approval & sandbox"),
                "columns": 2,
                "fields": [
                    select_field("approval_policy", "审批策略", "Approval policy", &["untrusted", "on-request", "never"]),
                    select_field("sandbox_mode", "沙箱模式", "Sandbox mode", &["read-only", "workspace-write", "danger-full-access"]),
                    select_field("web_search", "Web Search", "Web search", &["disabled", "cached", "indexed", "live"]),
                    switch_field("allow_login_shell", "允许 Login Shell", "Allow login shell")
                ]
            }),
            json!({
                "id": "workspace-write",
                "title": localized("Workspace Write", "Workspace write"),
                "columns": 2,
                "fields": [
                    string_array_field("sandbox_workspace_write.writable_roots", "额外可写目录", "Additional writable roots"),
                    switch_field("sandbox_workspace_write.network_access", "允许网络访问", "Allow network access"),
                    switch_field("sandbox_workspace_write.exclude_tmpdir_env_var", "排除 $TMPDIR", "Exclude $TMPDIR"),
                    switch_field("sandbox_workspace_write.exclude_slash_tmp", "排除 /tmp", "Exclude /tmp")
                ]
            }),
            json!({
                "id": "shell-environment",
                "title": localized("Shell 环境", "Shell environment"),
                "columns": 2,
                "fields": [
                    select_field("shell_environment_policy.inherit", "继承范围", "Inheritance", &["all", "core", "none"]),
                    switch_field("shell_environment_policy.ignore_default_excludes", "忽略默认敏感变量排除", "Ignore default secret exclusions"),
                    string_array_field("shell_environment_policy.exclude", "排除变量模式", "Excluded variable patterns"),
                    string_array_field("shell_environment_policy.include_only", "仅包含变量", "Included variables only")
                ]
            }),
        ],
        json!({
            "approval_policy": root_str(config, "approval_policy"),
            "sandbox_mode": root_str(config, "sandbox_mode"),
            "web_search": root_str(config, "web_search").unwrap_or("cached"),
            "allow_login_shell": root_bool(config, "allow_login_shell").unwrap_or(true),
            "sandbox_workspace_write.writable_roots": table_string_array(config, "sandbox_workspace_write", "writable_roots"),
            "sandbox_workspace_write.network_access": table_bool(config, "sandbox_workspace_write", "network_access").unwrap_or(false),
            "sandbox_workspace_write.exclude_tmpdir_env_var": table_bool(config, "sandbox_workspace_write", "exclude_tmpdir_env_var").unwrap_or(false),
            "sandbox_workspace_write.exclude_slash_tmp": table_bool(config, "sandbox_workspace_write", "exclude_slash_tmp").unwrap_or(false),
            "shell_environment_policy.inherit": table_str(config, "shell_environment_policy", "inherit").unwrap_or("all"),
            "shell_environment_policy.ignore_default_excludes": table_bool(config, "shell_environment_policy", "ignore_default_excludes").unwrap_or(false),
            "shell_environment_policy.exclude": table_string_array(config, "shell_environment_policy", "exclude"),
            "shell_environment_policy.include_only": table_string_array(config, "shell_environment_policy", "include_only")
        }),
    )
}

const FEATURE_FLAGS: [(&str, bool, &str); 11] = [
    ("apps", true, "stable"),
    ("goals", true, "stable"),
    ("hooks", true, "stable"),
    ("fast_mode", true, "stable"),
    ("memories", false, "experimental"),
    ("multi_agent", true, "stable"),
    ("personality", true, "stable"),
    ("remote_plugin", true, "stable"),
    ("shell_snapshot", true, "stable"),
    ("shell_tool", true, "stable"),
    ("unified_exec", true, "stable"),
];

fn features(config: &DocumentMut) -> Value {
    let table = config.get("features").and_then(Item::as_table_like);
    let fields = FEATURE_FLAGS
        .iter()
        .map(|(key, default, maturity)| {
            let description = if *maturity == "experimental" {
                localized("实验性功能", "Experimental feature")
            } else {
                localized("稳定功能", "Stable feature")
            };
            let mut field = switch_field(&format!("features.{key}"), key, key);
            field["description"] = description;
            field["defaultValue"] = json!(*default);
            field
        })
        .collect::<Vec<_>>();
    let values = FEATURE_FLAGS
        .iter()
        .filter_map(|(key, _, _)| {
            table
                .and_then(|value| value.get(key))
                .and_then(Item::as_bool)
                .map(|configured| (format!("features.{key}"), json!(configured)))
        })
        .collect::<Map<_, _>>();
    form_data(
        vec![json!({ "id": "features", "columns": 2, "fields": fields })],
        Value::Object(values),
    )
}

fn environment(params: &Value, config: &DocumentMut) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "configFiles": [{ "id": "config", "label": "config.toml", "path": "~/.codex/config.toml", "exists": !config.is_empty() }],
        "variables": [],
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut DocumentMut, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "MCP server")?;
    let servers = ensure_table(config, "mcp_servers")?;
    if action == "remove" {
        servers.remove(id);
        return Ok(());
    }
    let payload = mutation
        .get("payload")
        .ok_or("MCP server payload is required")?;
    let previous = servers
        .remove(id)
        .unwrap_or_else(|| Item::Table(Table::new()));
    let mut table = previous.into_table().unwrap_or_else(|_| Table::new());
    let transport = payload
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    if transport == "stdio" {
        set_string(&mut table, "command", payload.get("command"));
        table.remove("url");
        let mut args = Array::new();
        for argument in payload
            .get("args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            args.push(argument);
        }
        table["args"] = value(args);
    } else {
        set_string(&mut table, "url", payload.get("url"));
        table.remove("command");
        table.remove("args");
        if let Some(headers) = payload.get("headers").and_then(Value::as_object) {
            let mut header_table = Table::new();
            for (key, header_value) in headers
                .iter()
                .filter_map(|(key, value)| value.as_str().map(|value| (key.as_str(), value)))
            {
                header_table.insert(key, value(header_value));
            }
            table["http_headers"] = Item::Table(header_table);
        }
    }
    table["enabled"] = value(
        payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    );
    servers.insert(id, Item::Table(table));
    Ok(())
}

fn mutate_general(config: &mut DocumentMut, mutation: &Value) -> Result<(), String> {
    let payload = form_values(mutation)?;
    validate_enum(
        payload,
        "file_opener",
        &["vscode", "vscode-insiders", "windsurf", "cursor", "none"],
    )?;
    set_root_string(config, "file_opener", payload.get("file_opener"));
    for key in [
        "check_for_update_on_startup",
        "hide_agent_reasoning",
        "show_raw_agent_reasoning",
        "disable_paste_burst",
    ] {
        set_root_bool(config, key, payload.get(key));
    }
    Ok(())
}

fn mutate_models(config: &mut DocumentMut, mutation: &Value) -> Result<(), String> {
    let payload = form_values(mutation)?;
    validate_enum(
        payload,
        "model_reasoning_effort",
        &["none", "minimal", "low", "medium", "high", "xhigh"],
    )?;
    validate_enum(
        payload,
        "plan_mode_reasoning_effort",
        &["none", "minimal", "low", "medium", "high", "xhigh"],
    )?;
    validate_enum(
        payload,
        "model_reasoning_summary",
        &["auto", "concise", "detailed", "none"],
    )?;
    validate_enum(payload, "model_verbosity", &["low", "medium", "high"])?;
    validate_enum(payload, "personality", &["none", "friendly", "pragmatic"])?;
    for key in [
        "model",
        "model_provider",
        "review_model",
        "model_reasoning_effort",
        "plan_mode_reasoning_effort",
        "model_reasoning_summary",
        "model_verbosity",
        "personality",
        "service_tier",
    ] {
        set_root_string(config, key, payload.get(key));
    }
    Ok(())
}

fn mutate_permissions(config: &mut DocumentMut, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("Permission settings payload is required")?;
    validate_enum(
        payload,
        "approvalMode",
        &["untrusted", "on-request", "never"],
    )?;
    validate_enum(
        payload,
        "sandboxMode",
        &["read-only", "workspace-write", "danger-full-access"],
    )?;
    set_root_string(config, "approval_policy", payload.get("approvalMode"));
    set_root_string(config, "sandbox_mode", payload.get("sandboxMode"));
    let projects = ensure_table(config, "projects")?;
    for rule in payload
        .get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if rule.get("kind").and_then(Value::as_str) != Some("project") {
            continue;
        }
        let path = rule
            .get("target")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .ok_or("Project rule path is required")?;
        let previous = projects
            .remove(path)
            .unwrap_or_else(|| Item::Table(Table::new()));
        let mut table = previous.into_table().unwrap_or_else(|_| Table::new());
        table["trust_level"] = value(
            if rule.get("decision").and_then(Value::as_str) == Some("allow") {
                "trusted"
            } else {
                "untrusted"
            },
        );
        projects.insert(path, Item::Table(table));
    }
    Ok(())
}

fn mutate_execution(config: &mut DocumentMut, mutation: &Value) -> Result<(), String> {
    let values = form_values(mutation)?;
    validate_enum(
        values,
        "approval_policy",
        &["untrusted", "on-request", "never"],
    )?;
    validate_enum(
        values,
        "sandbox_mode",
        &["read-only", "workspace-write", "danger-full-access"],
    )?;
    validate_enum(
        values,
        "web_search",
        &["disabled", "cached", "indexed", "live"],
    )?;
    validate_enum(
        values,
        "shell_environment_policy.inherit",
        &["all", "core", "none"],
    )?;
    for key in ["approval_policy", "sandbox_mode", "web_search"] {
        set_root_string(config, key, values.get(key));
    }
    set_root_bool(config, "allow_login_shell", values.get("allow_login_shell"));
    let workspace = ensure_table(config, "sandbox_workspace_write")?;
    set_string_array(
        workspace,
        "writable_roots",
        values.get("sandbox_workspace_write.writable_roots"),
    );
    for (key, form_key) in [
        ("network_access", "sandbox_workspace_write.network_access"),
        (
            "exclude_tmpdir_env_var",
            "sandbox_workspace_write.exclude_tmpdir_env_var",
        ),
        (
            "exclude_slash_tmp",
            "sandbox_workspace_write.exclude_slash_tmp",
        ),
    ] {
        set_bool(workspace, key, values.get(form_key));
    }
    let shell = ensure_table(config, "shell_environment_policy")?;
    set_string(
        shell,
        "inherit",
        values.get("shell_environment_policy.inherit"),
    );
    set_bool(
        shell,
        "ignore_default_excludes",
        values.get("shell_environment_policy.ignore_default_excludes"),
    );
    set_string_array(
        shell,
        "exclude",
        values.get("shell_environment_policy.exclude"),
    );
    set_string_array(
        shell,
        "include_only",
        values.get("shell_environment_policy.include_only"),
    );
    Ok(())
}

fn mutate_features(config: &mut DocumentMut, mutation: &Value) -> Result<(), String> {
    let values = form_values(mutation)?;
    let table = ensure_table(config, "features")?;
    for (key, _, _) in FEATURE_FLAGS {
        let form_key = format!("features.{key}");
        match values.get(&form_key).and_then(Value::as_bool) {
            Some(enabled) => table[key] = value(enabled),
            None => {
                table.remove(key);
            }
        }
    }
    Ok(())
}

fn form_values(mutation: &Value) -> Result<&Value, String> {
    mutation
        .get("payload")
        .and_then(|payload| payload.get("values"))
        .filter(|values| values.is_object())
        .ok_or_else(|| "Dynamic form values are required".to_string())
}

fn entity_id<'a>(mutation: &'a Value, label: &str) -> Result<&'a str, String> {
    mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| {
            mutation
                .get("payload")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
        })
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("{label} id is required"))
}

fn ensure_table<'a>(config: &'a mut DocumentMut, key: &str) -> Result<&'a mut Table, String> {
    if !config.get(key).is_some_and(Item::is_table) {
        config[key] = Item::Table(Table::new());
    }
    config
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| format!("Codex {key} must be a table"))
}

fn table_str<'a>(config: &'a DocumentMut, table: &str, key: &str) -> Option<&'a str> {
    config
        .get(table)
        .and_then(Item::as_table_like)
        .and_then(|value| value.get(key))
        .and_then(Item::as_str)
}

fn table_bool(config: &DocumentMut, table: &str, key: &str) -> Option<bool> {
    config
        .get(table)
        .and_then(Item::as_table_like)
        .and_then(|value| value.get(key))
        .and_then(Item::as_bool)
}

fn table_string_array(config: &DocumentMut, table: &str, key: &str) -> Vec<String> {
    config
        .get(table)
        .and_then(Item::as_table_like)
        .and_then(|value| value.get(key))
        .and_then(Item::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn validate_enum(payload: &Value, key: &str, allowed: &[&str]) -> Result<(), String> {
    let Some(input) = payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if allowed.contains(&input) {
        Ok(())
    } else {
        Err(format!("Invalid Codex {key}: {input}"))
    }
}

fn set_root_string(config: &mut DocumentMut, key: &str, input: Option<&Value>) {
    match input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(input) => config[key] = value(input),
        None => {
            config.remove(key);
        }
    }
}

fn set_root_bool(config: &mut DocumentMut, key: &str, input: Option<&Value>) {
    match input.and_then(Value::as_bool) {
        Some(input) => config[key] = value(input),
        None => {
            config.remove(key);
        }
    }
}

fn set_string(table: &mut Table, key: &str, input: Option<&Value>) {
    match input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(input) => table[key] = value(input),
        None => {
            table.remove(key);
        }
    }
}

fn set_bool(table: &mut Table, key: &str, input: Option<&Value>) {
    match input.and_then(Value::as_bool) {
        Some(input) => table[key] = value(input),
        None => {
            table.remove(key);
        }
    }
}

fn set_string_array(table: &mut Table, key: &str, input: Option<&Value>) {
    let Some(values) = input.and_then(Value::as_array) else {
        table.remove(key);
        return;
    };
    let mut output = Array::new();
    for input in values
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        output.push(input);
    }
    table[key] = value(output);
}

fn table_to_json(item: Option<&Item>) -> Value {
    let Some(table) = item.and_then(Item::as_table_like) else {
        return json!({});
    };
    Value::Object(
        table
            .iter()
            .filter_map(|(key, value)| Some((key.to_string(), json!(value.as_str()?))))
            .collect::<Map<_, _>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_plugin_owned_dynamic_form_sections() {
        let descriptor = describe(&json!({ "instance": { "id": "codex-test" } })).unwrap();
        let sections = descriptor["sections"].as_array().unwrap();
        for id in ["general", "models", "execution", "features"] {
            let section = sections.iter().find(|section| section["id"] == id).unwrap();
            assert_eq!(section["renderer"], "form");
            assert_eq!(section["source"], "plugin");
            assert!(section["label"].is_object());
        }
    }

    #[test]
    fn model_mutation_preserves_unknown_fields() {
        let mut config = parse("model = \"old\"\nunknown = 42\n").unwrap();
        mutate_models(
            &mut config,
            &json!({ "payload": { "values": { "model": "new", "model_reasoning_effort": "high" } } }),
        )
        .unwrap();
        assert_eq!(root_str(&config, "model"), Some("new"));
        assert_eq!(config.get("unknown").and_then(Item::as_integer), Some(42));
    }

    #[test]
    fn reads_structured_codex_settings() {
        let config = parse(
            r#"model = "gpt-test"
model_provider = "custom"
review_model = "gpt-review"
model_reasoning_effort = "high"
plan_mode_reasoning_effort = "xhigh"
model_reasoning_summary = "detailed"
model_verbosity = "low"
personality = "pragmatic"
service_tier = "fast"
approval_policy = "never"
sandbox_mode = "danger-full-access"
web_search = "live"
allow_login_shell = false

[sandbox_workspace_write]
writable_roots = ["/work", "/tmp/build"]
network_access = true

[shell_environment_policy]
inherit = "core"
exclude = ["AWS_*"]
include_only = ["PATH", "HOME"]

[features]
memories = true
"#,
        )
        .unwrap();
        let model_settings = models(&config);
        assert_eq!(model_settings["values"]["model_provider"], "custom");
        assert_eq!(
            model_settings["values"]["plan_mode_reasoning_effort"],
            "xhigh"
        );
        assert_eq!(
            model_settings["values"]["model_reasoning_summary"],
            "detailed"
        );
        let execution_settings = execution(&config);
        assert_eq!(execution_settings["values"]["web_search"], "live");
        assert_eq!(
            execution_settings["values"]["sandbox_workspace_write.writable_roots"][1],
            "/tmp/build"
        );
        assert_eq!(
            execution_settings["values"]["shell_environment_policy.inherit"],
            "core"
        );
        let feature_settings = features(&config);
        assert_eq!(feature_settings["values"]["features.memories"], true);
        assert_eq!(feature_settings["schemaVersion"], 1);
    }

    #[test]
    fn permission_mutation_preserves_unknown_table_fields() {
        let mut config = parse(
            "unknown = 42\n[sandbox_workspace_write]\ncustom = \"keep\"\n[shell_environment_policy]\nset = { FOO = \"bar\" }\n",
        )
        .unwrap();
        mutate_execution(
            &mut config,
            &json!({
                "payload": {
                    "values": {
                        "approval_policy": "on-request",
                        "sandbox_mode": "workspace-write",
                        "web_search": "cached",
                        "allow_login_shell": true,
                        "sandbox_workspace_write.writable_roots": ["/work"],
                        "sandbox_workspace_write.network_access": true,
                        "sandbox_workspace_write.exclude_tmpdir_env_var": false,
                        "sandbox_workspace_write.exclude_slash_tmp": true,
                        "shell_environment_policy.inherit": "none",
                        "shell_environment_policy.ignore_default_excludes": false,
                        "shell_environment_policy.exclude": ["TOKEN_*"],
                        "shell_environment_policy.include_only": ["PATH"]
                    }
                }
            }),
        )
        .unwrap();
        assert_eq!(root_str(&config, "sandbox_mode"), Some("workspace-write"));
        assert_eq!(
            table_str(&config, "sandbox_workspace_write", "custom"),
            Some("keep")
        );
        assert!(config["shell_environment_policy"]["set"].is_inline_table());
        assert_eq!(
            table_string_array(&config, "shell_environment_policy", "include_only"),
            vec!["PATH"]
        );
    }

    #[test]
    fn feature_mutation_preserves_unknown_flags() {
        let mut config =
            parse("[features]\nunknown_future_flag = true\nmemories = false\n").unwrap();
        mutate_features(
            &mut config,
            &json!({ "payload": { "values": {
                "features.memories": true
            } } }),
        )
        .unwrap();
        assert_eq!(table_bool(&config, "features", "memories"), Some(true));
        assert_eq!(
            table_bool(&config, "features", "unknown_future_flag"),
            Some(true)
        );
        assert_eq!(table_bool(&config, "features", "apps"), None);
    }

    #[test]
    fn rejects_invalid_codex_enums() {
        let mut config = DocumentMut::new();
        let error = mutate_execution(
            &mut config,
            &json!({ "payload": { "values": {
                "approval_policy": "always-do-it"
            } } }),
        )
        .unwrap_err();
        assert!(error.contains("approval_policy"));
    }

    #[test]
    fn reads_mcp_server() {
        let config =
            parse("[mcp_servers.demo]\ncommand = \"npx\"\nargs = [\"-y\", \"demo\"]\n").unwrap();
        let servers = mcp_servers(&config);
        assert_eq!(servers[0]["id"], "demo");
        assert_eq!(servers[0]["args"][1], "demo");
    }
}
