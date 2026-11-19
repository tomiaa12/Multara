use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::models::{MessageAttachmentInput, RunMode};

/// 前端输入框上方的会话排队消息；刷新 / 重启后从这里恢复。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedMessageRow {
    pub id: String,
    pub session_id: String,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<MessageAttachmentInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_mode: Option<RunMode>,
    #[serde(default)]
    pub priority: bool,
    #[serde(default)]
    pub immediate: bool,
    /// 与前端 `MessageSourceInput` 同形的 JSON
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<serde_json::Value>,
    /// 与前端 `FeishuRecipientInput[]` 同形的 JSON
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feishu_recipients: Option<serde_json::Value>,
    pub created_at: String,
}

pub fn list(db: &Connection) -> Result<Vec<QueuedMessageRow>, String> {
    let mut statement = db
        .prepare(
            "SELECT id, session_id, content, attachments_json, requested_mode, priority, immediate,
                    source_json, feishu_recipients_json, created_at
             FROM session_message_queue
             ORDER BY position ASC, rowid ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    rows.into_iter()
        .map(
            |(
                id,
                session_id,
                content,
                attachments_json,
                requested_mode,
                priority,
                immediate,
                source_json,
                feishu_recipients_json,
                created_at,
            )| {
                let attachments: Vec<MessageAttachmentInput> =
                    serde_json::from_str(&attachments_json).unwrap_or_default();
                let requested_mode = requested_mode
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .map(str::parse)
                    .transpose()?;
                let source = parse_optional_json(source_json.as_deref())?;
                let feishu_recipients = parse_optional_json(feishu_recipients_json.as_deref())?;
                Ok(QueuedMessageRow {
                    id,
                    session_id,
                    content,
                    attachments,
                    requested_mode,
                    priority: priority != 0,
                    immediate: immediate != 0,
                    source,
                    feishu_recipients,
                    created_at,
                })
            },
        )
        .collect()
}

fn parse_optional_json(raw: Option<&str>) -> Result<Option<serde_json::Value>, String> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => serde_json::from_str(value)
            .map(Some)
            .map_err(|error| error.to_string()),
        None => Ok(None),
    }
}

/// 整表替换：前端每次队列变更后把当前快照写回，顺序即 position。
pub fn replace_all(db: &Connection, items: &[QueuedMessageRow]) -> Result<(), String> {
    let transaction = db
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM session_message_queue", [])
        .map_err(|error| error.to_string())?;
    let now = Utc::now().to_rfc3339();
    for (position, item) in items.iter().enumerate() {
        let attachments_json =
            serde_json::to_string(&item.attachments).map_err(|error| error.to_string())?;
        let source_json = item
            .source
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| error.to_string())?;
        let feishu_recipients_json = item
            .feishu_recipients
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| error.to_string())?;
        let created_at = if item.created_at.trim().is_empty() {
            now.clone()
        } else {
            item.created_at.clone()
        };
        transaction
            .execute(
                "INSERT INTO session_message_queue
                 (id, session_id, position, content, attachments_json, requested_mode, priority,
                  immediate, source_json, feishu_recipients_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    item.id,
                    item.session_id,
                    position as i64,
                    item.content,
                    attachments_json,
                    item.requested_mode.map(RunMode::as_str),
                    if item.priority { 1 } else { 0 },
                    if item.immediate { 1 } else { 0 },
                    source_json,
                    feishu_recipients_json,
                    created_at,
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;

    fn open_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        database::migrate(&db).unwrap();
        db
    }

    #[test]
    fn replace_and_list_round_trip() {
        let db = open_db();
        db.execute(
            "INSERT INTO chat_sessions
             (id, project_id, title, target_type, target_id, created_at, updated_at, default_mode)
             VALUES ('s1', NULL, 't', 'agent', 'miku', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'auto')",
            [],
        )
        .unwrap();
        replace_all(
            &db,
            &[
                QueuedMessageRow {
                    id: "q1".into(),
                    session_id: "s1".into(),
                    content: "first".into(),
                    attachments: vec![],
                    requested_mode: Some(RunMode::Agent),
                    priority: false,
                    immediate: false,
                    source: None,
                    feishu_recipients: None,
                    created_at: "2026-01-01T00:00:00Z".into(),
                },
                QueuedMessageRow {
                    id: "q2".into(),
                    session_id: "s1".into(),
                    content: "second".into(),
                    attachments: vec![MessageAttachmentInput {
                        name: "a.png".into(),
                        path: "/tmp/a.png".into(),
                        mime_type: "image/png".into(),
                        size: 12,
                    }],
                    requested_mode: None,
                    priority: true,
                    immediate: true,
                    source: Some(serde_json::json!({
                        "sourceType": "genui",
                        "meta": { "agentId": "miku" }
                    })),
                    feishu_recipients: None,
                    created_at: "2026-01-01T00:00:01Z".into(),
                },
            ],
        )
        .unwrap();
        let rows = list(&db).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "q1");
        assert_eq!(rows[0].content, "first");
        assert_eq!(rows[0].requested_mode, Some(RunMode::Agent));
        assert_eq!(rows[1].id, "q2");
        assert!(rows[1].priority);
        assert!(rows[1].immediate);
        assert_eq!(rows[1].attachments.len(), 1);
        assert!(rows[1].source.is_some());
        replace_all(&db, &[]).unwrap();
        assert!(list(&db).unwrap().is_empty());
    }
}
