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

use crate::access_keys::migrate_access_keys_database;
use crate::util::{
    json::{set_object_field, set_object_value, value_id},
    time::now_iso,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoreState {
    pub(crate) servers: Vec<Value>,
    pub(crate) settings: Map<String, Value>,
    pub(crate) workflows: Vec<Value>,
    pub(crate) hooks: Vec<Value>,
    pub(crate) skills: Vec<Value>,
}

pub(crate) struct DesktopState {
    pub(crate) store_path: PathBuf,
    pub(crate) store: Mutex<StoreState>,
    pub(crate) runtimes: Mutex<HashMap<String, DesktopRuntime>>,
    pub(crate) mcp_client_peers: Mutex<HashMap<String, Peer<RoleServer>>>,
    #[cfg(feature = "desktop")]
    pub(crate) mcp_endpoint: Mutex<Option<String>>,
    #[cfg(feature = "desktop")]
    pub(crate) mcp_server_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

pub(crate) struct DesktopRuntime {
    pub(crate) client: RunningService<RoleClient, ClientInfo>,
    pub(crate) pid: Option<u32>,
}

impl DesktopState {
    pub(crate) fn load(store_path: PathBuf) -> Self {
        let legacy_json_path = store_path.with_file_name("state.json");
        let legacy_access_keys_path = store_path.with_file_name("access-keys.sqlite");
        let store = match load_store(&store_path) {
            Ok(Some(store)) => store,
            Ok(None) => read_legacy_store(&legacy_json_path).unwrap_or_else(create_default_state),
            Err(error) => {
                eprintln!("Failed to load desktop SQLite state: {error}");
                read_legacy_store(&legacy_json_path).unwrap_or_else(create_default_state)
            }
        };
        let store = normalize_state(store);

        let state = Self {
            store_path,
            store: Mutex::new(store),
            runtimes: Mutex::new(HashMap::new()),
            mcp_client_peers: Mutex::new(HashMap::new()),
            #[cfg(feature = "desktop")]
            mcp_endpoint: Mutex::new(None),
            #[cfg(feature = "desktop")]
            mcp_server_task: Mutex::new(None),
        };

        let _ = state.save();
        let _ = migrate_access_keys_database(&state.store_path, &legacy_access_keys_path);
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
        settings: Map::from_iter([
            ("loadExternalMCPConfigs".to_string(), json!(true)),
            ("showWindowOnStartup".to_string(), json!(true)),
            ("theme".to_string(), json!("system")),
            ("language".to_string(), json!("zh")),
            ("desktopMcpListenHost".to_string(), json!("127.0.0.1")),
            ("desktopMcpListenPort".to_string(), json!(3284)),
            ("serverPassword".to_string(), json!("admin")),
        ]),
        workflows: vec![],
        hooks: vec![],
        skills: vec![],
    }
}

pub(crate) fn normalize_state(mut store: StoreState) -> StoreState {
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
    if let Some(parent) = store_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let conn = Connection::open(store_path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS store_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
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
        return Ok(None);
    }

    serde_json::from_value::<StoreState>(Value::Object(object))
        .map(Some)
        .map_err(|error| error.to_string())
}

fn read_legacy_store(store_path: &Path) -> Option<StoreState> {
    fs::read_to_string(store_path)
        .ok()
        .and_then(|body| serde_json::from_str::<StoreState>(&body).ok())
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
        let serialized = serde_json::to_string(value).map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO store_state (key, value) VALUES (?1, ?2)",
            params![key, serialized],
        )
        .map_err(|error| error.to_string())?;
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
    fn desktop_state_load_migrates_legacy_json_to_sqlite() {
        let dir = temp_state_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let legacy_json_path = dir.join("state.json");
        let db_path = dir.join("mcp.db");
        let mut store = create_default_state();
        store.settings.insert("language".into(), json!("en"));
        fs::write(
            &legacy_json_path,
            serde_json::to_string(&store).expect("state should serialize"),
        )
        .expect("legacy state should be written");

        let state = DesktopState::load(db_path.clone());
        let loaded = state.store.lock().expect("state should lock");

        assert_eq!(
            loaded.settings.get("language").and_then(Value::as_str),
            Some("en")
        );
        assert!(db_path.exists());
        assert!(load_store(&db_path)
            .expect("state should load from sqlite")
            .is_some());

        drop(loaded);
        let _ = fs::remove_dir_all(dir);
    }
}
