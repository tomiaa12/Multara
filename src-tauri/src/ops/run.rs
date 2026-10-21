use rusqlite::{params, Connection};
use uuid::Uuid;

pub struct Enqueue<'a> {
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub agent_id: &'a str,
    pub parent_run_id: Option<&'a str>,
    pub assignment: &'a str,
    pub status: &'a str,
    pub now: &'a str,
}

pub fn enqueue(db: &Connection, run: Enqueue<'_>) -> Result<String, String> {
    let runtime_id: String = db
        .query_row(
            "SELECT runtime_id FROM agents WHERE id = ?1",
            [run.agent_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let id = Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO agent_runs
         (id, session_id, turn_id, agent_id, runtime_id, parent_run_id, assignment, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            run.session_id,
            run.turn_id,
            run.agent_id,
            runtime_id,
            run.parent_run_id,
            run.assignment,
            run.status,
            run.now
        ],
    )
    .map_err(|error| error.to_string())?;
    let _ = crate::ops::task::sync_from_run(db, run.session_id, "queued", run.now);
    Ok(id)
}
