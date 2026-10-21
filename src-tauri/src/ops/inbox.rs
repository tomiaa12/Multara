use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::models::{InboxInput, InboxItem};

const SELECT_ITEM: &str = "
SELECT i.id, i.session_id, i.project_id, i.task_id, i.title, i.preview, i.status, i.read,
       i.created_at, i.read_message_id,
       CASE WHEN i.read != 0 THEN 0 ELSE (
         SELECT COUNT(*) FROM messages m
         WHERE m.session_id = i.session_id
           AND (
             i.read_message_id IS NULL
             OR NOT EXISTS (SELECT 1 FROM messages anchor WHERE anchor.id = i.read_message_id)
             OR (m.created_at, m.id) > (
               SELECT anchor.created_at, anchor.id
               FROM messages anchor
               WHERE anchor.id = i.read_message_id
             )
           )
       ) END,
       i.archived_at
FROM inbox_items i
";

fn map_item(row: &Row) -> rusqlite::Result<InboxItem> {
    Ok(InboxItem {
        id: row.get(0)?,
        session_id: row.get(1)?,
        project_id: row.get(2)?,
        task_id: row.get(3)?,
        title: row.get(4)?,
        preview: row.get(5)?,
        status: row.get(6)?,
        read: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        read_message_id: row.get(9)?,
        unread_count: row.get(10)?,
        archived_at: row.get(11)?,
    })
}

pub fn list(db: &Connection) -> Result<Vec<InboxItem>, String> {
    let mut statement = db
        .prepare(&format!(
            "{SELECT_ITEM} WHERE i.archived_at IS NULL ORDER BY i.created_at DESC"
        ))
        .map_err(|error| error.to_string())?;
    let items = statement
        .query_map([], map_item)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(items)
}

pub fn mark_read(db: &Connection, id: &str, message_id: Option<&str>) -> Result<(), String> {
    let message_id = message_id.map(str::trim).filter(|value| !value.is_empty());
    db.execute(
        "UPDATE inbox_items
         SET read=1, read_message_id=COALESCE(?2, read_message_id)
         WHERE id=?1",
        params![id, message_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn mark_unread(db: &Connection, id: &str) -> Result<(), String> {
    db.execute(
        "UPDATE inbox_items SET read=0 WHERE id=?1",
        [id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn mark_all_read(db: &Connection) -> Result<(), String> {
    db.execute(
        "UPDATE inbox_items SET read=1, read_message_id=(
            SELECT m.id FROM messages m
            WHERE m.session_id = inbox_items.session_id
            ORDER BY m.created_at DESC, m.rowid DESC LIMIT 1
         )
         WHERE archived_at IS NULL AND read=0",
        [],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn archive(db: &Connection, id: &str) -> Result<(), String> {
    db.execute(
        "UPDATE inbox_items SET archived_at=?1, read=1 WHERE id=?2",
        params![Utc::now().to_rfc3339(), id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete(db: &Connection, id: &str) -> Result<(), String> {
    db.execute("DELETE FROM inbox_items WHERE id=?1", [id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn upsert(db: &Connection, input: InboxInput) -> Result<InboxItem, String> {
    if !matches!(
        input.status.as_str(),
        "completed" | "failed" | "stopped" | "needs_input" | "needs_permission"
    ) {
        return Err("无效的收件箱状态".to_string());
    }
    let title = input.title.trim();
    if title.is_empty() {
        return Err("收件箱标题不能为空".to_string());
    }
    let preview = shorten_preview(&input.preview);
    let now = Utc::now().to_rfc3339();
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM inbox_items WHERE session_id=?1",
            [&input.session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());
    db.execute(
        "INSERT INTO inbox_items
         (id, session_id, project_id, task_id, title, preview, status, read, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)
         ON CONFLICT(session_id) DO UPDATE SET
           project_id=excluded.project_id, task_id=excluded.task_id, title=excluded.title,
           preview=excluded.preview, status=excluded.status, read=0, created_at=excluded.created_at,
           archived_at=NULL",
        params![
            id,
            input.session_id,
            input.project_id.as_deref().map(str::trim).filter(|value| !value.is_empty()),
            input.task_id.as_deref().map(str::trim).filter(|value| !value.is_empty()),
            title,
            preview,
            input.status,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    db.query_row(
        &format!("{SELECT_ITEM} WHERE i.id=?1"),
        [&id],
        map_item,
    )
    .map_err(|error| error.to_string())
}

pub fn shorten_preview(source: &str) -> String {
    let preview = source.replace('\n', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    if preview.is_empty() {
        return "对话已结束".to_string();
    }
    let shortened: String = preview.chars().take(120).collect();
    if shortened.chars().count() < preview.chars().count() {
        format!("{shortened}…")
    } else {
        shortened
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{list, mark_read, upsert};
    use crate::models::InboxInput;

    fn setup() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute(
            "INSERT INTO chat_sessions
             (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('session-1', '对话', 'agent', 'miku', 't0', 't0')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO messages (id, session_id, role, content, created_at)
             VALUES ('m1', 'session-1', 'user', '第一问', 't1'),
                    ('m2', 'session-1', 'assistant', '第一答', 't2'),
                    ('m3', 'session-1', 'user', '第二问', 't3')",
            [],
        )
        .unwrap();
        db
    }

    #[test]
    fn unread_count_starts_after_the_read_cursor() {
        let db = setup();
        let created = upsert(
            &db,
            InboxInput {
                session_id: "session-1".to_string(),
                project_id: None,
                task_id: None,
                title: "对话".to_string(),
                preview: "第一答".to_string(),
                status: "completed".to_string(),
            },
        )
        .unwrap();
        assert_eq!(created.unread_count, 3);
        assert!(created.read_message_id.is_none());

        mark_read(&db, &created.id, Some("m1")).unwrap();
        let after_read = list(&db).unwrap();
        assert_eq!(after_read[0].unread_count, 0);
        assert_eq!(after_read[0].read_message_id.as_deref(), Some("m1"));

        let again = upsert(
            &db,
            InboxInput {
                session_id: "session-1".to_string(),
                project_id: None,
                task_id: None,
                title: "对话".to_string(),
                preview: "又有新回复".to_string(),
                status: "completed".to_string(),
            },
        )
        .unwrap();
        assert!(!again.read);
        assert_eq!(again.read_message_id.as_deref(), Some("m1"));
        assert_eq!(again.unread_count, 2);
    }
}
