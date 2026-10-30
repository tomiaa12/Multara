use std::collections::HashMap;

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::models::{Squad, SquadInput};

use super::require_agent;

pub fn list(db: &Connection) -> Result<Vec<Squad>, String> {
    let mut statement = db
        .prepare("SELECT id, name, description, instructions, leader_agent_id FROM squads ORDER BY name")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut members: HashMap<String, Vec<String>> = HashMap::new();
    db.prepare("SELECT squad_id, agent_id FROM squad_members ORDER BY agent_id")
        .map_err(|error| error.to_string())?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|error| error.to_string())?
        .try_for_each(|row| {
            let (squad_id, agent_id) = row?;
            members.entry(squad_id).or_default().push(agent_id);
            Ok::<_, rusqlite::Error>(())
        })
        .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, name, description, instructions, leader_agent_id)| Squad {
            member_agent_ids: members.remove(&id).unwrap_or_default(),
            id,
            name,
            description,
            instructions,
            leader_agent_id,
        })
        .collect())
}

pub fn get(db: &Connection, squad_id: &str) -> Result<Squad, String> {
    list(db)?
        .into_iter()
        .find(|squad| squad.id == squad_id)
        .ok_or_else(|| "小队不存在".to_string())
}

pub fn save(db: &mut Connection, input: SquadInput) -> Result<Vec<Squad>, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("小队名称不能为空".to_string());
    }
    if !input.member_agent_ids.contains(&input.leader_agent_id) {
        return Err("队长必须同时是小队成员".to_string());
    }
    for agent_id in &input.member_agent_ids {
        require_agent(db, agent_id)?;
    }
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let transaction = db.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO squads (id, name, description, instructions, leader_agent_id) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description,
               instructions=excluded.instructions, leader_agent_id=excluded.leader_agent_id",
            params![id, name, input.description.trim(), input.instructions.trim(), input.leader_agent_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM squad_members WHERE squad_id = ?1", [&id])
        .map_err(|error| error.to_string())?;
    for agent_id in &input.member_agent_ids {
        transaction
            .execute(
                "INSERT INTO squad_members (squad_id, agent_id) VALUES (?1, ?2)",
                params![id, agent_id],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    list(db)
}

pub fn create(
    db: &mut Connection,
    name: &str,
    description: &str,
    lead_id: &str,
    member_ids: Vec<String>,
) -> Result<Squad, String> {
    let id = Uuid::new_v4().to_string();
    save(
        db,
        SquadInput {
            id: Some(id.clone()),
            name: name.to_string(),
            description: description.to_string(),
            instructions: String::new(),
            leader_agent_id: lead_id.to_string(),
            member_agent_ids: member_ids,
        },
    )?;
    get(db, &id)
}

pub fn set_members(
    db: &mut Connection,
    squad_id: &str,
    member_ids: Vec<String>,
) -> Result<Squad, String> {
    let squad = get(db, squad_id)?;
    save(
        db,
        SquadInput {
            id: Some(squad.id),
            name: squad.name,
            description: squad.description,
            instructions: squad.instructions,
            leader_agent_id: squad.leader_agent_id,
            member_agent_ids: member_ids,
        },
    )?;
    get(db, squad_id)
}
