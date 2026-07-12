use std::{collections::HashMap, fs, path::Path};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccessKeySummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) key_prefix: String,
    pub(crate) server_access: HashMap<String, bool>,
    pub(crate) created_at: String,
    pub(crate) last_used_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AccessKeyContext {
    #[cfg_attr(not(feature = "server"), allow(dead_code))]
    pub(crate) id: String,
    pub(crate) server_access: HashMap<String, bool>,
}

impl AccessKeyContext {
    pub(crate) fn allows_server(&self, server_id: &str) -> bool {
        self.server_access.get(server_id).copied() == Some(true)
    }
}

pub(crate) fn list_access_keys(db_path: &Path) -> Result<Vec<AccessKeySummary>, String> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, name, key_prefix, server_access, created_at, last_used_at
             FROM access_keys
             ORDER BY created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let server_access: String = row.get(3)?;
            Ok(AccessKeySummary {
                id: row.get(0)?,
                name: row.get(1)?,
                key_prefix: row.get(2)?,
                server_access: parse_server_access(&server_access),
                created_at: row.get(4)?,
                last_used_at: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(crate) fn generate_access_key(db_path: &Path, input: Option<&Value>) -> Result<String, String> {
    let conn = open_connection(db_path)?;
    let name = input
        .and_then(Value::as_object)
        .and_then(|input| input.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Access key");
    let server_access = input
        .and_then(Value::as_object)
        .and_then(|input| input.get("serverAccess"))
        .map(normalize_server_access)
        .unwrap_or_default();
    let token = format!(
        "mcpr_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let token_hash = hash_token(&token);
    let key_prefix = token.chars().take(12).collect::<String>();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO access_keys
         (id, name, key_prefix, token_hash, server_access, created_at, last_used_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
        params![
            Uuid::new_v4().to_string(),
            name,
            key_prefix,
            token_hash,
            server_access_to_json(&server_access)?,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(token)
}

pub(crate) fn revoke_access_key(db_path: &Path, id: &str) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    conn.execute("DELETE FROM access_keys WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn update_access_key_server_access(
    db_path: &Path,
    id: &str,
    server_access: &Value,
) -> Result<AccessKeySummary, String> {
    let conn = open_connection(db_path)?;
    let server_access = normalize_server_access(server_access);
    let server_access_json = server_access_to_json(&server_access)?;
    let changed = conn
        .execute(
            "UPDATE access_keys SET server_access = ?1 WHERE id = ?2",
            params![server_access_json, id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("Access key not found: {id}"));
    }

    get_access_key(&conn, id)
}

pub(crate) fn authenticate_access_key(
    db_path: &Path,
    token: &str,
) -> Result<Option<AccessKeyContext>, String> {
    let conn = open_connection(db_path)?;
    let token_hash = hash_token(token);
    let found = conn
        .query_row(
            "SELECT id, server_access FROM access_keys WHERE token_hash = ?1",
            params![token_hash],
            |row| {
                let server_access: String = row.get(1)?;
                Ok(AccessKeyContext {
                    id: row.get(0)?,
                    server_access: parse_server_access(&server_access),
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(context) = found {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE access_keys SET last_used_at = ?1 WHERE id = ?2",
            params![now, context.id],
        )
        .map_err(|error| error.to_string())?;
        Ok(Some(context))
    } else {
        Ok(None)
    }
}

pub(crate) fn migrate_access_keys_database(
    db_path: &Path,
    legacy_db_path: &Path,
) -> Result<(), String> {
    if db_path == legacy_db_path || !legacy_db_path.exists() {
        return Ok(());
    }

    let target = open_connection(db_path)?;
    let legacy = Connection::open(legacy_db_path).map_err(|error| error.to_string())?;
    let table_exists = legacy
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type = 'table' AND name = 'access_keys'
            )",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        != 0;
    if !table_exists {
        return Ok(());
    }

    let mut stmt = legacy
        .prepare(
            "SELECT id, name, key_prefix, token_hash, server_access, created_at, last_used_at
         FROM access_keys",
        )
        .map_err(|error| error.to_string())?;

    let keys = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut insert = target
        .prepare(
            "INSERT OR IGNORE INTO access_keys
             (id, name, key_prefix, token_hash, server_access, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .map_err(|error| error.to_string())?;

    for key in keys {
        let (id, name, key_prefix, token_hash, server_access, created_at, last_used_at) =
            key.map_err(|error| error.to_string())?;
        insert
            .execute(params![
                id,
                name,
                key_prefix,
                token_hash,
                server_access,
                created_at,
                last_used_at
            ])
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn get_access_key(conn: &Connection, id: &str) -> Result<AccessKeySummary, String> {
    conn.query_row(
        "SELECT id, name, key_prefix, server_access, created_at, last_used_at
         FROM access_keys
         WHERE id = ?1",
        params![id],
        |row| {
            let server_access: String = row.get(3)?;
            Ok(AccessKeySummary {
                id: row.get(0)?,
                name: row.get(1)?,
                key_prefix: row.get(2)?,
                server_access: parse_server_access(&server_access),
                created_at: row.get(4)?,
                last_used_at: row.get(5)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

fn open_connection(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let conn = Connection::open(db_path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS access_keys (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            key_prefix TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            server_access TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            last_used_at TEXT
        );",
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
}

fn normalize_server_access(value: &Value) -> HashMap<String, bool> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(server_id, allowed)| {
                    allowed
                        .as_bool()
                        .map(|allowed| (server_id.clone(), allowed))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_server_access(server_access: &str) -> HashMap<String, bool> {
    serde_json::from_str::<HashMap<String, bool>>(server_access).unwrap_or_default()
}

fn server_access_to_json(server_access: &HashMap<String, bool>) -> Result<String, String> {
    let object = server_access
        .iter()
        .map(|(server_id, allowed)| (server_id.clone(), json!(allowed)))
        .collect::<Map<_, _>>();
    serde_json::to_string(&Value::Object(object)).map_err(|error| error.to_string())
}

fn hash_token(token: &str) -> String {
    let hash = Sha256::digest(token.as_bytes());
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mcp-link-access-key-{}.sqlite", Uuid::new_v4()))
    }

    #[test]
    fn generated_key_is_listed_without_plaintext_and_can_authenticate() {
        let db_path = test_db_path();
        let input = json!({
            "name": "Backend",
            "serverAccess": {
                "server-a": true,
                "server-b": false
            }
        });

        let token = generate_access_key(&db_path, Some(&input)).expect("key should be generated");
        assert!(token.starts_with("mcpr_"));

        let keys = list_access_keys(&db_path).expect("keys should be listed");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "Backend");
        assert_eq!(
            keys[0].key_prefix,
            token.chars().take(12).collect::<String>()
        );
        assert_eq!(keys[0].server_access.get("server-a"), Some(&true));
        assert_eq!(keys[0].server_access.get("server-b"), Some(&false));

        let context = authenticate_access_key(&db_path, &token)
            .expect("auth should not fail")
            .expect("key should authenticate");
        assert!(context.allows_server("server-a"));
        assert!(!context.allows_server("server-b"));
        assert!(!context.allows_server("server-c"));

        let keys = list_access_keys(&db_path).expect("keys should be listed");
        assert!(keys[0].last_used_at.is_some());

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn revoked_key_no_longer_authenticates() {
        let db_path = test_db_path();
        let token = generate_access_key(&db_path, None).expect("key should be generated");
        let id = list_access_keys(&db_path).expect("keys should be listed")[0]
            .id
            .clone();

        revoke_access_key(&db_path, &id).expect("key should be revoked");

        let context = authenticate_access_key(&db_path, &token).expect("auth should not fail");
        assert!(context.is_none());

        let _ = fs::remove_file(db_path);
    }
}
