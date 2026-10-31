use rusqlite::{Connection, OptionalExtension};

use crate::models::{Automation, InboxInput, Task};

use super::{inbox, task};

pub fn list(db: &Connection) -> Result<Vec<Automation>, String> {
    load(db)
}

pub fn load(db: &Connection) -> Result<Vec<Automation>, String> {
    let mut statement = db
        .prepare(
            "SELECT a.id, a.name, a.project_id, a.target_type, a.target_id, a.prompt,
                    a.trigger_type, a.interval_minutes, a.event_name, a.enabled,
                    a.last_run_at, a.created_at
             FROM automations a
             ORDER BY a.created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let automations = statement
        .query_map([], |row| {
            Ok(Automation {
                id: row.get(0)?,
                name: row.get(1)?,
                project_id: row.get(2)?,
                target_type: row.get(3)?,
                target_id: row.get(4)?,
                prompt: row.get(5)?,
                trigger_type: row.get(6)?,
                interval_minutes: row.get(7)?,
                event_name: row.get(8)?,
                enabled: row.get::<_, i64>(9)? != 0,
                last_run_at: row.get(10)?,
                created_at: row.get(11)?,
                trigger_id: None,
                cron_expression: None,
                timezone: None,
                next_run_at: None,
                webhook_token: None,
                last_run_status: None,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(automations)
}

pub fn get(db: &Connection, automation_id: &str) -> Result<Automation, String> {
    load(db)?
        .into_iter()
        .find(|item| item.id == automation_id)
        .ok_or_else(|| tr!("自动化不存在"))
}

/// 自动化会话挂到任务看板。会话已存在时不会重复建。
pub fn ensure_task(
    db: &Connection,
    session_id: &str,
    title: &str,
    description: &str,
    agent_id: &str,
    status: &str,
    now: &str,
) -> Result<Task, String> {
    task::attach_existing_session(db, session_id, title, description, agent_id, status, now)
}

/// 跑完后写入收件箱，并补上可能缺失的任务行（旧记录回填也走这里）。
pub fn publish_result(db: &Connection, session_id: &str, status: &str) -> Result<Task, String> {
    let inbox_status = if status == "completed" {
        "completed"
    } else {
        "failed"
    };
    let task_status = if status == "completed" {
        "in_review"
    } else {
        "todo"
    };
    let now = chrono::Utc::now().to_rfc3339();
    let task = match existing_task(db, session_id)? {
        Some(task) => task,
        None => attach_from_session(db, session_id, task_status, &now)?,
    };
    let project_id: Option<String> = db
        .query_row(
            "SELECT project_id FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    let preview = last_assistant_preview(db, session_id);
    inbox::upsert(
        db,
        InboxInput {
            session_id: session_id.to_string(),
            project_id,
            task_id: Some(task.id.clone()),
            title: task.title.clone(),
            preview,
            status: inbox_status.to_string(),
        },
    )?;
    Ok(task)
}

/// 把历史上只建了隐藏会话、没有任务/收件箱的自动化结果补上。
pub fn backfill_results(db: &Connection) -> Result<u32, String> {
    let mut statement = db
        .prepare(
            "SELECT s.id, COALESCE(r.status, 'completed')
             FROM chat_sessions s
             LEFT JOIN automation_runs r ON r.id = substr(s.id, length('automation-run:') + 1)
             -- 用主键范围代替 LIKE（默认不区分大小写，走不了索引）；';' 紧跟在 ':' 之后。
             WHERE s.id >= 'automation-run:' AND s.id < 'automation-run;'
               AND COALESCE(r.status, 'completed') IN ('completed', 'failed', 'running')
               AND (
                 NOT EXISTS (SELECT 1 FROM tasks t WHERE t.session_id = s.id)
                 OR (
                   COALESCE(r.status, 'completed') IN ('completed', 'failed')
                   AND NOT EXISTS (SELECT 1 FROM inbox_items i WHERE i.session_id = s.id)
                 )
               )",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut count = 0;
    for (session_id, status) in rows {
        if status == "running" {
            let now = chrono::Utc::now().to_rfc3339();
            if attach_from_session(db, &session_id, "in_progress", &now).is_ok() {
                count += 1;
            }
            continue;
        }
        if publish_result(db, &session_id, &status).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

fn existing_task(db: &Connection, session_id: &str) -> Result<Option<Task>, String> {
    let task_id: Option<String> = db
        .query_row(
            "SELECT id FROM tasks WHERE session_id=?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match task_id {
        Some(id) => task::get(db, &id).map(Some),
        None => Ok(None),
    }
}

fn attach_from_session(
    db: &Connection,
    session_id: &str,
    status: &str,
    now: &str,
) -> Result<Task, String> {
    let (title, target_type, target_id): (String, String, String) = db
        .query_row(
            "SELECT title, target_type, target_id FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| tr!("会话不存在"))?;
    let (name, prompt): (Option<String>, Option<String>) = db
        .query_row(
            "SELECT a.name, a.prompt
             FROM automation_runs r
             JOIN automations a ON a.id = r.automation_id
             WHERE r.id = substr(?1, length('automation-run:') + 1)",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or((None, None));
    let title = name.unwrap_or_else(|| title.trim_start_matches("自动化 · ").to_string());
    let description = prompt.unwrap_or_default();
    let agent_id = if target_type == "squad" {
        db.query_row(
            "SELECT leader_agent_id FROM squads WHERE id=?1",
            [&target_id],
            |row| row.get(0),
        )
        .unwrap_or(target_id)
    } else {
        target_id
    };
    task::attach_existing_session(db, session_id, &title, &description, &agent_id, status, now)
}

fn last_assistant_preview(db: &Connection, session_id: &str) -> String {
    db.query_row(
        "SELECT content FROM messages
         WHERE session_id=?1 AND role='assistant'
         ORDER BY created_at DESC LIMIT 1",
        [session_id],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{backfill_results, ensure_task, publish_result};

    fn setup() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute(
            "INSERT INTO agents (id, name, avatar, description, instructions, runtime_id)
             VALUES ('miku', 'Miku', 'M', '', '', 'cursor')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO automations
             (id, name, target_type, target_id, prompt, trigger_type, enabled, created_at)
             VALUES ('auto', '查询今天长沙的天气', 'agent', 'miku', '列出下雨风速温度', 'manual', 1, 't0')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO automation_runs
             (id, automation_id, source, status, triggered_at)
             VALUES ('run1', 'auto', 'schedule', 'completed', 't1')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO chat_sessions
             (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('automation-run:run1', '自动化 · 查询今天长沙的天气', 'agent', 'miku', 't1', 't1')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO messages (id, session_id, role, content, created_at)
             VALUES ('m1', 'automation-run:run1', 'user', '列出下雨风速温度', 't1'),
                    ('m2', 'automation-run:run1', 'assistant', '长沙今天晴，15到22度。', 't2')",
            [],
        )
        .unwrap();
        db
    }

    #[test]
    fn finished_run_becomes_task_and_inbox() {
        let db = setup();
        let task = publish_result(&db, "automation-run:run1", "completed").unwrap();
        assert_eq!(task.title, "查询今天长沙的天气");
        assert_eq!(task.status, "in_review");
        let inbox_title: String = db
            .query_row(
                "SELECT title FROM inbox_items WHERE session_id='automation-run:run1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(inbox_title, "查询今天长沙的天气");
        let preview: String = db
            .query_row(
                "SELECT preview FROM inbox_items WHERE session_id='automation-run:run1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(preview.contains("长沙今天晴"));
        assert_eq!(
            publish_result(&db, "automation-run:run1", "completed")
                .unwrap()
                .id,
            task.id
        );
    }

    #[test]
    fn backfill_creates_missing_task() {
        let db = setup();
        assert_eq!(backfill_results(&db).unwrap(), 1);
        assert_eq!(backfill_results(&db).unwrap(), 0);
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE session_id='automation-run:run1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn ensure_task_does_not_duplicate() {
        let db = setup();
        let first = ensure_task(
            &db,
            "automation-run:run1",
            "查询今天长沙的天气",
            "列出下雨风速温度",
            "miku",
            "in_progress",
            "t1",
        )
        .unwrap();
        let second = ensure_task(
            &db,
            "automation-run:run1",
            "查询今天长沙的天气",
            "列出下雨风速温度",
            "miku",
            "in_progress",
            "t1",
        )
        .unwrap();
        assert_eq!(first.id, second.id);
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
