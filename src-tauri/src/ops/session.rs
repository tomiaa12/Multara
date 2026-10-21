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
    let project_id = session::project_id(db, session_id)?;
    if let Some(project_id) = &project_id {
        session::ensure_project_idle(db, project_id)?;
    }
    let (target_type, target_id): (String, String) = db
        .query_row(
            "SELECT target_type, target_id FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "聊天不存在".to_string())?;
    if target_type != "agent" {
        return Err("只能继续单个智能体的会话".to_string());
    }
    let runtime_id: String = db
        .query_row(
            "SELECT runtime_id FROM agents WHERE id=?1",
            [&target_id],
            |row| row.get(0),
        )
        .map_err(|_| "智能体不存在".to_string())?;
    if !conversation::resumes_via_cli(&runtime_id) {
        return Err(
            "这个运行时不能 resume。目前只有 Cursor 和 Claude 可以接着上次的任务。".to_string(),
        );
    }
    let conversation_id = conversation::load_id(db, session_id, &target_id, &runtime_id)?
        .ok_or_else(|| "没有可继续的会话。上次运行还没留下 conversation id。".to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    let message_id = Uuid::new_v4().to_string();
    let turn_id = project_id.as_ref().map(|_| Uuid::new_v4().to_string());
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
