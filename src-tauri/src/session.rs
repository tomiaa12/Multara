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

pub fn project_id(db: &Connection, session_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT project_id FROM chat_sessions WHERE id=?1",
        [session_id],
        |row| row.get(0),
    )
    .map_err(|_| "聊天不存在".to_string())
}

pub fn session_workspaces(db: &Connection, session_id: &str) -> Result<Vec<Workspace>, String> {
    let mut explicit = db
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
    if !explicit.is_empty() {
        return Ok(explicit);
    }
    let Some(project_id) = project_id(db, session_id)? else {
        return Ok(Vec::new());
    };
    explicit = db
        .prepare(
            "SELECT id, project_id, session_id, source_workspace_id, name, kind,
                    source_path, work_path, branch, isolation_mode, created_at
             FROM workspaces
             WHERE project_id=?1 AND session_id IS NULL
             ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?
        .query_map([project_id], workspace_from_row)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(explicit)
}

pub(crate) fn ensure_project_idle(db: &Connection, project_id: &str) -> Result<(), String> {
    let active: bool = db
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM agent_runs r
               JOIN chat_sessions s ON s.id=r.session_id
               WHERE s.project_id=?1 AND r.status IN ('queued', 'running')
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
