use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::models::Project;

use super::require_project;

pub fn list(db: &Connection) -> Result<Vec<Project>, String> {
    let mut statement = db
        .prepare(
            "SELECT p.id, p.name, p.note, COUNT(w.id), p.created_at, p.opened_at, p.archived_at
             FROM projects p
             LEFT JOIN workspaces w ON w.project_id = p.id AND w.session_id IS NULL
             GROUP BY p.id
             ORDER BY p.opened_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let projects = statement
        .query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                note: row.get(2)?,
                workspace_count: row.get(3)?,
                created_at: row.get(4)?,
                opened_at: row.get(5)?,
                archived_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(projects)
}

pub fn get(db: &Connection, project_id: &str) -> Result<Project, String> {
    list(db)?
        .into_iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| tr!("需求不存在"))
}

pub fn create(db: &Connection, name: &str, note: &str) -> Result<Project, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(tr!("需求名称不能为空"));
    }
    let now = Utc::now().to_rfc3339();
    let project = Project {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        note: note.trim().to_string(),
        workspace_count: 0,
        created_at: now.clone(),
        opened_at: now,
        archived_at: None,
    };
    db.execute(
        "INSERT INTO projects (id, name, note, created_at, opened_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            project.id,
            project.name,
            project.note,
            project.created_at,
            project.opened_at
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(project)
}

pub fn update(
    db: &Connection,
    project_id: &str,
    name: Option<&str>,
    note: Option<&str>,
) -> Result<Project, String> {
    require_project(db, project_id)?;
    if let Some(name) = name.map(str::trim).filter(|value| !value.is_empty()) {
        db.execute(
            "UPDATE projects SET name=?1 WHERE id=?2",
            params![name, project_id],
        )
        .map_err(|error| error.to_string())?;
    }
    if let Some(note) = note {
        db.execute(
            "UPDATE projects SET note=?1 WHERE id=?2",
            params![note.trim(), project_id],
        )
        .map_err(|error| error.to_string())?;
    }
    get(db, project_id)
}
