use rusqlite::{params, Connection};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    conversation::{self, CONTINUE_ASSIGNMENT},
    ops::run::{self, Enqueue},
    session,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeResult {
    pub session_id: String,
    pub run_id: String,
    pub conversation_id: String,
    pub runtime_id: String,
    pub assignment: String,
}

pub fn resume(
    db: &Connection,
    session_id: &str,
    assignment: Option<&str>,
) -> Result<ResumeResult, String> {
    let assignment = assignment
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(CONTINUE_ASSIGNMENT);
    let project_ids = session::linked_project_ids(db, session_id)?;
    session::ensure_session_idle(db, session_id)?;
    let (target_type, target_id): (String, String) = db
        .query_row(
            "SELECT target_type, target_id FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| tr!("聊天不存在"))?;
    if target_type != "agent" {
        return Err(tr!("只能继续单个智能体的会话"));
    }
    let runtime_id: String = db
        .query_row(
            "SELECT runtime_id FROM agents WHERE id=?1",
            [&target_id],
            |row| row.get(0),
        )
        .map_err(|_| tr!("智能体不存在"))?;
    if !conversation::resumes_via_cli(&runtime_id) {
        return Err(tr!(
            "这个运行时不能 resume。目前只有 Cursor 和 Claude 可以接着上次的任务。"
        ));
    }
    let conversation_id = conversation::load_id(db, session_id, &target_id, &runtime_id)?
        .ok_or_else(|| tr!("没有可继续的会话。上次运行还没留下 conversation id。"))?;
    let now = chrono::Utc::now().to_rfc3339();
    let message_id = Uuid::new_v4().to_string();
    let turn_id = (!project_ids.is_empty()).then(|| Uuid::new_v4().to_string());
    if let Some(turn_id) = &turn_id {
        let seq: u32 = db
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM agent_turns WHERE session_id=?1",
                [session_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        db.execute(
            "INSERT INTO agent_turns
             (id, session_id, seq, user_message_id, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
            params![turn_id, session_id, seq, message_id, now],
        )
        .map_err(|error| error.to_string())?;
    }
    db.execute(
        "INSERT INTO messages
         (id, session_id, turn_id, role, content, target_type, target_id, created_at)
         VALUES (?1, ?2, ?3, 'user', ?4, 'agent', ?5, ?6)",
        params![message_id, session_id, turn_id, assignment, target_id, now],
    )
    .map_err(|error| error.to_string())?;
    let run_id = run::enqueue(
        db,
        Enqueue {
            session_id,
            turn_id: turn_id.as_deref(),
            agent_id: &target_id,
            parent_run_id: None,
            assignment,
            status: "queued",
            now: &now,
            mode: crate::models::RunMode::Auto,
        },
    )?;
    Ok(ResumeResult {
        session_id: session_id.to_string(),
        run_id,
        conversation_id,
        runtime_id,
        assignment: assignment.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::resume;
    use rusqlite::Connection;

    #[test]
    fn resume_is_independent_of_other_tabs_but_not_its_own_active_run() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO projects (id, name, created_at, opened_at)
             VALUES ('project', 'Project', 'now', 'now');
             INSERT INTO agents
               (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('agent', 'Agent', '', '', '', 'cursor', 'auto', 1);
             INSERT INTO chat_sessions
               (id, project_id, title, target_type, target_id, created_at, updated_at)
             VALUES
               ('first', 'project', 'First', 'agent', 'agent', 'now', 'now'),
               ('second', 'project', 'Second', 'agent', 'agent', 'now', 'now');
             INSERT INTO agent_runs
               (id, session_id, agent_id, runtime_id, assignment, status, created_at)
             VALUES ('other-run', 'first', 'agent', 'cursor', 'Work', 'running', 'now');",
        )
        .unwrap();
        crate::conversation::save_id(&db, "second", "agent", "cursor", "saved-chat").unwrap();

        let result = resume(&db, "second", Some("Continue second")).unwrap();
        assert_eq!(result.session_id, "second");
        assert_eq!(result.conversation_id, "saved-chat");
        assert_eq!(result.assignment, "Continue second");
        let status: String = db
            .query_row(
                "SELECT status FROM agent_runs WHERE id=?1",
                [&result.run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "queued");
        assert!(resume(&db, "second", None)
            .unwrap_err()
            .contains("当前会话"));
        let messages: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id='second'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(messages, 1);
    }
}
