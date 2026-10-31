use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::models::ChatSession;

pub fn fork_session(
    db: &Connection,
    session_id: &str,
    until_message_id: Option<&str>,
) -> Result<ChatSession, String> {
    // 一次提交：长会话逐条自动提交要上千次 fsync，中途失败也会留下半个副本。
    let transaction = db
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let forked = copy_session(&transaction, session_id, until_message_id)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(forked)
}

fn copy_session(
    db: &Connection,
    session_id: &str,
    until_message_id: Option<&str>,
) -> Result<ChatSession, String> {
    let (project_id, title, target_type, target_id, default_mode): (Option<String>, String, String, String, String) = db
        .query_row(
            "SELECT project_id, title, target_type, target_id, default_mode FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|_| tr!("会话不存在"))?;
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let forked_title = if title.ends_with("（副本）") {
        title
    } else {
        format!("{title}（副本）")
    };
    db.execute(
        "INSERT INTO chat_sessions
         (id, project_id, title, target_type, target_id, created_at, updated_at, default_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
        params![
            id,
            project_id,
            forked_title,
            target_type,
            target_id,
            now,
            default_mode
        ],
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
    let rows = match until_message_id
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        Some(until_id) => {
            let end = rows
                .iter()
                .position(|(id, ..)| id == until_id)
                .ok_or_else(|| tr!("消息不存在或不属于该会话"))?;
            rows.into_iter().take(end + 1).collect::<Vec<_>>()
        }
        None => rows,
    };
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
        default_mode: default_mode.parse().unwrap_or_default(),
    })
}

pub fn set_feedback(
    db: &Connection,
    message_id: &str,
    feedback: Option<&str>,
) -> Result<(), String> {
    let value = match feedback.map(str::trim).filter(|item| !item.is_empty()) {
        Some("up") | Some("down") => feedback.map(str::trim),
        Some(_) => return Err(tr!("反馈只能是 up 或 down")),
        None => None,
    };
    let updated = db
        .execute(
            "UPDATE messages SET feedback=?1 WHERE id=?2",
            params![value, message_id],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err(tr!("消息不存在"));
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
        return Err(tr!("摘要不能为空"));
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
        return Err(tr!("会话不存在"));
    }
    Ok(())
}

struct CompactText {
    instruction: &'static str,
    user: &'static str,
    assistant: &'static str,
    latest_plan: &'static str,
    title: &'static str,
    summary: &'static str,
    status: &'static str,
    untitled_plan: &'static str,
}

fn compact_text(locale: &str) -> CompactText {
    match locale {
        "en" => CompactText {
            instruction: "Condense the conversation below into an English summary of about 500 words or fewer. Keep the goals, changes already made, unfinished work, and open questions that still need confirmation:\n\n",
            user: "User: ",
            assistant: "Assistant: ",
            latest_plan: "Latest plan:\n",
            title: "Title: ",
            summary: "Summary: ",
            status: "Status: ",
            untitled_plan: "Plan",
        },
        "zh-TW" => CompactText {
            instruction: "請把下面的對話壓縮成不超過 800 字的繁體中文摘要，保留目標、已做改動、未完成事項和待確認問題：\n\n",
            user: "使用者：",
            assistant: "助手：",
            latest_plan: "最新計畫：\n",
            title: "標題：",
            summary: "摘要：",
            status: "狀態：",
            untitled_plan: "計畫",
        },
        "ja" => CompactText {
            instruction: "以下の会話を 1200 字以内の日本語の要約にまとめてください。目標、実施済みの変更、未完了の事項、確認待ちの質問は残してください：\n\n",
            user: "ユーザー：",
            assistant: "アシスタント：",
            latest_plan: "最新の計画：\n",
            title: "タイトル：",
            summary: "要約：",
            status: "ステータス：",
            untitled_plan: "計画",
        },
        "ko" => CompactText {
            instruction: "아래 대화를 1200자 이내의 한국어 요약으로 압축하세요. 목표, 이미 한 변경, 끝나지 않은 일, 확인이 필요한 질문은 남겨 두세요:\n\n",
            user: "사용자: ",
            assistant: "어시스턴트: ",
            latest_plan: "최신 계획:\n",
            title: "제목: ",
            summary: "요약: ",
            status: "상태: ",
            untitled_plan: "계획",
        },
        _ => CompactText {
            instruction: "请把下面的对话压缩成不超过 800 字的中文摘要，保留目标、已做改动、未完成事项和待确认问题：\n\n",
            user: "用户：",
            assistant: "助手：",
            latest_plan: "最新计划：\n",
            title: "标题：",
            summary: "摘要：",
            status: "状态：",
            untitled_plan: "计划",
        },
    }
}

pub fn compact_history_text(db: &Connection, session_id: &str) -> Result<String, String> {
    let text = compact_text(crate::app_locale::from_db(db));
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
        return Err(tr!("没有可以压缩的对话"));
    }
    let mut out = String::from(text.instruction);
    for (role, content) in lines {
        let clipped: String = content.chars().take(1200).collect();
        out.push_str(if role == "user" {
            text.user
        } else {
            text.assistant
        });
        out.push_str(&clipped);
        out.push_str("\n\n");
    }
    let latest_plan: Option<String> = db
        .query_row(
            "SELECT input_json FROM run_events
             WHERE session_id=?1 AND event_type='plan'
             ORDER BY created_at DESC, seq DESC LIMIT 1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(raw) = latest_plan {
        if let Ok(plan) = serde_json::from_str::<serde_json::Value>(&raw) {
            let plan_id = plan
                .get("planId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let revision = plan
                .get("revision")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1);
            let action: Option<String> = db
                .query_row(
                    "SELECT json_extract(input_json, '$.action') FROM run_events
                     WHERE session_id=?1 AND event_type='plan_action'
                       AND json_extract(input_json, '$.planId')=?2
                       AND json_extract(input_json, '$.revision')=?3
                     ORDER BY seq DESC LIMIT 1",
                    params![session_id, plan_id, revision],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            out.push_str(text.latest_plan);
            out.push_str(&format!(
                "{}{}\n{}{}\n{}{}\n\n",
                text.title,
                plan.get("title")
                    .and_then(serde_json::Value::as_str)
                    .filter(|title| !title.is_empty())
                    .unwrap_or(text.untitled_plan),
                text.summary,
                plan.get("summary")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
                text.status,
                action.as_deref().unwrap_or("awaiting_approval")
            ));
        }
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

        let forked = fork_session(&db, "s1", None).unwrap();

        assert_eq!(forked.project_id.as_deref(), Some("p1"));
        assert_eq!(forked.project_ids, ["p1", "p2"]);
        assert_eq!(
            crate::session::linked_project_ids(&db, &forked.id).unwrap(),
            ["p1", "p2"]
        );
    }

    #[test]
    fn fork_until_message_keeps_prefix_only() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO projects (id, name, created_at, opened_at) VALUES
               ('p1', '需求一', 'now', 'now');
             INSERT INTO chat_sessions
               (id, project_id, title, target_type, target_id, created_at, updated_at)
             VALUES ('s1', 'p1', '会话', 'agent', 'a1', 'now', 'now');
             INSERT INTO session_projects (session_id, project_id, sort_order) VALUES ('s1', 'p1', 0);
             INSERT INTO messages
               (id, session_id, role, content, message_kind, quick_actions_json, created_at)
             VALUES
               ('m1', 's1', 'user', '问1', 'text', '[]', '2024-01-01T00:00:01Z'),
               ('m2', 's1', 'assistant', '答1', 'text', '[]', '2024-01-01T00:00:02Z'),
               ('m3', 's1', 'user', '问2', 'text', '[]', '2024-01-01T00:00:03Z'),
               ('m4', 's1', 'assistant', '答2', 'text', '[]', '2024-01-01T00:00:04Z');",
        )
        .unwrap();

        let forked = fork_session(&db, "s1", Some("m2")).unwrap();
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id=?1",
                [&forked.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        let last: String = db
            .query_row(
                "SELECT content FROM messages WHERE session_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [&forked.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(last, "答1");
    }
}
