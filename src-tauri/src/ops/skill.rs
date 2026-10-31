use rusqlite::{params, Connection};

use crate::{builtin_skills, resources};

pub fn list_scanned() -> Vec<crate::models::GlobalResource> {
    resources::scan()
        .into_iter()
        .filter(|resource| resource.kind == "skill")
        .collect()
}

pub fn get(id: &str) -> Result<crate::models::GlobalResource, String> {
    list_scanned()
        .into_iter()
        .find(|resource| resource.id == id)
        .ok_or_else(|| tr!("Skill 不存在"))
}

pub fn builtin(locale: &str) -> serde_json::Value {
    serde_json::Value::Array(
        builtin_skills::ALL
            .iter()
            .map(|skill| {
                serde_json::json!({
                    "name": skill.name,
                    "description": skill.description_for(locale),
                })
            })
            .collect(),
    )
}

pub fn comment_history(db: &Connection, session_id: &str) -> Result<serde_json::Value, String> {
    let mut statement = db
        .prepare(
            "SELECT id, role, content, created_at FROM messages
             WHERE session_id=?1
             ORDER BY created_at, rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![session_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "role": row.get::<_, String>(1)?,
                "content": row.get::<_, String>(2)?,
                "createdAt": row.get::<_, String>(3)?,
            }))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "sessionId": session_id,
        "messages": rows,
    }))
}
