use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

#[allow(dead_code)]
const SENTINEL_PATH: &str = "";
const MAX_SNAPSHOT_BYTES: i64 = 500 * 1024 * 1024;
const KEEP_SESSIONS: usize = 30;

#[derive(Clone)]
pub struct TurnFileSnapshot {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub file_path: String,
    pub content: Vec<u8>,
    pub content_hash: String,
    pub snapshot_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshotRow {
    pub message_id: String,
    pub workspace_id: String,
    pub file_path: String,
    pub content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSnapshotRow {
    pub session_id: String,
    pub message_id: String,
    pub workspace_id: String,
    pub file_path: String,
    pub content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEntry {
    pub file_path: String,
    pub content: String,
}

pub fn snapshot_exists(
    db: &Connection,
    message_id: &str,
    workspace_id: &str,
    file_path: &str,
) -> Result<bool, String> {
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM turn_file_snapshots
             WHERE message_id=?1 AND workspace_id=?2 AND file_path=?3",
            params![message_id, workspace_id, file_path],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(count > 0)
}

#[allow(dead_code)]
pub fn last_snapshot_hash(
    db: &Connection,
    workspace_id: &str,
    file_path: &str,
) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT content_hash FROM turn_file_snapshots
         WHERE workspace_id=?1 AND file_path=?2
         ORDER BY snapshot_at DESC, id DESC LIMIT 1",
        params![workspace_id, file_path],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn insert_turn_snapshot(db: &Connection, row: &TurnFileSnapshot) -> Result<(), String> {
    db.execute(
        "INSERT OR REPLACE INTO turn_file_snapshots
         (id, message_id, session_id, workspace_id, file_path, content, content_hash, snapshot_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            row.id,
            row.message_id,
            row.session_id,
            row.workspace_id,
            row.file_path,
            row.content,
            row.content_hash,
            row.snapshot_at
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[allow(dead_code)]
pub fn insert_turn_marker(
    db: &Connection,
    message_id: &str,
    session_id: &str,
    workspace_id: &str,
    snapshot_at: i64,
) -> Result<(), String> {
    insert_turn_snapshot(
        db,
        &TurnFileSnapshot {
            id: Uuid::new_v4().to_string(),
            message_id: message_id.into(),
            session_id: session_id.into(),
            workspace_id: workspace_id.into(),
            file_path: SENTINEL_PATH.into(),
            content: Vec::new(),
            content_hash: String::new(),
            snapshot_at,
        },
    )
}

pub fn session_snapshot_message_ids(
    db: &Connection,
    session_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = db
        .prepare(
            "SELECT message_id FROM turn_file_snapshots
             WHERE session_id=?1
             GROUP BY message_id
             ORDER BY MIN(snapshot_at), message_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn decompress_utf8(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    let raw = zstd::decode_all(bytes).map_err(|error| error.to_string())?;
    String::from_utf8(raw).map_err(|error| error.to_string())
}

fn compress_utf8(content: &str) -> Result<Vec<u8>, String> {
    if content.is_empty() {
        return Ok(Vec::new());
    }
    zstd::bulk::compress(content.as_bytes(), 3).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileBaselineRow {
    pub workspace_id: String,
    pub file_path: String,
    pub content: String,
}

/// 文件还没有待审阅 baseline 时写入。已有 baseline 不覆盖，后续编辑叠在同一份原文上。
pub fn ensure_review_baseline(
    db: &Connection,
    workspace_id: &str,
    file_path: &str,
    content: &str,
) -> Result<bool, String> {
    ensure_review_baseline_compressed(db, workspace_id, file_path, &compress_utf8(content)?)
}

/// 同 [`ensure_review_baseline`]，但内容已经在锁外压缩好。
pub fn ensure_review_baseline_compressed(
    db: &Connection,
    workspace_id: &str,
    file_path: &str,
    compressed: &[u8],
) -> Result<bool, String> {
    let existing: Option<i64> = db
        .query_row(
            "SELECT updated_at FROM file_review_baselines
             WHERE workspace_id=?1 AND file_path=?2",
            params![workspace_id, file_path],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if existing.is_some() {
        return Ok(false);
    }
    save_compressed_baseline(db, workspace_id, file_path, compressed)?;
    Ok(true)
}

pub fn save_review_baseline(
    db: &Connection,
    workspace_id: &str,
    file_path: &str,
    content: &str,
) -> Result<(), String> {
    save_compressed_baseline(db, workspace_id, file_path, &compress_utf8(content)?)
}

fn save_compressed_baseline(
    db: &Connection,
    workspace_id: &str,
    file_path: &str,
    compressed: &[u8],
) -> Result<(), String> {
    db.execute(
        "INSERT INTO file_review_baselines (workspace_id, file_path, content, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(workspace_id, file_path) DO UPDATE SET
           content=excluded.content,
           updated_at=excluded.updated_at",
        params![
            workspace_id,
            file_path,
            compressed,
            chrono::Utc::now().timestamp()
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_review_baseline(
    db: &Connection,
    workspace_id: &str,
    file_path: &str,
) -> Result<(), String> {
    db.execute(
        "DELETE FROM file_review_baselines WHERE workspace_id=?1 AND file_path=?2",
        params![workspace_id, file_path],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn load_review_baselines(db: &Connection) -> Result<Vec<FileBaselineRow>, String> {
    let mut statement = db
        .prepare(
            "SELECT workspace_id, file_path, content FROM file_review_baselines
             ORDER BY file_path, workspace_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    rows.into_iter()
        .map(|(workspace_id, file_path, content)| {
            Ok(FileBaselineRow {
                workspace_id,
                file_path,
                content: decompress_utf8(&content)?,
            })
        })
        .collect()
}

fn stored_rows_for_session(
    db: &Connection,
    session_id: &str,
) -> Result<Vec<TurnFileSnapshot>, String> {
    let mut statement = db
        .prepare(
            "SELECT id, message_id, session_id, workspace_id, file_path, content, content_hash, snapshot_at
             FROM turn_file_snapshots
             WHERE session_id=?1
             ORDER BY snapshot_at, message_id, file_path",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok(TurnFileSnapshot {
                id: row.get(0)?,
                message_id: row.get(1)?,
                session_id: row.get(2)?,
                workspace_id: row.get(3)?,
                file_path: row.get(4)?,
                content: row.get(5)?,
                content_hash: row.get(6)?,
                snapshot_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// 只返回各轮实际存储的快照，不把早前轮次继承到后续轮次。
pub fn load_session_snapshots(
    db: &Connection,
    session_id: &str,
) -> Result<Vec<SessionSnapshotRow>, String> {
    let stored = stored_rows_for_session(db, session_id)?;
    let mut out = Vec::new();
    for row in stored {
        if row.file_path.is_empty() {
            continue;
        }
        out.push(SessionSnapshotRow {
            message_id: row.message_id,
            workspace_id: row.workspace_id,
            file_path: row.file_path,
            content: decompress_utf8(&row.content)?,
        });
    }
    Ok(out)
}

/// 快照 `tfs` 还有待 Accept/Undo：消息还在，且没有块状态（还没审阅过）或仍有 pending 块。
/// 两个子查询都走 diff_chunk_states 的唯一索引 (message_id, workspace_id, file_path, chunk_key)。
const UNRESOLVED_SNAPSHOT: &str = "tfs.file_path != ''
    AND EXISTS (SELECT 1 FROM messages m WHERE m.id = tfs.message_id)
    AND (
      NOT EXISTS (
        SELECT 1 FROM diff_chunk_states d
        WHERE d.message_id = tfs.message_id AND d.workspace_id = tfs.workspace_id
          AND d.file_path = tfs.file_path
      )
      OR EXISTS (
        SELECT 1 FROM diff_chunk_states d
        WHERE d.message_id = tfs.message_id AND d.workspace_id = tfs.workspace_id
          AND d.file_path = tfs.file_path AND d.status = 'pending'
      )
    )";

/// 每个仍有 Accept/Undo 的文件只取最早一版未处理完的快照。历史轮次留给 rollback 现查。
pub fn load_active_snapshots(db: &Connection) -> Result<Vec<ActiveSnapshotRow>, String> {
    // 先只按元数据排名，最后再按 id 取 content，不把 BLOB 带进排序。
    let mut statement = db
        .prepare(&format!(
            "WITH ranked AS (
                SELECT tfs.id,
                       ROW_NUMBER() OVER (
                         PARTITION BY tfs.workspace_id, tfs.file_path
                         ORDER BY tfs.snapshot_at, tfs.id
                       ) AS position
                FROM turn_file_snapshots tfs
                WHERE {UNRESOLVED_SNAPSHOT}
             )
             SELECT s.session_id, s.message_id, s.workspace_id, s.file_path, s.content
             FROM ranked
             JOIN turn_file_snapshots s ON s.id = ranked.id
             WHERE ranked.position = 1"
        ))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    rows.into_iter()
        .map(
            |(session_id, message_id, workspace_id, file_path, content)| {
                Ok(ActiveSnapshotRow {
                    session_id,
                    message_id,
                    workspace_id,
                    file_path,
                    content: decompress_utf8(&content)?,
                })
            },
        )
        .collect()
}

pub fn get_snapshot_content(
    db: &Connection,
    message_id: &str,
    workspace_id: &str,
    file_path: &str,
) -> Result<String, String> {
    if file_path.is_empty() {
        return Ok(String::new());
    }
    let exact: Option<Vec<u8>> = db
        .query_row(
            "SELECT content FROM turn_file_snapshots
             WHERE message_id=?1 AND workspace_id=?2 AND file_path=?3",
            params![message_id, workspace_id, file_path],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(bytes) = exact {
        return decompress_utf8(&bytes);
    }
    Err("没有该文件快照".to_string())
}

pub fn message_ids_from(
    db: &Connection,
    message_id: &str,
    session_id: &str,
) -> Result<Vec<String>, String> {
    let mut later_ids: Vec<String> = match db.query_row(
        "SELECT created_at, rowid FROM messages WHERE id=?1 AND session_id=?2",
        params![message_id, session_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    ) {
        Ok((created_at, rowid)) => {
            let mut statement = db
                .prepare(
                    "SELECT id FROM messages
                     WHERE session_id=?1
                       AND (created_at > ?2 OR (created_at = ?2 AND rowid >= ?3))",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params![session_id, created_at, rowid], |row| row.get(0))
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            rows
        }
        Err(_) => vec![message_id.to_string()],
    };
    if !later_ids.iter().any(|id| id == message_id) {
        later_ids.insert(0, message_id.to_string());
    }
    Ok(later_ids)
}

pub fn get_turn_snapshots(
    db: &Connection,
    message_id: &str,
    workspace_id: &str,
) -> Result<Vec<SnapshotEntry>, String> {
    let session_id: String = db
        .query_row(
            "SELECT session_id FROM turn_file_snapshots WHERE message_id=?1 LIMIT 1",
            [message_id],
            |row| row.get(0),
        )
        .or_else(|_| {
            db.query_row(
                "SELECT session_id FROM messages WHERE id=?1",
                [message_id],
                |row| row.get(0),
            )
        })
        .map_err(|_| "没有该轮快照".to_string())?;
    let later_ids = serde_json::to_string(&message_ids_from(db, message_id, &session_id)?)
        .map_err(|error| error.to_string())?;
    // 一次查询拿到所有后续轮次；每个文件取最早的一版（ROW_NUMBER 只排元数据，再回表取 content）。
    let mut statement = db
        .prepare(
            "WITH ranked AS (
                SELECT id,
                       ROW_NUMBER() OVER (PARTITION BY file_path ORDER BY snapshot_at, id) AS position
                FROM turn_file_snapshots
                WHERE session_id=?1 AND workspace_id=?2 AND file_path != ''
                  AND message_id IN (SELECT value FROM json_each(?3))
             )
             SELECT s.file_path, s.content
             FROM ranked JOIN turn_file_snapshots s ON s.id = ranked.id
             WHERE ranked.position = 1
             ORDER BY s.file_path",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![session_id, workspace_id, later_ids], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    rows.into_iter()
        .map(|(file_path, content)| {
            Ok(SnapshotEntry {
                file_path,
                content: decompress_utf8(&content)?,
            })
        })
        .collect()
}

pub fn delete_snapshots_from(
    db: &Connection,
    message_id: &str,
    workspace_id: &str,
    session_id: &str,
) -> Result<(), String> {
    let later_ids = message_ids_from(db, message_id, session_id)?;
    let tx = db
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    for later_id in later_ids {
        tx.execute(
            "DELETE FROM turn_file_snapshots
             WHERE session_id=?1 AND workspace_id=?2 AND message_id=?3",
            params![session_id, workspace_id, later_id],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "DELETE FROM diff_chunk_states
             WHERE session_id=?1 AND workspace_id=?2 AND message_id=?3",
            params![session_id, workspace_id, later_id],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(())
}

/// 超出最近 `KEEP_SESSIONS` 个会话或总量超过上限时删旧快照；还有待 Accept/Undo 的会话不删，
/// 否则用户还没审阅的修改会悄悄失去撤销能力。
pub fn cleanup_old_snapshots(db: &Connection) -> Result<(), String> {
    let mut sessions_stmt = db
        .prepare(&format!(
            "SELECT session_id, MAX(snapshot_at), COALESCE(SUM(LENGTH(content)), 0),
                    session_id IN (
                      SELECT DISTINCT tfs.session_id FROM turn_file_snapshots tfs
                      WHERE {UNRESOLVED_SNAPSHOT}
                    )
             FROM turn_file_snapshots
             GROUP BY session_id
             ORDER BY MAX(snapshot_at) DESC"
        ))
        .map_err(|error| error.to_string())?;
    let mut sessions = sessions_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut total: i64 = sessions.iter().map(|item| item.2).sum();
    let overflow = sessions.split_off(sessions.len().min(KEEP_SESSIONS));
    for (session_id, _, size, _) in overflow.into_iter().filter(|item| !item.3) {
        delete_session_snapshots(db, &session_id)?;
        total -= size;
    }
    if total <= MAX_SNAPSHOT_BYTES {
        return Ok(());
    }
    sessions.sort_by_key(|item| item.1);
    for (session_id, _, size, _) in sessions.into_iter().filter(|item| !item.3) {
        if total <= MAX_SNAPSHOT_BYTES {
            break;
        }
        delete_session_snapshots(db, &session_id)?;
        total -= size;
    }
    Ok(())
}

fn delete_session_snapshots(db: &Connection, session_id: &str) -> Result<(), String> {
    db.execute(
        "DELETE FROM turn_file_snapshots WHERE session_id=?1",
        [session_id],
    )
    .map_err(|error| error.to_string())?;
    db.execute(
        "DELETE FROM diff_chunk_states WHERE session_id=?1",
        [session_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        delete_snapshots_from, insert_turn_snapshot, load_active_snapshots, TurnFileSnapshot,
    };
    use crate::diff_state_db;
    use rusqlite::Connection;

    fn test_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute(
            "INSERT INTO chat_sessions (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('sess', '测试', 'agent', 'frontend', 't0', 't0')",
            [],
        )
        .unwrap();
        db
    }

    fn insert_message(db: &Connection, message_id: &str, created_at: &str) {
        db.execute(
            "INSERT INTO messages (id, session_id, role, content, created_at)
             VALUES (?1, 'sess', 'user', 'q', ?2)",
            [message_id, created_at],
        )
        .unwrap();
    }

    fn insert_file(
        db: &Connection,
        message_id: &str,
        file_path: &str,
        content: &str,
        snapshot_at: i64,
    ) {
        let compressed = zstd::bulk::compress(content.as_bytes(), 3).unwrap();
        insert_turn_snapshot(
            db,
            &TurnFileSnapshot {
                id: format!("{message_id}:{file_path}"),
                message_id: message_id.into(),
                session_id: "sess".into(),
                workspace_id: "ws".into(),
                file_path: file_path.into(),
                content: compressed,
                content_hash: snapshot_at.to_string(),
                snapshot_at,
            },
        )
        .unwrap();
    }

    #[test]
    fn t_r04_deletes_later_turns_and_chunk_states() {
        let db = test_db();
        insert_message(&db, "turn1", "t1");
        insert_message(&db, "turn2", "t2");
        insert_message(&db, "turn3", "t3");
        insert_message(&db, "turn4", "t4");
        insert_file(&db, "turn1", "a.ts", "v1", 100);
        insert_file(&db, "turn2", "b.ts", "v2", 200);
        insert_file(&db, "turn3", "c.ts", "v3", 300);
        insert_file(&db, "turn4", "d.ts", "v4", 400);
        for (message_id, key) in [
            ("turn1", "0"),
            ("turn2", "0"),
            ("turn3", "0"),
            ("turn4", "0"),
        ] {
            diff_state_db::upsert_chunk_state(
                &db, message_id, "sess", "ws", "file.ts", key, "pending",
            )
            .unwrap();
        }

        delete_snapshots_from(&db, "turn2", "ws", "sess").unwrap();

        let remaining: Vec<String> = {
            let mut statement = db
                .prepare(
                    "SELECT message_id FROM turn_file_snapshots
                     WHERE file_path != '' ORDER BY snapshot_at",
                )
                .unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert_eq!(remaining, vec!["turn1".to_string()]);

        let states: Vec<String> = {
            let mut statement = db
                .prepare("SELECT message_id FROM diff_chunk_states ORDER BY message_id")
                .unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert_eq!(states, vec!["turn1".to_string()]);
    }

    #[test]
    fn delete_without_target_snapshot_keeps_earlier_turns() {
        let db = test_db();
        insert_message(&db, "turn1", "t1");
        insert_message(&db, "turn2", "t2");
        insert_file(&db, "turn1", "a.ts", "v1", 100);

        delete_snapshots_from(&db, "turn2", "ws", "sess").unwrap();

        let remaining: Vec<String> = {
            let mut statement = db
                .prepare("SELECT message_id FROM turn_file_snapshots WHERE file_path != ''")
                .unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert_eq!(remaining, vec!["turn1".to_string()]);
    }

    #[test]
    fn load_active_snapshots_only_keeps_unresolved_file_baselines() {
        let db = test_db();
        insert_message(&db, "turn1", "t1");
        insert_message(&db, "turn2", "t2");
        insert_file(&db, "turn1", "a.ts", "A0", 100);
        insert_file(&db, "turn1", "b.ts", "B0", 100);
        insert_file(&db, "turn2", "b.ts", "B1", 200);
        diff_state_db::upsert_chunk_state(&db, "turn1", "sess", "ws", "b.ts", "0", "accepted")
            .unwrap();

        let active = load_active_snapshots(&db).unwrap();
        let by_file: std::collections::HashMap<_, _> = active
            .into_iter()
            .map(|row| (row.file_path, (row.message_id, row.content)))
            .collect();
        assert_eq!(
            by_file.get("a.ts").map(|(id, _)| id.as_str()),
            Some("turn1")
        );
        assert_eq!(by_file.get("b.ts"), Some(&("turn2".into(), "B1".into())));
    }

    #[test]
    fn load_session_snapshots_does_not_inherit_earlier_files() {
        let db = test_db();
        insert_message(&db, "turn1", "t1");
        insert_message(&db, "turn2", "t2");
        insert_file(&db, "turn1", "a.ts", "v0", 100);
        insert_file(&db, "turn2", "b.ts", "u0", 200);
        let rows = super::load_session_snapshots(&db, "sess").unwrap();
        let files: Vec<_> = rows
            .iter()
            .map(|row| (row.message_id.as_str(), row.file_path.as_str()))
            .collect();
        assert_eq!(files, vec![("turn1", "a.ts"), ("turn2", "b.ts")]);
    }

    #[test]
    fn get_turn_snapshots_rolls_back_later_files_to_earliest_baseline() {
        let db = test_db();
        insert_message(&db, "turn1", "t1");
        insert_message(&db, "turn2", "t2");
        insert_message(&db, "turn3", "t3");
        insert_file(&db, "turn1", "a.ts", "A0", 100);
        insert_file(&db, "turn2", "b.ts", "B0", 200);
        insert_file(&db, "turn3", "c.ts", "C0", 300);
        insert_file(&db, "turn3", "a.ts", "A1", 301);
        let rows = super::get_turn_snapshots(&db, "turn2", "ws").unwrap();
        let mut by_file: std::collections::HashMap<_, _> = rows
            .into_iter()
            .map(|row| (row.file_path, row.content))
            .collect();
        assert_eq!(by_file.remove("a.ts").as_deref(), Some("A1"));
        assert_eq!(by_file.remove("b.ts").as_deref(), Some("B0"));
        assert_eq!(by_file.remove("c.ts").as_deref(), Some("C0"));
        assert!(by_file.is_empty());
    }

    #[test]
    fn cleanup_keeps_old_sessions_with_pending_review() {
        let db = test_db();
        for index in 0..(super::KEEP_SESSIONS as i64 + 2) {
            insert_turn_snapshot(
                &db,
                &TurnFileSnapshot {
                    id: format!("snap-{index}"),
                    message_id: format!("msg-{index}"),
                    session_id: format!("s{index}"),
                    workspace_id: "ws".into(),
                    file_path: "a.ts".into(),
                    content: vec![1, 2, 3],
                    content_hash: String::new(),
                    snapshot_at: index,
                },
            )
            .unwrap();
        }
        insert_message(&db, "msg-0", "t0");
        insert_message(&db, "msg-1", "t1");
        diff_state_db::upsert_chunk_state(&db, "msg-1", "s1", "ws", "a.ts", "0", "accepted")
            .unwrap();

        super::cleanup_old_snapshots(&db).unwrap();

        let sessions: Vec<String> = {
            let mut statement = db
                .prepare("SELECT session_id FROM turn_file_snapshots WHERE session_id IN ('s0','s1')")
                .unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert_eq!(sessions, vec!["s0".to_string()]);
        let total: i64 = db
            .query_row("SELECT COUNT(*) FROM turn_file_snapshots", [], |row| row.get(0))
            .unwrap();
        assert_eq!(total, super::KEEP_SESSIONS as i64 + 1);
    }

    #[test]
    fn get_snapshot_content_errors_when_missing() {
        let db = test_db();
        insert_message(&db, "turn1", "t1");
        insert_file(&db, "turn1", "a.ts", "v0", 100);
        let error = super::get_snapshot_content(&db, "turn1", "ws", "missing.ts").unwrap_err();
        assert!(error.contains("没有该文件快照"));
    }
}
