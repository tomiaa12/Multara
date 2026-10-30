use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::models::ChatSession;

pub fn fork_session(db: &Connection, session_id: &str) -> Result<ChatSession, String> {
    // 一次提交：长会话逐条自动提交要上千次 fsync，中途失败也会留下半个副本。
    let transaction = db
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let forked = copy_session(&transaction, session_id)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(forked)
}

fn copy_session(db: &Connection, session_id: &str) -> Result<ChatSession, String> {
    let (project_id, title, target_type, target_id): (Option<String>, String, String, String) = db
        .query_row(
            "SELECT project_id, title, target_type, target_id FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| "会话不存在".to_string())?;
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let forked_title = if title.ends_with("（副本）") {
        title
    } else {
        format!("{title}（副本）")
    };
    db.execute(
        "INSERT INTO chat_sessions
         (id, project_id, title, target_type, target_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, project_id, forked_title, target_type, target_id, now],
    )
    .map_err(|error| error.to_string())?;
    let project_ids = crate::session::linked_project_ids(db, session_id)?;
    crate::session::replace_linked_projects(db, &id, &project_ids)?;
    let mut source = db
        .prepare(
            "SELECT id, role, content, failure_reason, elapsed_ms, message_kind, quick_actions_json,
                    agent_id, audience_type, audience_id, target_type, target_id, created_at, feedback
             FROM messages WHERE session_id=?1 ORDER BY created_at, rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = source
        .query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, Option<String>>(13)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    drop(source);
    for (
        old_id,
        role,
        content,
        failure_reason,
        elapsed_ms,
        message_kind,
        quick_actions,
        agent_id,
        audience_type,
        audience_id,
        msg_target_type,
        msg_target_id,
        created_at,
        feedback,
    ) in rows
    {
        let new_id = Uuid::new_v4().to_string();
        db.execute(
            "INSERT INTO messages
             (id, session_id, role, content, failure_reason, elapsed_ms, message_kind,
              quick_actions_json, agent_id, audience_type, audience_id, target_type, target_id,
              created_at, feedback)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                new_id,
                id,
                role,
                content,
                failure_reason,
                elapsed_ms,
                message_kind,
                quick_actions,
                agent_id,
                audience_type,
                audience_id,
                msg_target_type,
                msg_target_id,
                created_at,
                feedback
            ],
        )
        .map_err(|error| error.to_string())?;
        db.execute(
            "INSERT INTO message_attachments
             (id, message_id, session_id, name, path, mime_type, size, created_at)
             SELECT ?1, ?2, ?3, name, path, mime_type, size, created_at
             FROM message_attachments WHERE message_id=?4",
            params![Uuid::new_v4().to_string(), new_id, id, old_id],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(ChatSession {
        id,
        project_id,
        project_ids,
        title: forked_title,
        target_type: if target_type == "squad" {
            "squad"
        } else {
            "agent"
        },
        target_id,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn set_feedback(
    db: &Connection,
    message_id: &str,
    feedback: Option<&str>,
) -> Result<(), String> {
    let value = match feedback.map(str::trim).filter(|item| !item.is_empty()) {
        Some("up") | Some("down") => feedback.map(str::trim),
        Some(_) => return Err("反馈只能是 up 或 down".to_string()),
        None => None,
    };
    let updated = db
        .execute(
            "UPDATE messages SET feedback=?1 WHERE id=?2",
            params![value, message_id],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("消息不存在".to_string());
    }
    Ok(())
}

pub fn save_compact_summary(
    db: &Connection,
    session_id: &str,
    summary: &str,
    message_id: Option<&str>,
) -> Result<(), String> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err("摘要不能为空".to_string());
    }
    let updated = db
        .execute(
            "UPDATE chat_sessions
             SET compact_summary=?1, compact_message_id=?2, updated_at=?3
             WHERE id=?4",
            params![summary, message_id, Utc::now().to_rfc3339(), session_id],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("会话不存在".to_string());
    }
    Ok(())
}

pub fn compact_history_text(db: &Connection, session_id: &str) -> Result<String, String> {
    let mut statement = db
        .prepare(
            "SELECT role, content FROM messages WHERE session_id=?1 ORDER BY created_at, rowid",
        )
        .map_err(|error| error.to_string())?;
    let lines = statement
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    if lines.is_empty() {
        return Err("没有可以压缩的对话".to_string());
    }
    let mut out = String::from("请把下面的对话压缩成不超过 800 字的中文摘要，保留目标、已做改动、未完成事项和待确认问题：\n\n");
    for (role, content) in lines {
        let clipped: String = content.chars().take(1200).collect();
        out.push_str(if role == "user" {
            "用户："
        } else {
            "助手："
        });
        out.push_str(&clipped);
        out.push_str("\n\n");
    }
    Ok(out)
}

pub fn last_message_id(db: &Connection, session_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT id FROM messages WHERE session_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
        [session_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::fork_session;
    use rusqlite::Connection;

    #[test]
    fn fork_copies_all_linked_projects_in_order() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO projects (id, name, created_at, opened_at) VALUES
               ('p1', '需求一', 'now', 'now'),
               ('p2', '需求二', 'now', 'now');
             INSERT INTO chat_sessions
               (id, project_id, title, target_type, target_id, created_at, updated_at)
             VALUES ('s1', 'p1', '会话', 'agent', 'a1', 'now', 'now');
             INSERT INTO session_projects (session_id, project_id, sort_order) VALUES
               ('s1', 'p1', 0),
               ('s1', 'p2', 1);",
        )
        .unwrap();

        let forked = fork_session(&db, "s1").unwrap();

        assert_eq!(forked.project_id.as_deref(), Some("p1"));
        assert_eq!(forked.project_ids, ["p1", "p2"]);
        assert_eq!(
            crate::session::linked_project_ids(&db, &forked.id).unwrap(),
            ["p1", "p2"]
        );
    }
}
