use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::{database::AppState, models::Workspace};

pub fn project_lock(state: &AppState, project_id: &str) -> Result<Arc<Mutex<()>>, String> {
    let mut locks = state
        .project_locks
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(locks
        .entry(project_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone())
}

pub fn project_locks(
    state: &AppState,
    project_ids: &[String],
) -> Result<Vec<Arc<Mutex<()>>>, String> {
    let project_ids = sorted_project_ids(project_ids);
    project_ids
        .iter()
        .map(|project_id| project_lock(state, project_id))
        .collect()
}

fn sorted_project_ids(project_ids: &[String]) -> Vec<String> {
    let mut project_ids = project_ids.to_vec();
    project_ids.sort();
    project_ids.dedup();
    project_ids
}

pub fn project_id(db: &Connection, session_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT project_id FROM chat_sessions WHERE id=?1",
        [session_id],
        |row| row.get(0),
    )
    .map_err(|_| "聊天不存在".to_string())
}

pub fn linked_project_ids(db: &Connection, session_id: &str) -> Result<Vec<String>, String> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id=?1)",
            [session_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !exists {
        return Err("聊天不存在".to_string());
    }
    let mut statement = db
        .prepare(
            "SELECT project_id FROM session_projects
             WHERE session_id=?1 ORDER BY sort_order, project_id",
        )
        .map_err(|error| error.to_string())?;
    let project_ids = statement
        .query_map([session_id], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    if !project_ids.is_empty() {
        return Ok(project_ids);
    }
    Ok(project_id(db, session_id)?.into_iter().collect())
}

pub fn replace_linked_projects(
    db: &Connection,
    session_id: &str,
    project_ids: &[String],
) -> Result<(), String> {
    db.execute(
        "DELETE FROM session_projects WHERE session_id=?1",
        [session_id],
    )
    .map_err(|error| error.to_string())?;
    for (sort_order, project_id) in project_ids.iter().enumerate() {
        db.execute(
            "INSERT INTO session_projects (session_id, project_id, sort_order)
             VALUES (?1, ?2, ?3)",
            params![session_id, project_id, sort_order as i64],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn session_workspaces(db: &Connection, session_id: &str) -> Result<Vec<Workspace>, String> {
    let project_ids = linked_project_ids(db, session_id)?;
    let mut workspaces = db
        .prepare(
            "SELECT w.id, w.project_id, w.session_id, w.source_workspace_id, w.name, w.kind,
                    w.source_path, w.work_path, w.branch, w.isolation_mode, w.created_at
             FROM session_workspaces sw
             JOIN workspaces w ON w.id=sw.workspace_id
             WHERE sw.session_id=?1 ORDER BY w.created_at",
        )
        .map_err(|error| error.to_string())?
        .query_map([session_id], workspace_from_row)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut statement = db
        .prepare(
            "SELECT id, project_id, session_id, source_workspace_id, name, kind,
                    source_path, work_path, branch, isolation_mode, created_at
             FROM workspaces
             WHERE project_id=?1 AND session_id IS NULL
             ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    for project_id in project_ids {
        let rows = statement
            .query_map([project_id], workspace_from_row)
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        for workspace in rows {
            if !workspaces.iter().any(|item| item.id == workspace.id) {
                workspaces.push(workspace);
            }
        }
    }
    Ok(workspaces)
}

pub(crate) fn ensure_session_idle(db: &Connection, session_id: &str) -> Result<(), String> {
    let active: bool = db
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM agent_runs
               WHERE session_id=?1 AND status IN ('queued', 'running')
             )",
            [session_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active {
        Err("当前会话仍有智能体正在运行，此运行无法追加消息，请等待结束或停止后发送".to_string())
    } else {
        Ok(())
    }
}

pub(crate) fn ensure_project_idle(db: &Connection, project_id: &str) -> Result<(), String> {
    let active: bool = db
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM agent_runs r
               JOIN chat_sessions s ON s.id=r.session_id
               WHERE r.status IN ('queued', 'running')
                 AND (
                   EXISTS (
                     SELECT 1 FROM session_projects sp
                     WHERE sp.session_id=s.id AND sp.project_id=?1
                   )
                   OR (
                     s.project_id=?1
                     AND NOT EXISTS (
                       SELECT 1 FROM session_projects sp WHERE sp.session_id=s.id
                     )
                   )
                 )
             )",
            [project_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active {
        Err("项目仍有智能体正在运行".to_string())
    } else {
        Ok(())
    }
}

pub fn complete_turn(app: &AppHandle, turn_id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    let already_done: bool = db
        .query_row(
            "SELECT status != 'running' FROM agent_turns WHERE id=?1",
            [turn_id],
            |row| row.get(0),
        )
        .unwrap_or(true);
    if already_done {
        return Ok(());
    }
    let active: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs
             WHERE turn_id=?1 AND status IN ('queued', 'running')",
            [turn_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active > 0 {
        return Ok(());
    }
    let failed: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE turn_id=?1 AND status IN ('failed', 'stopped')",
            [turn_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let status = if failed == 0 { "completed" } else { "failed" };
    let now = Utc::now().to_rfc3339();
    db.execute(
        "UPDATE agent_turns SET status=?1, completed_at=?2 WHERE id=?3",
        params![status, now, turn_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn workspace_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    let kind: String = row.get(5)?;
    let isolation_mode: String = row.get(9)?;
    Ok(Workspace {
        id: row.get(0)?,
        project_id: row.get(1)?,
        session_id: row.get(2)?,
        source_workspace_id: row.get(3)?,
        name: row.get(4)?,
        kind: if kind == "git" { "git" } else { "folder" },
        source_path: row.get(6)?,
        work_path: row.get(7)?,
        branch: row.get(8)?,
        isolation_mode: match isolation_mode.as_str() {
            "worktree" => "worktree",
            "copy" => "copy",
            _ => "direct",
        },
        created_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::{ensure_project_idle, ensure_session_idle, linked_project_ids, session_workspaces, sorted_project_ids};
    use rusqlite::Connection;

    fn database() -> Connection {
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
               ('s1', 'p2', 1);
             INSERT INTO workspaces
               (id, project_id, name, kind, source_path, work_path, isolation_mode, created_at)
             VALUES
               ('w1', 'p1', '仓库一', 'folder', '/p1', '/p1', 'direct', '1'),
               ('w2', 'p2', '仓库二', 'folder', '/p2', '/p2', 'direct', '2');",
        )
        .unwrap();
        db
    }

    #[test]
    fn returns_linked_projects_and_all_project_workspaces_in_order() {
        let db = database();

        assert_eq!(linked_project_ids(&db, "s1").unwrap(), ["p1", "p2"]);
        let workspaces = session_workspaces(&db, "s1").unwrap();
        assert_eq!(
            workspaces
                .iter()
                .map(|workspace| workspace.id.as_str())
                .collect::<Vec<_>>(),
            ["w1", "w2"]
        );
    }

    #[test]
    fn idle_check_finds_runs_linked_through_any_project() {
        let db = database();
        db.execute_batch(
            "INSERT INTO agents
               (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('a1', '智能体', '', '', '', 'cursor', 'auto', 1);
             INSERT INTO agent_runs
               (id, session_id, agent_id, runtime_id, assignment, status, created_at)
             VALUES ('r1', 's1', 'a1', 'cursor', '任务', 'running', 'now');",
        )
        .unwrap();

        assert!(ensure_project_idle(&db, "p2").is_err());
    }

    #[test]
    fn session_idle_check_allows_other_tabs_but_preserves_project_disk_guard() {
        let db = database();
        db.execute_batch(
            "INSERT INTO agents
               (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('a1', 'Agent', '', '', '', 'cursor', 'auto', 1);
             INSERT INTO chat_sessions
               (id, project_id, title, target_type, target_id, created_at, updated_at)
             VALUES ('s2', 'p1', '另一会话', 'agent', 'a1', 'now', 'now');
             INSERT INTO agent_runs
               (id, session_id, agent_id, runtime_id, assignment, status, created_at)
             VALUES ('r1', 's1', 'a1', 'cursor', '任务', 'running', 'now');",
        )
        .unwrap();

        assert!(ensure_session_idle(&db, "s2").is_ok());
        let error = ensure_session_idle(&db, "s1").unwrap_err();
        assert!(error.contains("当前会话"));
        assert!(error.contains("停止后发送"));
        assert!(ensure_project_idle(&db, "p1").is_err());
        assert!(ensure_project_idle(&db, "p2").is_err());

        db.execute("UPDATE agent_runs SET status='queued' WHERE id='r1'", []).unwrap();
        assert!(ensure_session_idle(&db, "s1").is_err());
        db.execute("UPDATE agent_runs SET status='completed' WHERE id='r1'", []).unwrap();
        assert!(ensure_session_idle(&db, "s1").is_ok());
        assert!(ensure_project_idle(&db, "p1").is_ok());
    }

    #[test]
    fn project_locks_use_stable_sorted_unique_order() {
        assert_eq!(
            sorted_project_ids(&["p2".to_string(), "p1".to_string(), "p2".to_string()]),
            ["p1", "p2"]
        );
    }
}
