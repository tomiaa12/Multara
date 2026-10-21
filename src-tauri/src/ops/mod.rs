pub mod agent;
pub mod automation;
pub mod chat;
pub mod inbox;
pub mod project;
pub mod run;
pub mod session;
pub mod skill;
pub mod squad;
pub mod task;

use rusqlite::Connection;

pub fn json<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|error| error.to_string())
}

pub fn require_agent(db: &Connection, agent_id: &str) -> Result<(), String> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1)",
            [agent_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if exists {
        Ok(())
    } else {
        Err("指定的智能体不存在".to_string())
    }
}

pub fn require_project(db: &Connection, project_id: &str) -> Result<(), String> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
            [project_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if exists {
        Ok(())
    } else {
        Err("需求不存在".to_string())
    }
}

pub const TASK_STATUSES: &[&str] = &[
    "backlog",
    "todo",
    "in_progress",
    "in_review",
    "done",
    "blocked",
    "cancelled",
];

pub fn parse_task_status(status: &str) -> Result<&'static str, String> {
    TASK_STATUSES
        .iter()
        .copied()
        .find(|item| *item == status)
        .ok_or_else(|| "任务状态无效".to_string())
}
