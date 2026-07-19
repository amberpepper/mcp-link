use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::state::DesktopState;

use super::model::SessionSummary;

pub(crate) fn index_path(state: &DesktopState) -> PathBuf {
    state
        .store_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("session-index.sqlite")
}

fn open(state: &DesktopState) -> Result<Connection, String> {
    let path = index_path(state);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let conn = Connection::open(path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS agent_session_index (
           agent_id TEXT NOT NULL,
           native_id TEXT NOT NULL,
           native_session_id TEXT,
           source_instance_id TEXT,
           source_label TEXT,
           title TEXT NOT NULL,
           cwd TEXT,
           repository TEXT,
           model TEXT,
           created_at INTEGER,
           updated_at INTEGER,
           message_count INTEGER NOT NULL,
           source_ref TEXT NOT NULL,
           parent_native_id TEXT,
           search_text TEXT NOT NULL,
           indexed_at INTEGER NOT NULL,
           PRIMARY KEY (agent_id, native_id)
         );
         CREATE INDEX IF NOT EXISTS idx_agent_session_updated
           ON agent_session_index(updated_at DESC);
         CREATE INDEX IF NOT EXISTS idx_agent_session_agent
           ON agent_session_index(agent_id, updated_at DESC);
         CREATE INDEX IF NOT EXISTS idx_agent_session_source
           ON agent_session_index(agent_id, source_ref);
         CREATE TABLE IF NOT EXISTS agent_session_messages (
           agent_id TEXT NOT NULL,
           native_id TEXT NOT NULL,
           sequence INTEGER NOT NULL,
           data TEXT NOT NULL,
           PRIMARY KEY (agent_id, native_id, sequence)
         );",
    )
    .map_err(|error| error.to_string())?;
    ensure_column(&conn, "agent_session_index", "native_session_id", "TEXT")?;
    ensure_column(&conn, "agent_session_index", "source_instance_id", "TEXT")?;
    ensure_column(&conn, "agent_session_index", "source_label", "TEXT")?;
    Ok(conn)
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    column_type: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .map_err(|error| error.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if !columns.iter().any(|candidate| candidate == column) {
        conn.execute(
            &format!("ALTER TABLE \"{table}\" ADD COLUMN \"{column}\" {column_type}"),
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn needs_refresh(
    state: &DesktopState,
    summary: &SessionSummary,
) -> Result<bool, String> {
    let conn = open(state)?;
    let existing = conn
        .query_row(
            "SELECT updated_at, source_ref, title, cwd, source_label FROM agent_session_index
             WHERE agent_id = ?1 AND native_id = ?2",
            params![summary.agent_id, summary.native_id],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(
        existing.is_none_or(|(updated_at, source_ref, title, cwd, source_label)| {
            updated_at != summary.updated_at
                || source_ref != summary.source_ref
                || title != summary.title
                || cwd != summary.cwd
                || source_label != summary.source_label
        }),
    )
}

pub(crate) fn upsert_summary(state: &DesktopState, summary: &SessionSummary) -> Result<(), String> {
    let mut conn = open(state)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let search_text = build_search_text(summary);
    tx.execute(
        "INSERT INTO agent_session_index (
           agent_id, native_id, native_session_id, source_instance_id, source_label,
           title, cwd, repository, model, created_at, updated_at,
           message_count, source_ref, parent_native_id, search_text, indexed_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
         ON CONFLICT(agent_id,native_id) DO UPDATE SET
           native_session_id=excluded.native_session_id,
           source_instance_id=excluded.source_instance_id,
           source_label=excluded.source_label,
           title=excluded.title, cwd=excluded.cwd, repository=excluded.repository,
           model=excluded.model, created_at=excluded.created_at, updated_at=excluded.updated_at,
           message_count=excluded.message_count, source_ref=excluded.source_ref,
           parent_native_id=excluded.parent_native_id, search_text=excluded.search_text,
           indexed_at=excluded.indexed_at",
        params![
            summary.agent_id,
            summary.native_id,
            summary.native_session_id,
            summary.source_instance_id,
            summary.source_label,
            summary.title,
            summary.cwd,
            summary.repository,
            summary.model,
            summary.created_at,
            summary.updated_at,
            summary.message_count as i64,
            summary.source_ref,
            summary.parent_native_id,
            search_text,
            chrono::Utc::now().timestamp_millis(),
        ],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM agent_session_messages WHERE agent_id = ?1 AND native_id = ?2",
        params![summary.agent_id, summary.native_id],
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}

pub(crate) fn remove_missing(
    state: &DesktopState,
    agent_id: &str,
    native_ids: &[String],
) -> Result<(), String> {
    let mut conn = open(state)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let existing = {
        let mut stmt = tx
            .prepare("SELECT native_id FROM agent_session_index WHERE agent_id = ?1")
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([agent_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    for native_id in existing {
        if native_ids.iter().any(|candidate| candidate == &native_id) {
            continue;
        }
        tx.execute(
            "DELETE FROM agent_session_messages WHERE agent_id = ?1 AND native_id = ?2",
            params![agent_id, native_id],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "DELETE FROM agent_session_index WHERE agent_id = ?1 AND native_id = ?2",
            params![agent_id, native_id],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

pub(crate) fn remove_unknown_agents(
    state: &DesktopState,
    known_agents: &[String],
) -> Result<(), String> {
    let mut conn = open(state)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let indexed_agents = {
        let mut stmt = tx
            .prepare("SELECT DISTINCT agent_id FROM agent_session_index")
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    for agent_id in indexed_agents {
        if known_agents.iter().any(|known| known == &agent_id) {
            continue;
        }
        tx.execute(
            "DELETE FROM agent_session_messages WHERE agent_id = ?1",
            [&agent_id],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "DELETE FROM agent_session_index WHERE agent_id = ?1",
            [&agent_id],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

pub(crate) fn delete_indexed_session(
    state: &DesktopState,
    agent_id: &str,
    native_id: &str,
) -> Result<(), String> {
    let mut conn = open(state)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM agent_session_messages WHERE agent_id = ?1 AND native_id = ?2",
        params![agent_id, native_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM agent_session_index WHERE agent_id = ?1 AND native_id = ?2",
        params![agent_id, native_id],
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}

pub(crate) fn rename_indexed_session(
    state: &DesktopState,
    agent_id: &str,
    native_id: &str,
    title: &str,
) -> Result<(), String> {
    let conn = open(state)?;
    conn.execute(
        "UPDATE agent_session_index
         SET title = ?3,
             search_text = ?3 || char(10)
                || COALESCE(cwd, '') || char(10)
                || COALESCE(repository, '') || char(10)
                || COALESCE(model, '') || char(10)
                || COALESCE(native_session_id, ''),
             indexed_at = ?4
         WHERE agent_id = ?1 AND native_id = ?2",
        params![
            agent_id,
            native_id,
            title,
            chrono::Utc::now().timestamp_millis()
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn delete_instance_sessions(
    state: &DesktopState,
    agent_id: &str,
    instance_id: &str,
) -> Result<(), String> {
    let mut conn = open(state)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let native_ids = {
        let mut stmt = tx
            .prepare(
                "SELECT native_id FROM agent_session_index
                 WHERE agent_id = ?1 AND source_instance_id = ?2",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![agent_id, instance_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    for native_id in native_ids {
        tx.execute(
            "DELETE FROM agent_session_messages WHERE agent_id = ?1 AND native_id = ?2",
            params![agent_id, native_id],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "DELETE FROM agent_session_index WHERE agent_id = ?1 AND native_id = ?2",
            params![agent_id, native_id],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

pub(crate) fn query(
    state: &DesktopState,
    agent_id: Option<&str>,
    query: Option<&str>,
    cwd: Option<&str>,
    limit: usize,
) -> Result<Vec<SessionSummary>, String> {
    let conn = open(state)?;
    let query_pattern = query.map(|value| format!("%{}%", value.to_ascii_lowercase()));
    let cwd_pattern = cwd.map(|value| format!("%{}%", value.to_ascii_lowercase()));
    let mut stmt = conn
        .prepare(
            "SELECT agent_id, native_id, native_session_id, source_instance_id, source_label,
                    title, cwd, repository, model, created_at, updated_at,
                    message_count, source_ref, parent_native_id
             FROM agent_session_index
             WHERE (?1 IS NULL OR agent_id = ?1)
               AND (?2 IS NULL OR lower(search_text) LIKE ?2)
               AND (?3 IS NULL OR lower(COALESCE(cwd, '')) LIKE ?3)
             ORDER BY updated_at DESC
             LIMIT ?4",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(
            params![agent_id, query_pattern, cwd_pattern, limit as i64],
            |row| {
                let agent_id: String = row.get(0)?;
                let native_id: String = row.get(1)?;
                Ok(SessionSummary {
                    id: format!("{agent_id}:{native_id}"),
                    agent_id,
                    native_id,
                    native_session_id: row.get(2)?,
                    source_instance_id: row.get(3)?,
                    source_label: row.get(4)?,
                    title: row.get(5)?,
                    cwd: row.get(6)?,
                    repository: row.get(7)?,
                    model: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    message_count: row.get::<_, i64>(11)? as usize,
                    source_ref: row.get(12)?,
                    parent_native_id: row.get(13)?,
                    active: false,
                })
            },
        )
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn build_search_text(summary: &SessionSummary) -> String {
    [
        summary.title.as_str(),
        summary.cwd.as_deref().unwrap_or_default(),
        summary.repository.as_deref().unwrap_or_default(),
        summary.model.as_deref().unwrap_or_default(),
        summary.native_session_id.as_deref().unwrap_or_default(),
    ]
    .join("\n")
}
