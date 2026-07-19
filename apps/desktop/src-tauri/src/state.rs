use rmcp::{
    model::ClientInfo,
    service::{Peer, RoleClient, RoleServer, RunningService},
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

use crate::util::{
    json::{set_object_field, set_object_value, value_id},
    time::now_iso,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoreState {
    pub(crate) servers: Vec<Value>,
    #[serde(default)]
    pub(crate) gateway_providers: Vec<Value>,
    #[serde(default)]
    pub(crate) gateway_routes: Vec<Value>,
    pub(crate) settings: Map<String, Value>,
    pub(crate) workflows: Vec<Value>,
    pub(crate) hooks: Vec<Value>,
    pub(crate) skills: Vec<Value>,
    #[serde(default)]
    pub(crate) skill_installations: Vec<Value>,
    #[serde(default)]
    pub(crate) agent_instances: Vec<Value>,
}

pub(crate) struct DesktopState {
    pub(crate) store_path: PathBuf,
    pub(crate) store: Mutex<StoreState>,
    pub(crate) runtimes: Mutex<HashMap<String, DesktopRuntime>>,
    pub(crate) mcp_client_peers: Mutex<HashMap<String, Peer<RoleServer>>>,
    pub(crate) mcp_endpoint: Mutex<Option<String>>,
    pub(crate) mcp_listener_error: Mutex<Option<String>>,
    #[cfg(feature = "desktop")]
    pub(crate) mcp_server_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    pub(crate) model_gateway_endpoint: Mutex<Option<String>>,
    pub(crate) model_gateway_listener_error: Mutex<Option<String>>,
    #[cfg(feature = "server")]
    pub(crate) server_sessions: Mutex<HashMap<String, u64>>,
    #[cfg(feature = "desktop")]
    pub(crate) model_gateway_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

pub(crate) struct DesktopRuntime {
    pub(crate) client: RunningService<RoleClient, ClientInfo>,
    pub(crate) pid: Option<u32>,
}

impl DesktopState {
    pub(crate) fn load(store_path: PathBuf) -> Self {
        let store = match load_store(&store_path) {
            Ok(Some(store)) => store,
            Ok(None) => create_default_state(),
            Err(error) => {
                eprintln!("Failed to load desktop SQLite state: {error}");
                create_default_state()
            }
        };
        let store = normalize_state(store);

        let state = Self {
            store_path,
            store: Mutex::new(store),
            runtimes: Mutex::new(HashMap::new()),
            mcp_client_peers: Mutex::new(HashMap::new()),
            mcp_endpoint: Mutex::new(None),
            mcp_listener_error: Mutex::new(None),
            #[cfg(feature = "desktop")]
            mcp_server_task: Mutex::new(None),
            model_gateway_endpoint: Mutex::new(None),
            model_gateway_listener_error: Mutex::new(None),
            #[cfg(feature = "server")]
            server_sessions: Mutex::new(HashMap::new()),
            #[cfg(feature = "desktop")]
            model_gateway_task: Mutex::new(None),
        };

        let _ = state.save();
        state
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let store = self
            .store
            .lock()
            .map_err(|_| "Failed to lock desktop state".to_string())?;
        save_store(&self.store_path, &store)
    }

    pub(crate) fn access_keys_db_path(&self) -> PathBuf {
        self.store_path.clone()
    }

    #[cfg_attr(not(feature = "server"), allow(dead_code))]
    pub(crate) fn server_password(&self) -> String {
        self.store
            .lock()
            .ok()
            .and_then(|store| {
                store
                    .settings
                    .get("serverPassword")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "admin".to_string())
    }
}

pub(crate) fn create_default_state() -> StoreState {
    StoreState {
        servers: vec![],
        gateway_providers: vec![],
        gateway_routes: vec![],
        settings: Map::from_iter([
            ("showWindowOnStartup".to_string(), json!(false)),
            ("closeBehavior".to_string(), json!("exit")),
            ("sessionTerminal".to_string(), json!("auto")),
            ("theme".to_string(), json!("system")),
            ("language".to_string(), json!("zh")),
            ("desktopMcpListenHost".to_string(), json!("127.0.0.1")),
            ("desktopMcpListenPort".to_string(), json!(3284)),
            ("modelGatewayListenHost".to_string(), json!("127.0.0.1")),
            ("modelGatewayListenPort".to_string(), json!(3285)),
            (
                "modelGatewayAccessKey".to_string(),
                json!(format!(
                    "mcpg_{}{}",
                    Uuid::new_v4().simple(),
                    Uuid::new_v4().simple()
                )),
            ),
            ("serverPassword".to_string(), json!("admin")),
        ]),
        workflows: vec![],
        hooks: vec![],
        skills: vec![],
        skill_installations: vec![],
        agent_instances: vec![],
    }
}

pub(crate) fn normalize_state(mut store: StoreState) -> StoreState {
    store.settings.remove("loadExternalMCPConfigs");
    store
        .settings
        .entry("modelGatewayListenHost".to_string())
        .or_insert_with(|| json!("127.0.0.1"));
    store
        .settings
        .entry("modelGatewayListenPort".to_string())
        .or_insert_with(|| json!(3285));
    store
        .settings
        .entry("modelGatewayAccessKey".to_string())
        .or_insert_with(|| {
            json!(format!(
                "mcpg_{}{}",
                Uuid::new_v4().simple(),
                Uuid::new_v4().simple()
            ))
        });
    store.servers = store
        .servers
        .iter()
        .map(|server| {
            let mut normalized = normalize_server(server);
            // On startup, all servers are stopped because runtimes are not persisted.
            set_object_field(&mut normalized, "status", "stopped");
            set_object_value(&mut normalized, "pid", Value::Null);
            normalized
        })
        .collect();
    store
}

pub(crate) fn normalize_server(input: &Value) -> Value {
    let now = now_iso();
    let server_type = input
        .get("serverType")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            if input.get("remoteUrl").and_then(Value::as_str).is_some() {
                "remote-streamable".to_string()
            } else {
                "local".to_string()
            }
        });

    json!({
        "id": input.get("id").and_then(Value::as_str).map(ToOwned::to_owned).unwrap_or_else(|| Uuid::new_v4().to_string()),
        "name": input.get("name").and_then(Value::as_str)
            .or_else(|| input.get("command").and_then(Value::as_str))
            .or_else(|| input.get("remoteUrl").and_then(Value::as_str))
            .unwrap_or("MCP Server"),
        "description": input.get("description").and_then(Value::as_str).unwrap_or(""),
        "serverType": server_type,
        "command": input.get("command").cloned().unwrap_or(Value::Null),
        "args": input.get("args").and_then(Value::as_array).cloned().unwrap_or_default(),
        "env": input.get("env").and_then(Value::as_object).cloned().map(Value::Object).unwrap_or_else(|| json!({})),
        "remoteUrl": input.get("remoteUrl").cloned().unwrap_or(Value::Null),
        "bearerToken": input.get("bearerToken").cloned().unwrap_or(Value::Null),
        "autoStart": input.get("autoStart").and_then(Value::as_bool).unwrap_or(false),
        "disabled": input.get("disabled").and_then(Value::as_bool).unwrap_or(false),
        "startupTimeoutSec": input.get("startupTimeoutSec").and_then(Value::as_u64).filter(|value| *value > 0).unwrap_or(45),
        "capabilityTimeoutSec": input.get("capabilityTimeoutSec").and_then(Value::as_u64).filter(|value| *value > 0).unwrap_or(15),
        "setupInstructions": input.get("setupInstructions").cloned().unwrap_or(Value::Null),
        "inputParams": input.get("inputParams").and_then(Value::as_object).cloned().map(Value::Object).unwrap_or_else(|| json!({})),
        "verificationStatus": input.get("verificationStatus").cloned().unwrap_or(Value::Null),
        "required": input.get("required").and_then(Value::as_array).cloned().unwrap_or_default(),
        "latestVersion": input.get("latestVersion").cloned().unwrap_or(Value::Null),
        "version": input.get("version").cloned().unwrap_or(Value::Null),
        "importSource": input.get("importSource").cloned().unwrap_or(Value::Null),
        "bundlePath": input.get("bundlePath").cloned().unwrap_or(Value::Null),
        "status": input.get("status").and_then(Value::as_str).unwrap_or("stopped"),
        "errorMessage": input.get("errorMessage").cloned().unwrap_or(Value::Null),
        "pid": input.get("pid").cloned().unwrap_or(Value::Null),
        "logs": input.get("logs").and_then(Value::as_array).cloned().unwrap_or_default(),
        "tools": input.get("tools").and_then(Value::as_array).cloned().unwrap_or_default(),
        "resources": input.get("resources").and_then(Value::as_array).cloned().unwrap_or_default(),
        "prompts": input.get("prompts").and_then(Value::as_array).cloned().unwrap_or_default(),
        "toolPermissions": input.get("toolPermissions").and_then(Value::as_object).cloned().map(Value::Object).unwrap_or_else(|| json!({})),
        "createdAt": input.get("createdAt").cloned().unwrap_or_else(|| json!(now)),
        "updatedAt": input.get("updatedAt").cloned().unwrap_or_else(|| json!(now_iso()))
    })
}

fn open_store_db(store_path: &Path) -> Result<Connection, String> {
    const SCHEMA_VERSION: i64 = 2;
    if let Some(parent) = store_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let conn = Connection::open(store_path).map_err(|error| error.to_string())?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| error.to_string())?;
    let schema_version = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())?;
    if schema_version != SCHEMA_VERSION {
        conn.execute_batch(
            "DROP TABLE IF EXISTS gateway_routes;
             DROP TABLE IF EXISTS gateway_providers;
             DROP TABLE IF EXISTS agent_instances;
             DROP TABLE IF EXISTS skill_installations;",
        )
        .map_err(|error| error.to_string())?;
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS store_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS gateway_providers (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, protocol TEXT NOT NULL CHECK(protocol IN ('openai','anthropic')),
            api_format TEXT NOT NULL DEFAULT 'chat-completions' CHECK(api_format IN ('chat-completions','responses','messages')),
            base_url TEXT NOT NULL, api_key TEXT NOT NULL DEFAULT '', models_json TEXT NOT NULL DEFAULT '[]',
            enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)), created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS gateway_routes (
            id TEXT PRIMARY KEY, alias TEXT NOT NULL, protocol TEXT NOT NULL CHECK(protocol IN ('openai','anthropic')), provider_id TEXT NOT NULL,
            api_format TEXT NOT NULL DEFAULT 'chat-completions' CHECK(api_format IN ('chat-completions','responses','messages')),
            upstream_model TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            FOREIGN KEY(provider_id) REFERENCES gateway_providers(id) ON DELETE CASCADE,
            UNIQUE(provider_id, alias)
        );
        CREATE INDEX IF NOT EXISTS gateway_routes_provider_idx ON gateway_routes(provider_id);
        CREATE TABLE IF NOT EXISTS gateway_call_logs (
            id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE, started_at_ms INTEGER NOT NULL,
            finished_at_ms INTEGER, status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled')),
            http_status INTEGER, streaming INTEGER NOT NULL DEFAULT 0 CHECK(streaming IN (0,1)),
            client_protocol TEXT NOT NULL, upstream_protocol TEXT NOT NULL,
            requested_model TEXT NOT NULL, upstream_model TEXT NOT NULL,
            provider_id TEXT NOT NULL, provider_name TEXT NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0, cache_write_tokens INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0, first_token_ms INTEGER, duration_ms INTEGER,
            error TEXT
        );
        CREATE INDEX IF NOT EXISTS gateway_call_logs_started_idx ON gateway_call_logs(started_at_ms DESC);
        CREATE INDEX IF NOT EXISTS gateway_call_logs_status_idx ON gateway_call_logs(status, started_at_ms DESC);
        CREATE INDEX IF NOT EXISTS gateway_call_logs_provider_idx ON gateway_call_logs(provider_id, started_at_ms DESC);
        CREATE TABLE IF NOT EXISTS agent_instances (
            id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, label TEXT NOT NULL,
            cli_root TEXT, session_root TEXT, skill_root TEXT, resume_command TEXT,
            enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1))
        );
        CREATE INDEX IF NOT EXISTS agent_instances_agent_idx ON agent_instances(agent_id);
        CREATE TABLE IF NOT EXISTS skill_installations (
            id TEXT PRIMARY KEY, skill_id TEXT NOT NULL, agent_id TEXT NOT NULL, target_id TEXT NOT NULL,
            scope TEXT NOT NULL, project_path TEXT, mode TEXT NOT NULL CHECK(mode IN ('copy','symlink','native')), status TEXT NOT NULL,
            installed_path TEXT, native_reference TEXT, error TEXT, updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS skill_installations_skill_idx ON skill_installations(skill_id);
        CREATE INDEX IF NOT EXISTS skill_installations_agent_idx ON skill_installations(agent_id);",
    )
    .map_err(|error| error.to_string())?;
    ensure_gateway_api_format_columns(&conn)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(|error| error.to_string())?;
    Ok(conn)
}

fn ensure_gateway_api_format_columns(conn: &Connection) -> Result<(), String> {
    for (table, default) in [
        ("gateway_providers", "chat-completions"),
        ("gateway_routes", "chat-completions"),
    ] {
        let mut statement = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|error| error.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        if !columns.iter().any(|column| column == "api_format") {
            conn.execute(
                &format!(
                    "ALTER TABLE {table} ADD COLUMN api_format TEXT NOT NULL DEFAULT '{default}' CHECK(api_format IN ('chat-completions','responses','messages'))"
                ),
                [],
            )
            .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn load_store(store_path: &Path) -> Result<Option<StoreState>, String> {
    let conn = open_store_db(store_path)?;
    let mut stmt = conn
        .prepare("SELECT key, value FROM store_state")
        .map_err(|error| error.to_string())?;
    let mut rows = stmt.query([]).map_err(|error| error.to_string())?;
    let mut object = Map::new();

    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let key: String = row.get(0).map_err(|error| error.to_string())?;
        let raw_value: String = row.get(1).map_err(|error| error.to_string())?;
        let value = serde_json::from_str::<Value>(&raw_value).map_err(|error| error.to_string())?;
        object.insert(key, value);
    }

    if object.is_empty() {
        // A normalized database may contain domain rows without generic settings yet.
        let mut store = create_default_state();
        store.gateway_providers = read_gateway_providers(&conn)?;
        store.gateway_routes = read_gateway_routes(&conn)?;
        store.agent_instances = read_agent_instances(&conn)?;
        store.skill_installations = read_skill_installations(&conn)?;
        return if store.gateway_providers.is_empty()
            && store.gateway_routes.is_empty()
            && store.agent_instances.is_empty()
            && store.skill_installations.is_empty()
        {
            Ok(None)
        } else {
            Ok(Some(store))
        };
    }
    let mut store = serde_json::from_value::<StoreState>(Value::Object(object))
        .map_err(|error| error.to_string())?;
    store.gateway_providers = read_gateway_providers(&conn)?;
    store.gateway_routes = read_gateway_routes(&conn)?;
    store.agent_instances = read_agent_instances(&conn)?;
    store.skill_installations = read_skill_installations(&conn)?;
    Ok(Some(store))
}

fn read_gateway_providers(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn.prepare("SELECT id,name,protocol,base_url,api_key,models_json,enabled,created_at,updated_at,api_format FROM gateway_providers").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        let models: String = row.get(5)?;
        let storage_protocol: String = row.get(2)?;
        let api_format: String = row.get(9)?;
        Ok(json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"protocol":gateway_protocol_from_storage(&storage_protocol, &api_format),"baseUrl":row.get::<_,String>(3)?,"apiKey":row.get::<_,String>(4)?,"models":serde_json::from_str::<Value>(&models).unwrap_or_else(|_| json!([])),"enabled":row.get::<_,i64>(6)? != 0,"createdAt":row.get::<_,String>(7)?,"updatedAt":row.get::<_,String>(8)?}))
    }).map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

fn read_gateway_routes(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn.prepare("SELECT id,alias,protocol,provider_id,upstream_model,created_at,updated_at,api_format FROM gateway_routes").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        let storage_protocol: String = row.get(2)?;
        let api_format: String = row.get(7)?;
        Ok(json!({"id":row.get::<_,String>(0)?,"alias":row.get::<_,String>(1)?,"protocol":gateway_protocol_from_storage(&storage_protocol, &api_format),"providerId":row.get::<_,String>(3)?,"upstreamModel":row.get::<_,String>(4)?,"createdAt":row.get::<_,String>(5)?,"updatedAt":row.get::<_,String>(6)?}))
    }).map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

fn gateway_protocol_from_storage(protocol: &str, api_format: &str) -> &'static str {
    match (protocol, api_format) {
        ("anthropic", _) => "anthropic",
        ("openai", "responses") => "openai-responses",
        _ => "openai-compatible",
    }
}

fn gateway_protocol_for_storage(protocol: &str) -> (&'static str, &'static str) {
    match protocol {
        "anthropic" => ("anthropic", "messages"),
        "openai-responses" => ("openai", "responses"),
        _ => ("openai", "chat-completions"),
    }
}

fn read_agent_instances(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn.prepare("SELECT id,agent_id,label,cli_root,session_root,skill_root,resume_command,enabled FROM agent_instances").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| Ok(json!({"id":row.get::<_,String>(0)?,"agentId":row.get::<_,String>(1)?,"label":row.get::<_,String>(2)?,"cliRoot":row.get::<_,Option<String>>(3)?,"sessionRoot":row.get::<_,Option<String>>(4)?,"skillRoot":row.get::<_,Option<String>>(5)?,"resumeCommand":row.get::<_,Option<String>>(6)?,"enabled":row.get::<_,i64>(7)? != 0}))).map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

fn read_skill_installations(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn.prepare("SELECT id,skill_id,agent_id,target_id,scope,project_path,mode,status,installed_path,native_reference,error,updated_at FROM skill_installations").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| Ok(json!({"id":row.get::<_,String>(0)?,"skillId":row.get::<_,String>(1)?,"agentId":row.get::<_,String>(2)?,"targetId":row.get::<_,String>(3)?,"scope":row.get::<_,String>(4)?,"projectPath":row.get::<_,Option<String>>(5)?,"mode":row.get::<_,String>(6)?,"status":row.get::<_,String>(7)?,"installedPath":row.get::<_,Option<String>>(8)?,"nativeReference":row.get::<_,Option<String>>(9)?,"error":row.get::<_,Option<String>>(10)?,"updatedAt":row.get::<_,i64>(11)?}))).map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string())).collect()
}

pub(crate) fn save_store(store_path: &Path, store: &StoreState) -> Result<(), String> {
    let mut conn = open_store_db(store_path)?;
    let state_value = serde_json::to_value(store).map_err(|error| error.to_string())?;
    let state_object = state_value
        .as_object()
        .ok_or_else(|| "Desktop state must serialize to an object".to_string())?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM store_state", [])
        .map_err(|error| error.to_string())?;
    for (key, value) in state_object {
        if matches!(
            key.as_str(),
            "gatewayProviders" | "gatewayRoutes" | "agentInstances" | "skillInstallations"
        ) {
            continue;
        }
        let serialized = serde_json::to_string(value).map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO store_state (key, value) VALUES (?1, ?2)",
            params![key, serialized],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.execute("DELETE FROM gateway_routes", [])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM gateway_providers", [])
        .map_err(|e| e.to_string())?;
    for value in &store.gateway_providers {
        let id = value_id(value).ok_or_else(|| "gateway provider is missing id".to_string())?;
        let models =
            serde_json::to_string(&value.get("models").cloned().unwrap_or_else(|| json!([])))
                .map_err(|e| e.to_string())?;
        let (protocol, api_format) = gateway_protocol_for_storage(
            value
                .get("protocol")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        tx.execute("INSERT INTO gateway_providers (id,name,protocol,base_url,api_key,models_json,enabled,created_at,updated_at,api_format) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![id, value.get("name").and_then(Value::as_str).unwrap_or_default(), protocol, value.get("baseUrl").and_then(Value::as_str).unwrap_or_default(), value.get("apiKey").and_then(Value::as_str).unwrap_or_default(), models, value.get("enabled").and_then(Value::as_bool).unwrap_or(true) as i64, value.get("createdAt").and_then(Value::as_str).unwrap_or_default(), value.get("updatedAt").and_then(Value::as_str).unwrap_or_default(), api_format]).map_err(|e| e.to_string())?;
    }
    for value in &store.gateway_routes {
        let id = value_id(value).ok_or_else(|| "gateway route is missing id".to_string())?;
        let (protocol, api_format) = gateway_protocol_for_storage(
            value
                .get("protocol")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        tx.execute("INSERT INTO gateway_routes (id,alias,protocol,provider_id,upstream_model,created_at,updated_at,api_format) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![id, value.get("alias").and_then(Value::as_str).unwrap_or_default(), protocol, value.get("providerId").and_then(Value::as_str).unwrap_or_default(), value.get("upstreamModel").and_then(Value::as_str).unwrap_or_default(), value.get("createdAt").and_then(Value::as_str).unwrap_or_default(), value.get("updatedAt").and_then(Value::as_str).unwrap_or_default(), api_format]).map_err(|e| e.to_string())?;
    }
    tx.execute("DELETE FROM agent_instances", [])
        .map_err(|e| e.to_string())?;
    for value in &store.agent_instances {
        let id = value_id(value).ok_or_else(|| "agent instance is missing id".to_string())?;
        tx.execute("INSERT INTO agent_instances (id,agent_id,label,cli_root,session_root,skill_root,resume_command,enabled) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![id,value.get("agentId").and_then(Value::as_str).unwrap_or_default(),value.get("label").and_then(Value::as_str).unwrap_or_default(),value.get("cliRoot").and_then(Value::as_str),value.get("sessionRoot").and_then(Value::as_str),value.get("skillRoot").and_then(Value::as_str),value.get("resumeCommand").and_then(Value::as_str),value.get("enabled").and_then(Value::as_bool).unwrap_or(true) as i64]).map_err(|e| e.to_string())?;
    }
    tx.execute("DELETE FROM skill_installations", [])
        .map_err(|e| e.to_string())?;
    for value in &store.skill_installations {
        let id = value_id(value).ok_or_else(|| "skill installation is missing id".to_string())?;
        tx.execute("INSERT INTO skill_installations (id,skill_id,agent_id,target_id,scope,project_path,mode,status,installed_path,native_reference,error,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![id,value.get("skillId").and_then(Value::as_str).unwrap_or_default(),value.get("agentId").and_then(Value::as_str).unwrap_or_default(),value.get("targetId").and_then(Value::as_str).unwrap_or_default(),value.get("scope").and_then(Value::as_str).unwrap_or_default(),value.get("projectPath").and_then(Value::as_str),value.get("mode").and_then(Value::as_str).unwrap_or_default(),value.get("status").and_then(Value::as_str).unwrap_or_default(),value.get("installedPath").and_then(Value::as_str),value.get("nativeReference").and_then(Value::as_str),value.get("error").and_then(Value::as_str),value.get("updatedAt").and_then(Value::as_i64).unwrap_or_default()]).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

pub(crate) fn find_server_mut<'a>(
    store: &'a mut StoreState,
    id: &str,
) -> Result<&'a mut Value, String> {
    store
        .servers
        .iter_mut()
        .find(|server| value_id(server) == Some(id))
        .ok_or_else(|| format!("Server not found: {id}"))
}

pub(crate) fn find_entity_mut<'a>(
    items: &'a mut [Value],
    id: &str,
) -> Result<&'a mut Value, String> {
    items
        .iter_mut()
        .find(|item| value_id(item) == Some(id))
        .ok_or_else(|| format!("Entity not found: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_state_dir() -> PathBuf {
        std::env::temp_dir().join(format!("mcp-link-state-{}", Uuid::new_v4()))
    }

    #[test]
    fn store_round_trips_through_sqlite() {
        let dir = temp_state_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let db_path = dir.join("mcp.db");
        let mut store = create_default_state();
        store.servers.push(json!({
            "id": "server-1",
            "name": "Server 1",
            "serverType": "local",
            "args": [],
            "env": {}
        }));

        save_store(&db_path, &store).expect("state should save to sqlite");
        let loaded = load_store(&db_path)
            .expect("state should load from sqlite")
            .expect("state should exist");

        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(value_id(&loaded.servers[0]), Some("server-1"));
        assert!(db_path.exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn server_keeps_default_password_until_user_changes_it() {
        let dir = temp_state_dir();
        fs::create_dir_all(&dir).unwrap();
        let state = DesktopState::load(dir.join("mcp.db"));
        assert_eq!(state.server_password(), "admin");

        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn gateway_storage_preserves_legacy_openai_and_adds_api_format() {
        let dir = temp_state_dir();
        fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("mcp.db");
        let legacy = Connection::open(&db_path).unwrap();
        legacy
            .execute_batch(
                "PRAGMA user_version = 2;
                 CREATE TABLE gateway_providers (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL,
                    protocol TEXT NOT NULL CHECK(protocol IN ('openai','anthropic')),
                    base_url TEXT NOT NULL, api_key TEXT NOT NULL DEFAULT '',
                    models_json TEXT NOT NULL DEFAULT '[]', enabled INTEGER NOT NULL DEFAULT 1,
                    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
                 );
                 CREATE TABLE gateway_routes (
                    id TEXT PRIMARY KEY, alias TEXT NOT NULL,
                    protocol TEXT NOT NULL CHECK(protocol IN ('openai','anthropic')),
                    provider_id TEXT NOT NULL, upstream_model TEXT NOT NULL,
                    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
                 );
                 INSERT INTO gateway_providers VALUES
                    ('p','Legacy','openai','https://example.test/v1','key','[\"m\"]',1,'a','b');
                 INSERT INTO gateway_routes VALUES
                    ('r','alias','openai','p','m','a','b');",
            )
            .unwrap();
        drop(legacy);

        let conn = open_store_db(&db_path).unwrap();
        let providers = read_gateway_providers(&conn).unwrap();
        let routes = read_gateway_routes(&conn).unwrap();
        assert_eq!(providers[0]["protocol"], "openai-compatible");
        assert_eq!(routes[0]["protocol"], "openai-compatible");
        conn.execute(
            "UPDATE gateway_providers SET api_format='responses' WHERE id='p'",
            [],
        )
        .unwrap();
        assert_eq!(
            read_gateway_providers(&conn).unwrap()[0]["protocol"],
            "openai-responses"
        );
        drop(conn);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn gateway_protocol_storage_mapping_covers_all_public_formats() {
        assert_eq!(
            gateway_protocol_for_storage("openai-compatible"),
            ("openai", "chat-completions")
        );
        assert_eq!(
            gateway_protocol_for_storage("openai-responses"),
            ("openai", "responses")
        );
        assert_eq!(
            gateway_protocol_for_storage("anthropic"),
            ("anthropic", "messages")
        );
    }
}
