use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::models::{Task, TaskChecklistItem};

use super::{parse_task_status, require_agent, require_project};

pub struct CreateTaskInput<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub agent_id: &'a str,
    pub project_id: Option<&'a str>,
    pub source_session_id: Option<&'a str>,
    pub status: Option<&'a str>,
    pub parent_task_id: Option<&'a str>,
}

const TASK_COLUMNS: &str = "t.id, t.session_id, t.title, t.description, t.agent_id, t.status,
                    t.created_at, t.updated_at, t.parent_task_id, t.priority,
                    t.labels_json, t.checklist_json, t.pr_url";

fn parse_labels(raw: String) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| !item.trim().is_empty())
        .collect()
}

fn parse_checklist(raw: String) -> Vec<TaskChecklistItem> {
    serde_json::from_str(&raw).unwrap_or_default()
}

fn parse_priority(raw: &str) -> String {
    match raw {
        "urgent" | "high" | "medium" | "low" => raw.to_string(),
        _ => "none".to_string(),
    }
}

fn map_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let status: String = row.get(5)?;
    Ok(Task {
        id: row.get(0)?,
        session_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        agent_id: row.get(4)?,
        status: super::parse_task_status(&status).unwrap_or("todo"),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        parent_task_id: row.get(8)?,
        priority: parse_priority(&row.get::<_, String>(9).unwrap_or_else(|_| "none".into())),
        labels: parse_labels(row.get::<_, String>(10).unwrap_or_else(|_| "[]".into())),
        checklist: parse_checklist(row.get::<_, String>(11).unwrap_or_else(|_| "[]".into())),
        pr_url: row.get::<_, String>(12).unwrap_or_default(),
    })
}

pub struct CreatedTask {
    pub task: Task,
    pub queued: bool,
}

pub fn create(db: &mut Connection, input: CreateTaskInput<'_>) -> Result<CreatedTask, String> {
    let title = input.title.trim();
    let description = input.description.trim();
    if title.is_empty() {
        return Err("任务标题不能为空".to_string());
    }
    require_agent(db, input.agent_id)?;
    let status = parse_task_status(input.status.unwrap_or("todo"))?;
    let mut project_id = input
        .project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if project_id.is_none() {
        if let Some(session_id) = input.source_session_id {
            project_id = db
                .query_row(
                    "SELECT project_id FROM chat_sessions WHERE id=?1",
                    [session_id],
                    |row| row.get(0),
                )
                .unwrap_or(None);
        }
    }
    if let Some(project_id) = project_id.as_deref() {
        require_project(db, project_id)?;
    }

    let now = Utc::now().to_rfc3339();
    let task_id = Uuid::new_v4().to_string();
    let session_id = Uuid::new_v4().to_string();
    let transaction = db.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO chat_sessions
             (id, project_id, title, target_type, target_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'agent', ?4, ?5, ?5)",
            params![session_id, project_id, title, input.agent_id, now],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO tasks
             (id, session_id, title, description, agent_id, status, created_at, updated_at, parent_task_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
            params![
                task_id,
                session_id,
                title,
                description,
                input.agent_id,
                status,
                now,
                input.parent_task_id
            ],
        )
        .map_err(|error| error.to_string())?;
    let queued = status == "todo";
    if queued {
        let assignment = if description.is_empty() { title } else { description };
        super::run::enqueue(
            &transaction,
            super::run::Enqueue {
                session_id: &session_id,
                turn_id: None,
                agent_id: input.agent_id,
                parent_run_id: None,
                assignment,
                status: "queued",
                now: &now,
            },
        )?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(CreatedTask {
        task: Task {
            id: task_id,
            session_id,
            title: title.to_string(),
            description: description.to_string(),
            agent_id: input.agent_id.to_string(),
            status: if queued { "in_progress" } else { status },
            created_at: now.clone(),
            updated_at: now,
            parent_task_id: input.parent_task_id.map(str::to_string),
            priority: "none".to_string(),
            labels: Vec::new(),
            checklist: Vec::new(),
            pr_url: String::new(),
        },
        queued,
    })
}

/// 把已经存在的会话挂成任务，不再新建会话、也不再排队。
/// 自动化先建了 `automation-run:` 会话并派发 run，这里只补看板那一行。
pub fn attach_existing_session(
    db: &Connection,
    session_id: &str,
    title: &str,
    description: &str,
    agent_id: &str,
    status: &str,
    now: &str,
) -> Result<Task, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("任务标题不能为空".to_string());
    }
    require_agent(db, agent_id)?;
    let status = parse_task_status(status)?;
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM tasks WHERE session_id=?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(task_id) = existing {
        return get(db, &task_id);
    }
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id=?1)",
            [session_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !exists {
        return Err("会话不存在".to_string());
    }
    let task_id = Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO tasks
         (id, session_id, title, description, agent_id, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            task_id,
            session_id,
            title,
            description.trim(),
            agent_id,
            status,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    get(db, &task_id)
}

pub fn get(db: &Connection, task_id: &str) -> Result<Task, String> {
    list(db, None, None, None)?
        .into_iter()
        .find(|task| task.id == task_id)
        .ok_or_else(|| "任务不存在".to_string())
}

/// 回答已经结束、没有还在排队或运行的记录，却停在进行中时收口。
/// 最后一次成功进审核；最后一次失败回待办。用户手动停掉的不挪。
/// 只在打开看板时调用，避免刚拖进进行中又被立刻弹走。
pub(crate) fn settle_finished(db: &Connection) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    db.execute(
        "UPDATE tasks SET status='in_review', updated_at=?1
         WHERE status='in_progress'
           AND NOT EXISTS (
             SELECT 1 FROM agent_runs r
             WHERE r.session_id = tasks.session_id AND r.status IN ('queued', 'running')
           )
           AND (
             SELECT r.status FROM agent_runs r
             WHERE r.session_id = tasks.session_id
             ORDER BY COALESCE(r.completed_at, r.created_at) DESC
             LIMIT 1
           ) = 'completed'",
        [&now],
    )
    .map_err(|error| error.to_string())?;
    db.execute(
        "UPDATE tasks SET status='todo', updated_at=?1
         WHERE status='in_progress'
           AND NOT EXISTS (
             SELECT 1 FROM agent_runs r
             WHERE r.session_id = tasks.session_id AND r.status IN ('queued', 'running')
           )
           AND (
             SELECT r.status FROM agent_runs r
             WHERE r.session_id = tasks.session_id
             ORDER BY COALESCE(r.completed_at, r.created_at) DESC
             LIMIT 1
           ) = 'failed'",
        [&now],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list(
    db: &Connection,
    status: Option<&str>,
    assignee_id: Option<&str>,
    project_id: Option<&str>,
) -> Result<Vec<Task>, String> {
    let mut statement = db
        .prepare(
            &format!(
                "SELECT {TASK_COLUMNS}
             FROM tasks t
             JOIN chat_sessions s ON s.id = t.session_id
             WHERE (?1 IS NULL OR t.status = ?1)
               AND (?2 IS NULL OR t.agent_id = ?2)
               AND (?3 IS NULL OR s.project_id = ?3)
             ORDER BY t.updated_at DESC"
            ),
        )
        .map_err(|error| error.to_string())?;
    let tasks = statement
        .query_map(params![status, assignee_id, project_id], map_task)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(tasks)
}

pub fn update_status(db: &Connection, task_id: &str, status: &str) -> Result<Vec<Task>, String> {
    let status = parse_task_status(status)?;
    let now = Utc::now().to_rfc3339();
    let updated = db
        .execute(
            "UPDATE tasks SET status=?1, updated_at=?2 WHERE id=?3",
            params![status, now, task_id],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("任务不存在".to_string());
    }
    // 拖到待办只改列，不重新排队。否则 enqueue 会立刻把状态同步回进行中，卡片拖不过去。
    list(db, None, None, None)
}

/// 用户在看板把任务挪进「进行中」时真正开跑：会话里没有排队/运行的 run 就按任务描述排一条。
/// 不排队的话，下次打开看板 `settle_finished` 会因为最后一次 run 失败把它弹回待办。
/// 只给 UI 用；智能体在自己的 run 里通过 CLI 改状态时已有活跃 run，不能再叠一条。
pub fn start_if_idle(db: &Connection, task_id: &str) -> Result<bool, String> {
    let (session_id, title, description, agent_id): (String, String, String, String) = db
        .query_row(
            "SELECT session_id, title, description, agent_id FROM tasks WHERE id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| "任务不存在".to_string())?;
    let active: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs
             WHERE session_id=?1 AND status IN ('queued', 'running')",
            [&session_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active > 0 {
        return Ok(false);
    }
    let assignment = if description.trim().is_empty() { title.as_str() } else { description.as_str() };
    let now = Utc::now().to_rfc3339();
    super::run::enqueue(
        db,
        super::run::Enqueue {
            session_id: &session_id,
            turn_id: None,
            agent_id: &agent_id,
            parent_run_id: None,
            assignment,
            status: "queued",
            now: &now,
        },
    )?;
    Ok(true)
}

/// 轻量系统同步：
/// - 开跑：backlog/todo → in_progress
/// - 成功结束且无活跃 run：in_progress → in_review（等人审）
/// - 失败且无活跃 run：in_progress → todo
/// `done` / `blocked` 仍由智能体或用户显式写入。
pub fn sync_from_run(
    db: &Connection,
    session_id: &str,
    run_status: &str,
    now: &str,
) -> Result<(), String> {
    match run_status {
        "queued" | "running" => {
            db.execute(
                "UPDATE tasks SET status='in_progress', updated_at=?1
                 WHERE session_id=?2 AND status IN ('backlog', 'todo')",
                params![now, session_id],
            )
            .map_err(|error| error.to_string())?;
        }
        "completed" | "failed" => {
            let active: u32 = db
                .query_row(
                    "SELECT COUNT(*) FROM agent_runs
                     WHERE session_id=?1 AND status IN ('queued', 'running')",
                    [session_id],
                    |row| row.get(0),
                )
                .unwrap_or(1);
            if active == 0 {
                let next = if run_status == "completed" {
                    "in_review"
                } else {
                    "todo"
                };
                db.execute(
                    "UPDATE tasks SET status=?1, updated_at=?2
                     WHERE session_id=?3 AND status='in_progress'",
                    params![next, now, session_id],
                )
                .map_err(|error| error.to_string())?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn assign(db: &Connection, task_id: &str, agent_id: &str) -> Result<Task, String> {
    require_agent(db, agent_id)?;
    let updated = db
        .execute(
            "UPDATE tasks SET agent_id=?1, updated_at=?2 WHERE id=?3",
            params![agent_id, Utc::now().to_rfc3339(), task_id],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("任务不存在".to_string());
    }
    db.execute(
        "UPDATE chat_sessions SET target_type='agent', target_id=?1, updated_at=?2
         WHERE id=(SELECT session_id FROM tasks WHERE id=?3)",
        params![agent_id, Utc::now().to_rfc3339(), task_id],
    )
    .map_err(|error| error.to_string())?;
    get(db, task_id)
}

pub fn comment(db: &Connection, task_id: &str, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("评论不能为空".to_string());
    }
    let session_id: String = db
        .query_row(
            "SELECT session_id FROM tasks WHERE id=?1",
            [task_id],
            |row| row.get(0),
        )
        .map_err(|_| "任务不存在".to_string())?;
    let now = Utc::now().to_rfc3339();
    db.execute(
        "INSERT INTO messages
         (id, session_id, role, content, message_kind, quick_actions_json, created_at)
         VALUES (?1, ?2, 'system', ?3, 'message', '[]', ?4)",
        params![Uuid::new_v4().to_string(), session_id, text, now],
    )
    .map_err(|error| error.to_string())?;
    db.execute(
        "UPDATE tasks SET updated_at=?1 WHERE id=?2",
        params![now, task_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn runs(db: &Connection, task_id: &str) -> Result<serde_json::Value, String> {
    let session_id: String = db
        .query_row(
            "SELECT session_id FROM tasks WHERE id=?1",
            [task_id],
            |row| row.get(0),
        )
        .map_err(|_| "任务不存在".to_string())?;
    let mut statement = db
        .prepare(
            "SELECT id, agent_id, runtime_id, status, assignment, created_at
             FROM agent_runs WHERE session_id=?1 ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([&session_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "agentId": row.get::<_, String>(1)?,
                "runtimeId": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "assignment": row.get::<_, String>(4)?,
                "createdAt": row.get::<_, String>(5)?,
            }))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(serde_json::Value::Array(rows))
}

pub fn update_meta(
    db: &Connection,
    task_id: &str,
    priority: Option<&str>,
    labels: Option<&[String]>,
    checklist: Option<&[TaskChecklistItem]>,
    pr_url: Option<&str>,
    parent_task_id: Option<Option<&str>>,
) -> Result<(), String> {
    get(db, task_id)?;
    let now = Utc::now().to_rfc3339();
    if let Some(priority) = priority {
        db.execute(
            "UPDATE tasks SET priority=?1, updated_at=?2 WHERE id=?3",
            params![parse_priority(priority), now, task_id],
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(labels) = labels {
        let json = serde_json::to_string(labels).unwrap_or_else(|_| "[]".into());
        db.execute(
            "UPDATE tasks SET labels_json=?1, updated_at=?2 WHERE id=?3",
            params![json, now, task_id],
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(checklist) = checklist {
        let json = serde_json::to_string(checklist).unwrap_or_else(|_| "[]".into());
        db.execute(
            "UPDATE tasks SET checklist_json=?1, updated_at=?2 WHERE id=?3",
            params![json, now, task_id],
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(pr_url) = pr_url {
        db.execute(
            "UPDATE tasks SET pr_url=?1, updated_at=?2 WHERE id=?3",
            params![pr_url.trim(), now, task_id],
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(parent) = parent_task_id {
        if parent == Some(task_id) {
            return Err("任务不能作为自己的子任务".to_string());
        }
        db.execute(
            "UPDATE tasks SET parent_task_id=?1, updated_at=?2 WHERE id=?3",
            params![parent, now, task_id],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn update(
    db: &mut Connection,
    task_id: &str,
    title: Option<&str>,
    description: Option<&str>,
    status: Option<&str>,
    agent_id: Option<&str>,
    comment_text: Option<&str>,
) -> Result<Task, String> {
    get(db, task_id)?;
    if let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) {
        db.execute(
            "UPDATE tasks SET title=?1, updated_at=?2 WHERE id=?3",
            params![title, Utc::now().to_rfc3339(), task_id],
        )
        .map_err(|error| error.to_string())?;
        db.execute(
            "UPDATE chat_sessions SET title=?1, updated_at=?2
             WHERE id=(SELECT session_id FROM tasks WHERE id=?3)",
            params![title, Utc::now().to_rfc3339(), task_id],
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(description) = description {
        db.execute(
            "UPDATE tasks SET description=?1, updated_at=?2 WHERE id=?3",
            params![description.trim(), Utc::now().to_rfc3339(), task_id],
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(agent_id) = agent_id {
        assign(db, task_id, agent_id)?;
    }
    if let Some(status) = status {
        update_status(db, task_id, status)?;
    }
    if let Some(comment_text) = comment_text {
        comment(db, task_id, comment_text)?;
    }
    get(db, task_id)
}
