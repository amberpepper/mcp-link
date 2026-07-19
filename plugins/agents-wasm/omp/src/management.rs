use mcp_link_agent_wasm_sdk::{
    finish_json_management_mutation, management_section, management_section_descriptor,
    masked_secret, read_json_document, ConfigDocument, Host,
};
use serde_json::{json, Map, Value};

const CONFIG: &str = "config";
const MODELS: &str = "models";
const MCP: &str = "mcp";

pub(super) fn describe(params: &Value) -> Result<Value, String> {
    let instance = params.get("instance").ok_or("OMP instance is required")?;
    Ok(json!({
        "schemaVersion": 1,
        "agentId": "omp",
        "instanceId": instance.get("id"),
        "sections": [
            section("overview", "overview", "plugin", true),
            section("mcp", "mcp", "plugin", false),
            section("skills", "skills", "host", false),
            section("prompts", "prompts", "host", false),
            section("providers", "providers", "plugin", false),
            section("models", "models", "plugin", false),
            section("environment", "environment", "plugin", true),
            section("raw-config", "raw-config", "host", false)
        ]
    }))
}

pub(super) fn load_section(params: &Value) -> Result<Value, String> {
    let id = required_string(params, "section")?;
    let (config_doc, config) = read_yaml_document(CONFIG, "OMP config.yml")?;
    let (models_doc, models) = read_yaml_document(MODELS, "OMP models.yml")?;
    let (mcp_doc, mcp) = read_json_document(MCP, "", "OMP mcp.json")?;
    let (revision, data) = match id {
        "overview" => (
            &config_doc.revision,
            overview(params, &config, &models, &mcp),
        ),
        "mcp" => (&mcp_doc.revision, json!({ "servers": mcp_servers(&mcp) })),
        "providers" => (
            &models_doc.revision,
            json!({
                "providers": providers(&models, &config),
                "secretInput": {
                    "mode": "environment-variable",
                    "defaultEnvironmentVariable": "OPENAI_API_KEY"
                }
            }),
        ),
        "models" => (&config_doc.revision, model_settings(&config, &models)),
        "environment" => (
            &config_doc.revision,
            environment(params, &config, &models, &mcp),
        ),
        _ => return Err(format!("Unsupported OMP management section: {id}")),
    };
    Ok(management_section(id, revision, data))
}

pub(super) fn mutate(params: &Value) -> Result<Value, String> {
    let mutation = params
        .get("mutation")
        .ok_or("OMP management mutation is required")?;
    let section = required_string(mutation, "section")?;
    let action = required_string(mutation, "action")?;
    let expected = required_string(mutation, "expectedRevision")?;
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match section {
        "mcp" => {
            let (document, mut config) = read_json_document(MCP, "", "OMP mcp.json")?;
            ensure_revision(expected, &document.revision)?;
            mutate_mcp(&mut config, action, mutation)?;
            finish_json_management_mutation(
                MCP, "", section, "mcp.json", &config, document, dry_run, true,
            )
        }
        "providers" => {
            let (document, mut config) = read_yaml_document(MODELS, "OMP models.yml")?;
            ensure_revision(expected, &document.revision)?;
            let original = config.clone();
            mutate_provider(&mut config, action, mutation)?;
            let content =
                patch_provider_yaml(&document.content, &original, &config, action, mutation)?;
            finish_yaml_content(
                MODELS,
                section,
                "models.yml",
                content,
                document,
                dry_run,
                false,
            )
        }
        "models" => {
            let (document, mut config) = read_yaml_document(CONFIG, "OMP config.yml")?;
            ensure_revision(expected, &document.revision)?;
            let original = config.clone();
            mutate_model_settings(&mut config, mutation)?;
            let content = patch_model_roles_yaml(&document.content, &original, &config)?;
            finish_yaml_content(
                CONFIG,
                section,
                "config.yml",
                content,
                document,
                dry_run,
                false,
            )
        }
        _ => Err(format!("OMP section is read-only: {section}")),
    }
}

fn section(id: &str, renderer: &str, source: &str, read_only: bool) -> Value {
    management_section_descriptor(id, renderer, source, read_only)
}

fn overview(params: &Value, config: &Value, models: &Value, mcp: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "cliName": "OMP (Oh My Pi)",
        "configRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot"),
        "defaultModel": config.pointer("/modelRoles/default"),
        "defaultProvider": config.pointer("/modelRoles/default").and_then(Value::as_str).and_then(|model| model.split_once('/').map(|(provider, _)| provider)),
        "mcpServerCount": mcp_servers(mcp).len(),
        "providerCount": providers(models, config).len(),
        "skillTargetCount": 2,
        "warnings": []
    })
}

fn mcp_servers(config: &Value) -> Vec<Value> {
    let disabled = config
        .get("disabledServers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    config
        .get("mcpServers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|servers| servers.iter())
        .map(|(id, server)| {
            let transport = server.get("type").and_then(Value::as_str).unwrap_or("stdio");
            json!({
                "id": id,
                "name": id,
                "transport": transport,
                "command": server.get("command"),
                "args": server.get("args").cloned().unwrap_or_else(|| json!([])),
                "url": server.get("url"),
                "env": server.get("env").cloned().unwrap_or_else(|| json!({})),
                "headers": server.get("headers").cloned().unwrap_or_else(|| json!({})),
                "enabled": server.get("enabled").and_then(Value::as_bool).unwrap_or(true) && !disabled.contains(&id.as_str()),
                "scope": "global"
            })
        })
        .collect()
}

fn providers(models: &Value, config: &Value) -> Vec<Value> {
    models
        .get("providers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|providers| providers.iter())
        .map(|(id, provider)| {
            let model_ids = provider
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|model| model.get("id").and_then(Value::as_str))
                .collect::<Vec<_>>();
            let api = provider.get("api").and_then(Value::as_str).unwrap_or("custom");
            let default = config.pointer("/modelRoles/default").and_then(Value::as_str);
            json!({
                "id": id,
                "name": provider.get("name").and_then(Value::as_str).unwrap_or(id),
                "protocol": api_protocol(api),
                "baseUrl": provider.get("baseUrl"),
                "apiKey": masked_secret(provider.get("apiKey").and_then(Value::as_str)),
                "defaultModel": default.filter(|model| model.starts_with(&format!("{id}/"))).and_then(|model| model.split_once('/').map(|(_, model)| model)),
                "models": model_ids,
                "enabled": true
            })
        })
        .collect()
}

fn model_settings(config: &Value, models: &Value) -> Value {
    let available = providers(models, config)
        .iter()
        .flat_map(|provider| {
            let id = provider
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            provider
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(move |model| format!("{id}/{model}"))
        })
        .collect::<Vec<_>>();
    json!({
        "defaultModel": config.pointer("/modelRoles/default"),
        "smallModel": config.pointer("/modelRoles/smol"),
        "reasoningModel": config.pointer("/modelRoles/slow"),
        "reasoningEffort": Value::Null,
        "availableModels": available,
        "aliases": config.get("modelRoles").cloned().unwrap_or_else(|| json!({}))
    })
}

fn environment(params: &Value, config: &Value, models: &Value, mcp: &Value) -> Value {
    let instance = params.get("instance").unwrap_or(&Value::Null);
    json!({
        "configFiles": [
            { "id": "config", "label": "config.yml", "path": "~/.omp/agent/config.yml", "exists": !config.as_object().is_none_or(Map::is_empty) },
            { "id": "models", "label": "models.yml", "path": "~/.omp/agent/models.yml", "exists": !models.as_object().is_none_or(Map::is_empty) },
            { "id": "mcp", "label": "mcp.json", "path": "~/.omp/agent/mcp.json", "exists": !mcp.as_object().is_none_or(Map::is_empty) }
        ],
        "variables": [
            { "name": "OMP_PROFILE", "secret": false, "source": "environment" },
            { "name": "OMP_MCP_TIMEOUT_MS", "secret": false, "source": "environment" }
        ],
        "cliRoot": instance.get("cliRoot"),
        "sessionRoot": instance.get("sessionRoot"),
        "skillRoot": instance.get("skillRoot")
    })
}

fn mutate_mcp(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "MCP server")?;
    let servers = object_field(config, "mcpServers", "OMP MCP configuration")?;
    if action == "remove" {
        servers.remove(id);
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported OMP MCP action: {action}"));
    }
    let payload = mutation
        .get("payload")
        .ok_or("MCP server payload is required")?;
    let transport = payload
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    let mut server = servers
        .get(id)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    server.insert("type".into(), json!(transport));
    server.insert(
        "enabled".into(),
        json!(payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)),
    );
    if transport == "stdio" {
        server.insert(
            "command".into(),
            json!(non_empty(payload.get("command")).unwrap_or_default()),
        );
        server.insert(
            "args".into(),
            payload.get("args").cloned().unwrap_or_else(|| json!([])),
        );
        server.insert(
            "env".into(),
            payload.get("env").cloned().unwrap_or_else(|| json!({})),
        );
        server.remove("url");
        server.remove("headers");
    } else {
        server.insert(
            "url".into(),
            json!(non_empty(payload.get("url")).unwrap_or_default()),
        );
        server.insert(
            "headers".into(),
            payload.get("headers").cloned().unwrap_or_else(|| json!({})),
        );
        server.remove("command");
        server.remove("args");
        server.remove("env");
    }
    servers.insert(id.to_string(), Value::Object(server));
    if payload
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        remove_array_string(config, "disabledServers", id)?;
    }
    Ok(())
}

fn mutate_provider(config: &mut Value, action: &str, mutation: &Value) -> Result<(), String> {
    let id = entity_id(mutation, "Provider")?;
    let providers = object_field(config, "providers", "OMP models configuration")?;
    if action == "remove" {
        providers.remove(id);
        return Ok(());
    }
    if action != "upsert" {
        return Err(format!("Unsupported OMP provider action: {action}"));
    }
    let payload = mutation
        .get("payload")
        .ok_or("Provider payload is required")?;
    let mut provider = providers
        .get(id)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(name) = non_empty(payload.get("name")) {
        provider.insert("name".into(), json!(name));
    }
    if let Some(base_url) = non_empty(payload.get("baseUrl")) {
        provider.insert("baseUrl".into(), json!(base_url));
    } else {
        provider.remove("baseUrl");
    }
    let protocol = payload
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or("openai");
    if provider.get("api").is_none() || protocol != "custom" {
        provider.insert("api".into(), json!(protocol_api(protocol)));
    }
    if let Some(key) = non_empty(payload.get("apiKeyValue")) {
        provider.insert("apiKey".into(), json!(key));
    } else if let Some(variable) = non_empty(payload.get("apiKeyEnvironmentVariable")) {
        provider.insert("apiKey".into(), json!(variable));
    }
    if let Some(models) = payload.get("models").and_then(Value::as_array) {
        let previous = provider
            .get("models")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        provider.insert(
            "models".into(),
            Value::Array(
                models
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|id| {
                        previous
                            .iter()
                            .find(|model| model.get("id").and_then(Value::as_str) == Some(id))
                            .cloned()
                            .unwrap_or_else(|| json!({ "id": id }))
                    })
                    .collect(),
            ),
        );
    }
    providers.insert(id.to_string(), Value::Object(provider));
    Ok(())
}

fn mutate_model_settings(config: &mut Value, mutation: &Value) -> Result<(), String> {
    let payload = mutation
        .get("payload")
        .ok_or("OMP model settings payload is required")?;
    let roles = object_field(config, "modelRoles", "OMP settings")?;
    set_optional_string(roles, "default", payload.get("defaultModel"));
    set_optional_string(roles, "smol", payload.get("smallModel"));
    set_optional_string(roles, "slow", payload.get("reasoningModel"));
    Ok(())
}

fn read_yaml_document(resource: &str, label: &str) -> Result<(ConfigDocument, Value), String> {
    let document = Host::config_read(resource, "")?;
    let value = if document.content.trim().is_empty() {
        json!({})
    } else {
        serde_yaml::from_str(&document.content)
            .map_err(|error| format!("Invalid {label}: {error}"))?
    };
    Ok((document, value))
}

fn patch_provider_yaml(
    content: &str,
    before: &Value,
    after: &Value,
    action: &str,
    mutation: &Value,
) -> Result<String, String> {
    let id = entity_id(mutation, "Provider")?;
    let mut yaml = LosslessYaml::new(content);
    if action == "remove" {
        yaml.remove(&["providers", id]);
        return Ok(yaml.finish());
    }

    let next = after
        .pointer(&format!("/providers/{}", escape_json_pointer(id)))
        .ok_or("Updated OMP provider is missing")?;
    let previous = before.pointer(&format!("/providers/{}", escape_json_pointer(id)));
    if previous.is_none() {
        yaml.set_value(&["providers", id], next)?;
        return Ok(yaml.finish());
    }
    if yaml.entry(&["providers", id]).is_none() {
        yaml.set_value(
            &["providers"],
            after
                .get("providers")
                .ok_or("Updated OMP providers are missing")?,
        )?;
        return Ok(yaml.finish());
    }
    if yaml.has_inline_value(&["providers", id]) {
        yaml.set_value(&["providers", id], next)?;
        return Ok(yaml.finish());
    }

    let previous = previous.unwrap_or(&Value::Null);
    for key in ["name", "baseUrl", "api", "apiKey", "models"] {
        if previous.get(key) == next.get(key) {
            continue;
        }
        match next.get(key) {
            Some(value) => yaml.set_value(&["providers", id, key], value)?,
            None => yaml.remove(&["providers", id, key]),
        }
    }
    Ok(yaml.finish())
}

fn patch_model_roles_yaml(content: &str, before: &Value, after: &Value) -> Result<String, String> {
    let mut yaml = LosslessYaml::new(content);
    let previous = before.get("modelRoles");
    let next = after
        .get("modelRoles")
        .ok_or("Updated OMP model roles are missing")?;
    if previous.is_none() {
        yaml.set_value(&["modelRoles"], next)?;
        return Ok(yaml.finish());
    }
    if yaml.has_inline_value(&["modelRoles"]) {
        yaml.set_value(&["modelRoles"], next)?;
        return Ok(yaml.finish());
    }
    let previous = previous.unwrap_or(&Value::Null);
    for key in ["default", "smol", "slow"] {
        if previous.get(key) == next.get(key) {
            continue;
        }
        match next.get(key) {
            Some(value) => yaml.set_value(&["modelRoles", key], value)?,
            None => yaml.remove(&["modelRoles", key]),
        }
    }
    Ok(yaml.finish())
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[derive(Clone, Copy)]
struct YamlEntry {
    start: usize,
    end: usize,
    indent: usize,
    colon: usize,
}

struct LosslessYaml {
    lines: Vec<String>,
    newline: &'static str,
    trailing_newline: bool,
}

impl LosslessYaml {
    fn new(content: &str) -> Self {
        let newline = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let normalized = content.replace("\r\n", "\n");
        let trailing_newline = normalized.ends_with('\n');
        let mut lines = if normalized.is_empty() {
            Vec::new()
        } else {
            normalized
                .split('\n')
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        };
        if trailing_newline {
            lines.pop();
        }
        Self {
            lines,
            newline,
            trailing_newline,
        }
    }

    fn finish(self) -> String {
        let mut content = self.lines.join(self.newline);
        if self.trailing_newline {
            content.push_str(self.newline);
        }
        content
    }

    fn remove(&mut self, path: &[&str]) {
        if let Some(entry) = self.entry(path) {
            self.lines.drain(entry.start..entry.end);
        }
    }

    fn set_value(&mut self, path: &[&str], value: &Value) -> Result<(), String> {
        if path.is_empty() {
            return Err("OMP YAML path cannot be empty".to_string());
        }
        if let Some(entry) = self.entry(path) {
            let replacement = render_existing_yaml_entry(&self.lines[entry.start], entry, value)?;
            self.lines.splice(entry.start..entry.end, replacement);
            return Ok(());
        }
        if path.len() == 1 {
            let entry = render_new_yaml_entry(0, path[0], value)?;
            if !self.lines.is_empty() && self.lines.last().is_some_and(|line| !line.is_empty()) {
                self.lines.push(String::new());
            }
            self.lines.extend(entry);
            return Ok(());
        }

        let parent_path = &path[..path.len() - 1];
        if self.entry(parent_path).is_none() {
            if parent_path.len() == 1 {
                self.set_value(parent_path, &json!({}))?;
            } else {
                return Err(format!(
                    "OMP YAML parent path is missing: {}",
                    parent_path.join(".")
                ));
            }
        }
        self.expand_empty_mapping(parent_path);
        let parent = self
            .entry(parent_path)
            .ok_or_else(|| format!("OMP YAML parent path is missing: {}", parent_path.join(".")))?;
        let entry = render_new_yaml_entry(parent.indent + 2, path[path.len() - 1], value)?;
        self.lines.splice(parent.end..parent.end, entry);
        Ok(())
    }

    fn expand_empty_mapping(&mut self, path: &[&str]) {
        let Some(entry) = self.entry(path) else {
            return;
        };
        let line = &self.lines[entry.start];
        let tail = line[entry.colon + 1..].trim();
        if tail == "{}" || tail == "null" || tail == "~" {
            self.lines[entry.start].truncate(entry.colon + 1);
        }
    }

    fn has_inline_value(&self, path: &[&str]) -> bool {
        self.entry(path).is_some_and(|entry| {
            let tail = self.lines[entry.start][entry.colon + 1..].trim();
            !tail.is_empty() && !tail.starts_with('#')
        })
    }

    fn entry(&self, path: &[&str]) -> Option<YamlEntry> {
        let mut start = 0;
        let mut end = self.lines.len();
        let mut parent_indent: isize = -1;
        let mut found = None;
        for key in path {
            let direct_indent = (start..end)
                .filter_map(|index| parse_yaml_mapping_line(&self.lines[index]))
                .map(|(indent, _, _)| indent)
                .filter(|indent| (*indent as isize) > parent_indent)
                .min()?;
            let (index, indent, colon) = (start..end).find_map(|index| {
                let (indent, candidate, colon) = parse_yaml_mapping_line(&self.lines[index])?;
                (indent == direct_indent && candidate == *key).then_some((index, indent, colon))
            })?;
            let entry_end = yaml_entry_end(&self.lines, index, end, indent);
            let entry = YamlEntry {
                start: index,
                end: entry_end,
                indent,
                colon,
            };
            found = Some(entry);
            start = index + 1;
            end = entry_end;
            parent_indent = indent as isize;
        }
        found
    }
}

fn parse_yaml_mapping_line(line: &str) -> Option<(usize, String, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let trimmed = &line[indent..];
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with('-')
        || trimmed.starts_with("---")
        || trimmed.starts_with("...")
    {
        return None;
    }
    let relative_colon = yaml_delimiter(trimmed, ':')?;
    let token = trimmed[..relative_colon].trim();
    if token.is_empty() {
        return None;
    }
    let key = serde_yaml::from_str::<String>(token).unwrap_or_else(|_| token.to_string());
    Some((indent, key, indent + relative_colon))
}

fn yaml_delimiter(value: &str, target: char) -> Option<usize> {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if double_quoted && character == '\\' {
            escaped = true;
            continue;
        }
        if !double_quoted && character == '\'' {
            single_quoted = !single_quoted;
            continue;
        }
        if !single_quoted && character == '"' {
            double_quoted = !double_quoted;
            continue;
        }
        if !single_quoted && !double_quoted && character == target {
            return Some(index);
        }
    }
    None
}

fn yaml_entry_end(lines: &[String], start: usize, scope_end: usize, indent: usize) -> usize {
    for (index, line) in lines.iter().enumerate().take(scope_end).skip(start + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let next_indent = line.len() - line.trim_start_matches(' ').len();
        if next_indent <= indent {
            return index;
        }
    }
    scope_end
}

fn render_existing_yaml_entry(
    original_line: &str,
    entry: YamlEntry,
    value: &Value,
) -> Result<Vec<String>, String> {
    let prefix = &original_line[..entry.colon + 1];
    let comment = yaml_inline_comment(&original_line[entry.colon + 1..]);
    render_yaml_entry(prefix, entry.indent, value, comment)
}

fn render_new_yaml_entry(indent: usize, key: &str, value: &Value) -> Result<Vec<String>, String> {
    let key = serde_yaml::to_string(&Value::String(key.to_string()))
        .map_err(|error| error.to_string())?
        .trim()
        .to_string();
    render_yaml_entry(
        &format!("{}{key}:", " ".repeat(indent)),
        indent,
        value,
        None,
    )
}

fn render_yaml_entry(
    prefix: &str,
    indent: usize,
    value: &Value,
    comment: Option<&str>,
) -> Result<Vec<String>, String> {
    if !value.is_array() && !value.is_object() {
        let scalar = serde_yaml::to_string(value).map_err(|error| error.to_string())?;
        let scalar = scalar.trim();
        if scalar.contains('\n') {
            return Err("OMP YAML scalar cannot span multiple lines".to_string());
        }
        return Ok(vec![format!(
            "{prefix} {scalar}{}",
            comment.map(|value| format!(" {value}")).unwrap_or_default()
        )]);
    }
    if value.as_object().is_some_and(Map::is_empty) {
        return Ok(vec![format!(
            "{prefix} {{}}{}",
            comment.map(|value| format!(" {value}")).unwrap_or_default()
        )]);
    }
    if value.as_array().is_some_and(Vec::is_empty) {
        return Ok(vec![format!(
            "{prefix} []{}",
            comment.map(|value| format!(" {value}")).unwrap_or_default()
        )]);
    }

    let mut rendered = vec![format!(
        "{prefix}{}",
        comment.map(|value| format!(" {value}")).unwrap_or_default()
    )];
    let nested = serde_yaml::to_string(value).map_err(|error| error.to_string())?;
    rendered.extend(
        nested
            .trim_end_matches(['\r', '\n'])
            .lines()
            .map(|line| format!("{}{}", " ".repeat(indent + 2), line)),
    );
    Ok(rendered)
}

fn yaml_inline_comment(value: &str) -> Option<&str> {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;
    let mut previous = None;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            previous = Some(character);
            continue;
        }
        if double_quoted && character == '\\' {
            escaped = true;
            previous = Some(character);
            continue;
        }
        if !double_quoted && character == '\'' {
            single_quoted = !single_quoted;
        } else if !single_quoted && character == '"' {
            double_quoted = !double_quoted;
        } else if !single_quoted
            && !double_quoted
            && character == '#'
            && previous.is_none_or(char::is_whitespace)
        {
            return Some(&value[index..]);
        }
        previous = Some(character);
    }
    None
}

fn finish_yaml_content(
    resource: &str,
    section: &str,
    changed_resource: &str,
    content: String,
    document: ConfigDocument,
    dry_run: bool,
    restart_required: bool,
) -> Result<Value, String> {
    let changed = content != document.content;
    let revision = if dry_run {
        document.revision
    } else {
        Host::config_write_atomic(resource, "", &content, &document.revision)?
            .get("revision")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or("Host file.writeAtomic returned no revision")?
    };
    Ok(json!({
        "section": section,
        "revision": revision,
        "changed": changed,
        "changedResources": [changed_resource],
        "restartRequired": restart_required,
        "warnings": []
    }))
}

fn remove_array_string(config: &mut Value, key: &str, target: &str) -> Result<(), String> {
    let root = config
        .as_object_mut()
        .ok_or("OMP configuration must be an object")?;
    if let Some(values) = root.get_mut(key).and_then(Value::as_array_mut) {
        values.retain(|value| value.as_str() != Some(target));
    }
    Ok(())
}

fn object_field<'a>(
    config: &'a mut Value,
    key: &str,
    label: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let root = config
        .as_object_mut()
        .ok_or_else(|| format!("{label} must be an object"))?;
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), json!({}));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("{label}.{key} must be an object"))
}

fn set_optional_string(object: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    match non_empty(value) {
        Some(value) => {
            object.insert(key.to_string(), json!(value));
        }
        None => {
            object.remove(key);
        }
    }
}

fn api_protocol(api: &str) -> &str {
    if api.starts_with("openai") || api.starts_with("azure-openai") {
        "openai"
    } else if api.starts_with("anthropic") {
        "anthropic"
    } else if api.starts_with("google") {
        "gemini"
    } else {
        "custom"
    }
}

fn protocol_api(protocol: &str) -> &str {
    match protocol {
        "anthropic" => "anthropic-messages",
        "gemini" => "google-generative-ai",
        _ => "openai-completions",
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("OMP {key} is required"))
}

fn entity_id<'a>(mutation: &'a Value, label: &str) -> Result<&'a str, String> {
    mutation
        .get("entityId")
        .and_then(Value::as_str)
        .or_else(|| mutation.pointer("/payload/id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} id is required"))
}

fn non_empty(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn ensure_revision(expected: &str, current: &str) -> Result<(), String> {
    if expected == current {
        Ok(())
    } else {
        Err(format!(
            "CONFIG_CONFLICT: configuration changed on disk (expected {expected}, found {current})"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_update_preserves_auth_timeout_and_denylist() {
        let mut config = json!({
            "$schema": "schema.json",
            "mcpServers": { "demo": { "type": "http", "url": "https://old", "timeout": 4000, "auth": { "type": "oauth" } } },
            "disabledServers": ["other"]
        });
        mutate_mcp(&mut config, "upsert", &json!({
            "entityId": "demo", "payload": { "id": "demo", "transport": "http", "url": "https://new", "headers": {}, "enabled": true }
        })).unwrap();
        assert_eq!(config["mcpServers"]["demo"]["timeout"], 4000);
        assert_eq!(config["mcpServers"]["demo"]["auth"]["type"], "oauth");
        assert_eq!(config["disabledServers"][0], "other");
        assert_eq!(config["$schema"], "schema.json");
    }

    #[test]
    fn provider_update_preserves_model_metadata_and_equivalence() {
        let mut config = json!({
            "providers": { "demo": { "api": "openai-responses", "apiKey": "DEMO_KEY", "models": [{ "id": "a", "contextWindow": 1000 }], "unknown": true } },
            "equivalence": { "overrides": { "demo/a": "upstream-a" } }
        });
        mutate_provider(&mut config, "upsert", &json!({
            "entityId": "demo", "payload": { "id": "demo", "protocol": "custom", "models": ["a"], "apiKeyEnvironmentVariable": "" }
        })).unwrap();
        assert_eq!(config["providers"]["demo"]["api"], "openai-responses");
        assert_eq!(config["providers"]["demo"]["apiKey"], "DEMO_KEY");
        assert_eq!(
            config["providers"]["demo"]["models"][0]["contextWindow"],
            1000
        );
        assert_eq!(config["providers"]["demo"]["unknown"], true);
        assert_eq!(config["equivalence"]["overrides"]["demo/a"], "upstream-a");
    }

    #[test]
    fn provider_yaml_patch_preserves_comments_line_endings_and_unrelated_formatting() {
        let content = "# models file\r\nproviders:\r\n  demo:\r\n    # provider note\r\n    api: openai-responses # keep protocol note\r\n    apiKey: DEMO_KEY\r\n    models:\r\n      - id: a # model note\r\n        contextWindow: 1000\r\nequivalence: { enabled: true } # untouched\r\n";
        let before: Value = serde_yaml::from_str(content).unwrap();
        let mut after = before.clone();
        let mutation = json!({
            "entityId": "demo",
            "payload": {
                "id": "demo",
                "name": "Demo Provider",
                "baseUrl": "https://example.test/v1",
                "protocol": "custom",
                "models": ["a"],
                "apiKeyEnvironmentVariable": ""
            }
        });
        mutate_provider(&mut after, "upsert", &mutation).unwrap();
        let patched = patch_provider_yaml(content, &before, &after, "upsert", &mutation).unwrap();
        assert!(patched.contains("# models file\r\n"));
        assert!(patched.contains("# provider note\r\n"));
        assert!(patched.contains("api: openai-responses # keep protocol note\r\n"));
        assert!(patched.contains("- id: a # model note\r\n"));
        assert!(patched.contains("equivalence: { enabled: true } # untouched\r\n"));
        assert!(patched.contains("baseUrl: https://example.test/v1\r\n"));
        let parsed: Value = serde_yaml::from_str(&patched).unwrap();
        assert_eq!(parsed, after);
    }

    #[test]
    fn model_roles_yaml_patch_changes_only_managed_scalars() {
        let content = "# config\nmodelRoles:\n  default: old/main # selected model\n  # keep this note\n  smol: old/small\n  slow: old/reasoning\n  custom: untouched/model\nother: { spacing: stays }\n";
        let before: Value = serde_yaml::from_str(content).unwrap();
        let mut after = before.clone();
        mutate_model_settings(
            &mut after,
            &json!({
                "payload": {
                    "defaultModel": "new/main",
                    "smallModel": "",
                    "reasoningModel": "old/reasoning"
                }
            }),
        )
        .unwrap();
        let patched = patch_model_roles_yaml(content, &before, &after).unwrap();
        assert!(patched.contains("default: new/main # selected model"));
        assert!(patched.contains("# keep this note"));
        assert!(patched.contains("custom: untouched/model"));
        assert!(patched.contains("other: { spacing: stays }"));
        assert!(!patched.contains("smol: old/small"));
        let parsed: Value = serde_yaml::from_str(&patched).unwrap();
        assert_eq!(parsed, after);
    }

    #[test]
    fn yaml_comment_detection_does_not_treat_url_fragments_as_comments() {
        assert_eq!(yaml_inline_comment(" https://example.test/#fragment"), None);
        assert_eq!(yaml_inline_comment(" value # note"), Some("# note"));
    }
}
