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
    pub project_ids: &'a [String],
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
    if let Some(parent_id) = input.parent_task_id {
        let exists: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE id=?1",
                [parent_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists == 0 {
            return Err("父任务不存在".to_string());
        }
    }
    let status = parse_task_status(input.status.unwrap_or("todo"))?;
    let mut project_ids = input
        .project_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if project_ids.is_empty() {
        project_ids.extend(
            input
                .project_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        );
    }
    if project_ids.is_empty() {
        if let Some(session_id) = input.source_session_id {
            project_ids = crate::session::linked_project_ids(db, session_id)?;
        }
    }
    let mut unique_project_ids = Vec::new();
    for project_id in project_ids {
        if !unique_project_ids.contains(&project_id) {
            unique_project_ids.push(project_id);
        }
    }
    for project_id in &unique_project_ids {
        require_project(db, project_id)?;
    }
    let project_id = unique_project_ids.first();

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
    crate::session::replace_linked_projects(&transaction, &session_id, &unique_project_ids)?;
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
    // 挂在父任务下时：在父会话写一条 @指派，详情动态里能看到队长/负责人派活。
    if let Some(parent_id) = input.parent_task_id {
        let _ = record_parent_assignment(&transaction, parent_id, input.agent_id, title, &task_id, &now);
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

fn record_parent_assignment(
    db: &Connection,
    parent_task_id: &str,
    assignee_id: &str,
    child_title: &str,
    child_id: &str,
    now: &str,
) -> Result<(), String> {
    let (parent_session, parent_agent): (String, String) = db
        .query_row(
            "SELECT session_id, agent_id FROM tasks WHERE id=?1",
            [parent_task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "父任务不存在".to_string())?;
    let spoken = format!("请处理子任务「{child_title}」（`{child_id}`），完成后向我汇报。");
    db.execute(
        "INSERT INTO messages
         (id, session_id, role, content, agent_id, audience_type, audience_id, message_kind, created_at)
         VALUES (?1, ?2, 'assistant', ?3, ?4, 'agent', ?5, 'assignment', ?6)",
        params![
            Uuid::new_v4().to_string(),
            parent_session,
            spoken,
            parent_agent,
            assignee_id,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// 子任务全部收口后，给父任务负责人再排一轮，汇总结果。
/// 返回 `(run_id, parent_session_id, parent_agent_id)`。
pub fn maybe_resume_parent(
    db: &Connection,
    child_session_id: &str,
    now: &str,
) -> Result<Option<(String, String, String)>, String> {
    let Some((child_id, child_title, parent_id)) = db
        .query_row(
            "SELECT id, title, parent_task_id FROM tasks WHERE session_id=?1",
            [child_session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let Some(parent_id) = parent_id.filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    // 旧数据里可能已有 A→B→A 的环，不拦的话两边会互相排恢复运行，永不停止。
    if parent_chain_contains(db, &parent_id, &child_id)? {
        return Ok(None);
    }

    let open_children: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM tasks
             WHERE parent_task_id=?1
               AND status IN ('backlog', 'todo', 'in_progress', 'blocked')",
            [&parent_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if open_children > 0 {
        return Ok(None);
    }

    let (parent_session, parent_agent, parent_title): (String, String, String) = db
        .query_row(
            "SELECT session_id, agent_id, title FROM tasks WHERE id=?1",
            [&parent_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| "父任务不存在".to_string())?;

    let active: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs
             WHERE session_id=?1 AND status IN ('queued', 'running')",
            [&parent_session],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active > 0 {
        return Ok(None);
    }

    let mut lines = Vec::new();
    {
        let mut statement = db
            .prepare(
                "SELECT id, title, status, agent_id FROM tasks
                 WHERE parent_task_id=?1
                 ORDER BY updated_at ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([&parent_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let (id, title, status, agent_id) = row.map_err(|error| error.to_string())?;
            let name: String = db
                .query_row("SELECT name FROM agents WHERE id=?1", [&agent_id], |r| r.get(0))
                .unwrap_or(agent_id);
            lines.push(format!("- [{status}] {title}（`{id}`，@{name}）"));
        }
    }
    let summary = lines.join("\n");
    let assignment = format!(
        "子任务已全部收口（刚结束的是「{child_title}」`{child_id}`）。\n\
         请汇总各成员结果，更新父任务「{parent_title}」进度；若可验收则 `multara task status {parent_id} in_review`。\n\n\
         子任务状态：\n{summary}"
    );

    let run_id = super::run::enqueue(
        db,
        super::run::Enqueue {
            session_id: &parent_session,
            turn_id: None,
            agent_id: &parent_agent,
            parent_run_id: None,
            assignment: &assignment,
            status: "queued",
            now,
        },
    )?;
    Ok(Some((run_id, parent_session, parent_agent)))
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
    db.query_row(
        &format!(
            "SELECT {TASK_COLUMNS}
             FROM tasks t
             JOIN chat_sessions s ON s.id = t.session_id
             WHERE t.id = ?1"
        ),
        [task_id],
        map_task,
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "任务不存在".to_string())
}

const IDLE_IN_PROGRESS: &str = "status='in_progress'
  AND NOT EXISTS (
    SELECT 1 FROM agent_runs r
    WHERE r.session_id = tasks.session_id AND r.status IN ('queued', 'running')
  )";

const LAST_RUN_STATUS: &str = "(
    SELECT r.status FROM agent_runs r
    WHERE r.session_id = tasks.session_id
    ORDER BY COALESCE(r.completed_at, r.created_at) DESC
    LIMIT 1
  )";

/// 回答已经结束、没有还在排队或运行的记录，却停在进行中时收口。
/// 最后一次成功进审核；最后一次失败回待办。用户手动停掉的不挪。
/// 只在打开看板时调用，避免刚拖进进行中又被立刻弹走。
pub(crate) fn settle_finished(db: &Connection) -> Result<(), String> {
    // 绝大多数时候没有可收口的任务，先只读判断，免得每次打开看板都开写事务。
    let settleable: bool = db
        .query_row(
            &format!(
                "SELECT EXISTS(
                   SELECT 1 FROM tasks
                   WHERE {IDLE_IN_PROGRESS} AND {LAST_RUN_STATUS} IN ('completed', 'failed')
                 )"
            ),
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !settleable {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    for (next, last_run) in [("in_review", "completed"), ("todo", "failed")] {
        db.execute(
            &format!(
                "UPDATE tasks SET status=?1, updated_at=?2
                 WHERE {IDLE_IN_PROGRESS} AND {LAST_RUN_STATUS} = ?3"
            ),
            params![next, now, last_run],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// `ancestor_id` 沿 `parent_task_id` 往上能不能走到 `task_id`（包括两者相同）。
/// `UNION` 去重，库里已有环时也会终止。
fn parent_chain_contains(db: &Connection, ancestor_id: &str, task_id: &str) -> Result<bool, String> {
    db.query_row(
        "WITH RECURSIVE chain(id) AS (
           SELECT ?1
           UNION
           SELECT t.parent_task_id FROM tasks t JOIN chain c ON t.id = c.id
           WHERE t.parent_task_id IS NOT NULL AND t.parent_task_id != ''
         )
         SELECT EXISTS(SELECT 1 FROM chain WHERE id = ?2)",
        params![ancestor_id, task_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
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
               AND (
                 ?3 IS NULL
                 OR EXISTS (
                   SELECT 1 FROM session_projects sp
                   WHERE sp.session_id=s.id AND sp.project_id=?3
                 )
                 OR (
                   s.project_id=?3
                   AND NOT EXISTS (
                     SELECT 1 FROM session_projects sp WHERE sp.session_id=s.id
                   )
                 )
               )
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
        if let Some(parent_id) = parent {
            if parent_chain_contains(db, parent_id, task_id)? {
                return Err("不能把任务挂到它自己的子任务下".to_string());
            }
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

#[cfg(test)]
mod tests {
    use super::{create, list, CreateTaskInput};
    use rusqlite::Connection;

    #[test]
    fn creates_and_starts_tasks_while_another_tab_in_the_project_is_running() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO projects (id, name, created_at, opened_at)
             VALUES ('project', 'Project', 'now', 'now');
             INSERT INTO agents
               (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('agent', 'Agent', '', '', '', 'cursor', 'auto', 1);
             INSERT INTO chat_sessions
               (id, project_id, title, target_type, target_id, created_at, updated_at)
             VALUES ('other', 'project', 'Other', 'agent', 'agent', 'now', 'now');
             INSERT INTO agent_runs
               (id, session_id, agent_id, runtime_id, assignment, status, created_at)
             VALUES ('other-run', 'other', 'agent', 'cursor', 'Work', 'running', 'now');",
        )
        .unwrap();

        let queued = create(&mut db, CreateTaskInput {
            title: "Parallel task", description: "", agent_id: "agent",
            project_id: Some("project"), project_ids: &[], source_session_id: None,
            status: Some("todo"), parent_task_id: None,
        }).unwrap();
        assert!(queued.queued);
        assert!(!super::start_if_idle(&db, &queued.task.id).unwrap());

        let backlog = create(&mut db, CreateTaskInput {
            title: "Start later", description: "Task details", agent_id: "agent",
            project_id: Some("project"), project_ids: &[], source_session_id: None,
            status: Some("backlog"), parent_task_id: None,
        }).unwrap();
        assert!(!backlog.queued);
        assert!(super::start_if_idle(&db, &backlog.task.id).unwrap());
        assert!(!super::start_if_idle(&db, &backlog.task.id).unwrap());
        let queued_count: i64 = db.query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE status='queued'", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(queued_count, 2);
    }

    #[test]
    fn creates_multi_project_task_with_compatible_primary_and_filters() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO projects (id, name, created_at, opened_at) VALUES
               ('p1', '需求一', 'now', 'now'),
               ('p2', '需求二', 'now', 'now');
             INSERT INTO agents
               (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('a1', '智能体', '', '', '', 'cursor', 'auto', 1);",
        )
        .unwrap();

        let created = create(
            &mut db,
            CreateTaskInput {
                title: "跨需求任务",
                description: "",
                agent_id: "a1",
                project_id: None,
                project_ids: &["p1".to_string(), "p2".to_string()],
                source_session_id: None,
                status: Some("backlog"),
                parent_task_id: None,
            },
        )
        .unwrap();

        let primary: String = db
            .query_row(
                "SELECT project_id FROM chat_sessions WHERE id=?1",
                [&created.task.session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(primary, "p1");
        let linked = crate::session::linked_project_ids(&db, &created.task.session_id).unwrap();
        assert_eq!(linked, ["p1", "p2"]);
        assert_eq!(list(&db, None, None, Some("p1")).unwrap().len(), 1);
        assert_eq!(list(&db, None, None, Some("p2")).unwrap().len(), 1);
    }

    #[test]
    fn rejects_parent_cycles_and_settles_only_when_needed() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO agents
               (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('a1', '智能体', '', '', '', 'cursor', 'auto', 1);",
        )
        .unwrap();
        let input = |title| CreateTaskInput {
            title,
            description: "",
            agent_id: "a1",
            project_id: None,
            project_ids: &[],
            source_session_id: None,
            status: Some("backlog"),
            parent_task_id: None,
        };
        let a = create(&mut db, input("A")).unwrap().task;
        let b = create(&mut db, input("B")).unwrap().task;
        super::update_meta(&db, &b.id, None, None, None, None, Some(Some(&a.id))).unwrap();
        let cycle = super::update_meta(&db, &a.id, None, None, None, None, Some(Some(&b.id)));
        assert!(cycle.is_err());
        assert_eq!(super::get(&db, &b.id).unwrap().parent_task_id.as_deref(), Some(a.id.as_str()));
        assert!(super::get(&db, "missing").is_err());

        super::settle_finished(&db).unwrap();
        db.execute(
            "UPDATE tasks SET status='in_progress' WHERE id=?1",
            [&a.id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO agent_runs (id, session_id, agent_id, runtime_id, assignment, status, created_at)
             VALUES ('r1', ?1, 'a1', 'cursor', 'A', 'completed', 'now')",
            [&a.session_id],
        )
        .unwrap();
        super::settle_finished(&db).unwrap();
        assert_eq!(super::get(&db, &a.id).unwrap().status, "in_review");
    }
}
