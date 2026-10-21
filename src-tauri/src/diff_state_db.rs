use rusqlite::{params, Connection};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkStateRow {
    pub chunk_id: String,
    pub status: String,
}

fn row_to_chunk_state(
    message_id: String,
    workspace_id: String,
    file_path: String,
    chunk_key: String,
    status: String,
) -> ChunkStateRow {
    ChunkStateRow {
        chunk_id: format!("{message_id}:{workspace_id}:{file_path}:{chunk_key}"),
        status,
    }
}

pub fn load_session_chunk_states(
    db: &Connection,
    session_id: &str,
) -> Result<Vec<ChunkStateRow>, String> {
    let mut statement = db
        .prepare(
            "SELECT message_id, workspace_id, file_path, chunk_key, status
             FROM diff_chunk_states WHERE session_id=?1",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok(row_to_chunk_state(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

pub fn load_all_chunk_states(db: &Connection) -> Result<Vec<ChunkStateRow>, String> {
    let mut statement = db
        .prepare(
            "SELECT message_id, workspace_id, file_path, chunk_key, status
             FROM diff_chunk_states",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(row_to_chunk_state(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

pub fn upsert_chunk_state(
    db: &Connection,
    message_id: &str,
    session_id: &str,
    workspace_id: &str,
    file_path: &str,
    chunk_key: &str,
    status: &str,
) -> Result<(), String> {
    if !matches!(status, "pending" | "accepted" | "undone") {
        return Err("无效的 chunk 状态".to_string());
    }
    if chunk_key.trim().is_empty() {
        return Err("无效的 chunk key".to_string());
    }
    let now = chrono::Utc::now().timestamp();
    db.execute(
        "INSERT INTO diff_chunk_states
         (id, message_id, session_id, workspace_id, file_path, chunk_key, status, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(message_id, workspace_id, file_path, chunk_key)
         DO UPDATE SET status=excluded.status, updated_at=excluded.updated_at",
        params![
            Uuid::new_v4().to_string(),
            message_id,
            session_id,
            workspace_id,
            file_path,
            chunk_key,
            status,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_chunk_states_for_message(db: &Connection, message_id: &str) -> Result<(), String> {
    db.execute(
        "DELETE FROM diff_chunk_states WHERE message_id=?1",
        [message_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
