use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::{
    builtin_agents::{self, MIKU_SYSTEM_KEY},
    models::{Agent, AgentInput},
    runtime,
};

pub fn list(db: &Connection) -> Result<Vec<Agent>, String> {
    let locale = crate::app_locale::from_db(db);
    let mut statement = db
        .prepare(
            "SELECT id, name, avatar, description, instructions, runtime_id, model,
                    max_concurrent_tasks, system_key
             FROM agents
             ORDER BY CASE WHEN system_key = 'miku' THEN 0 ELSE 1 END, name",
        )
        .map_err(|error| error.to_string())?;
    let mut agents = statement
        .query_map([], |row| {
            let system_key: Option<String> = row.get(8)?;
            let name: String = row.get(1)?;
            let instructions: String = row.get(4)?;
            let system_instructions = if builtin_agents::is_miku_key(system_key.as_deref()) {
                Some(builtin_agents::miku_system_instructions(&name, locale))
            } else {
                None
            };
            Ok(Agent {
                id: row.get(0)?,
                name,
                avatar: row.get(2)?,
                description: row.get(3)?,
                instructions,
                runtime_id: row.get(5)?,
                model: row.get(6)?,
                max_concurrent_tasks: row.get(7)?,
                skill_ids: Vec::new(),
                mcp_ids: Vec::new(),
                system_key,
                system_instructions,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut resources = db
        .prepare("SELECT agent_id, resource_id, kind FROM agent_resources ORDER BY resource_id")
        .map_err(|error| error.to_string())?;
    let mut assigned: HashMap<String, Vec<(String, String)>> = HashMap::new();
    resources
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .try_for_each(|row| {
            let (agent_id, resource_id, kind) = row?;
            assigned
                .entry(agent_id)
                .or_default()
                .push((resource_id, kind));
            Ok::<_, rusqlite::Error>(())
        })
        .map_err(|error| error.to_string())?;
    for agent in &mut agents {
        for (resource_id, kind) in assigned.remove(&agent.id).unwrap_or_default() {
            if kind == "skill" {
                agent.skill_ids.push(resource_id);
            } else {
                agent.mcp_ids.push(resource_id);
            }
        }
    }
    Ok(agents)
}

pub fn get(db: &Connection, agent_id: &str) -> Result<Agent, String> {
    list(db)?
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| tr!("智能体不存在"))
}

pub fn system_key(db: &Connection, agent_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT system_key FROM agents WHERE id=?1",
        [agent_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| tr!("智能体不存在"))
}

pub fn save(
    db: &mut Connection,
    input: AgentInput,
    require_online: bool,
) -> Result<Vec<Agent>, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(tr!("智能体名称不能为空"));
    }
    if !(1..=6).contains(&input.max_concurrent_tasks) {
        return Err(tr!("智能体并发必须在 1 到 6 之间"));
    }
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if id == MIKU_SYSTEM_KEY && input.id.is_none() {
        return Err(tr!("不能创建与总管相同的标识"));
    }
    let existing_key = db
        .query_row("SELECT system_key FROM agents WHERE id=?1", [&id], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    if existing_key.as_deref() == Some(MIKU_SYSTEM_KEY) && id != crate::builtin_agents::MIKU_ID {
        return Err(tr!("不能修改总管身份"));
    }
    let existing_runtime: Option<String> = db
        .query_row("SELECT runtime_id FROM agents WHERE id=?1", [&id], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| error.to_string())?;
    let custom_runtime = if input.runtime_id.starts_with("custom:") {
        db.query_row(
            "SELECT command, provider FROM custom_runtimes WHERE id=?1",
            [&input.runtime_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else {
        None
    };
    let runtime_ok = existing_runtime.as_deref() == Some(input.runtime_id.as_str())
        || runtime::detect().iter().any(|runtime| {
            runtime.id == input.runtime_id && (!require_online || runtime.status == "online")
        })
        || custom_runtime.is_some_and(|(command, provider)| {
            !require_online || runtime::resolve_runtime_command(&command, &provider).is_some()
        });
    if !runtime_ok {
        return Err(tr!("请选择本机已检测到的运行时"));
    }

    let is_miku = builtin_agents::is_miku_key(existing_key.as_deref())
        || id == crate::builtin_agents::MIKU_ID;
    let avatar = if is_miku {
        crate::builtin_agents::MIKU_AVATAR.to_string()
    } else {
        input.avatar.trim().to_string()
    };

    let transaction = db.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO agents
             (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks, system_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, avatar=excluded.avatar,
               description=excluded.description, instructions=excluded.instructions,
               runtime_id=excluded.runtime_id, model=excluded.model,
               max_concurrent_tasks=excluded.max_concurrent_tasks",
            params![
                id,
                name,
                avatar,
                input.description.trim(),
                input.instructions.trim(),
                input.runtime_id,
                input.model.trim(),
                input.max_concurrent_tasks
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM agent_resources WHERE agent_id = ?1", [&id])
        .map_err(|error| error.to_string())?;
    for resource_id in &input.skill_ids {
        transaction
            .execute(
                "INSERT INTO agent_resources (agent_id, resource_id, kind) VALUES (?1, ?2, 'skill')",
                params![id, resource_id],
            )
            .map_err(|error| error.to_string())?;
    }
    for resource_id in &input.mcp_ids {
        transaction
            .execute(
                "INSERT INTO agent_resources (agent_id, resource_id, kind) VALUES (?1, ?2, 'mcp')",
                params![id, resource_id],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    list(db)
}

pub fn delete(db: &Connection, agent_id: &str) -> Result<Vec<Agent>, String> {
    let key = system_key(db, agent_id)?;
    if builtin_agents::is_miku_key(key.as_deref()) {
        return Err(tr!("Miku 是产品总管，不能删除"));
    }
    db.execute("DELETE FROM agents WHERE id = ?1", [agent_id])
        .map_err(|error| tr!("该智能体仍被小队或历史任务使用，无法删除：{error}", error = error))?;
    list(db)
}

pub fn create(
    db: &mut Connection,
    name: &str,
    description: &str,
    instructions: &str,
    runtime_id: &str,
    avatar: &str,
    max_concurrent_tasks: u8,
) -> Result<Agent, String> {
    let id = Uuid::new_v4().to_string();
    save(
        db,
        AgentInput {
            id: Some(id.clone()),
            name: name.to_string(),
            avatar: if avatar.trim().is_empty() {
                "🤖".into()
            } else {
                avatar.to_string()
            },
            description: description.to_string(),
            instructions: instructions.to_string(),
            runtime_id: if runtime_id.trim().is_empty() {
                runtime::seed_runtime_id()
            } else {
                runtime_id.trim().to_string()
            },
            model: "auto".into(),
            max_concurrent_tasks: max_concurrent_tasks.clamp(1, 6),
            skill_ids: Vec::new(),
            mcp_ids: Vec::new(),
        },
        false,
    )?;
    get(db, &id)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::database;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        database::migrate(&db).unwrap();
        database::seed(&db).unwrap();
        db
    }

    #[test]
    fn cannot_delete_miku() {
        let db = db();
        assert!(delete(&db, "miku").is_err());
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM agents WHERE id='miku'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn can_delete_a_specialist() {
        let db = db();
        db.execute(
            "INSERT INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('spec', '前端', '🤖', '', '', 'cursor', 'auto', 1)",
            [],
        )
        .unwrap();
        delete(&db, "spec").unwrap();
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM agents WHERE id='spec'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn save_cannot_forge_miku_system_key() {
        let mut db = db();
        db.execute(
            "INSERT INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('spec', '前端', '🤖', '', '', 'cursor', 'auto', 1)",
            [],
        )
        .unwrap();
        save(
            &mut db,
            AgentInput {
                id: Some("spec".into()),
                name: "前端".into(),
                avatar: "🤖".into(),
                description: String::new(),
                instructions: String::new(),
                runtime_id: "cursor".into(),
                model: "auto".into(),
                max_concurrent_tasks: 1,
                skill_ids: Vec::new(),
                mcp_ids: Vec::new(),
            },
            false,
        )
        .unwrap();
        let key: Option<String> = db
            .query_row("SELECT system_key FROM agents WHERE id='spec'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(key.is_none());
        let miku_key: String = db
            .query_row("SELECT system_key FROM agents WHERE id='miku'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(miku_key, "miku");
    }

    #[test]
    fn save_accepts_an_online_custom_runtime() {
        let mut db = db();
        let executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .to_string();
        db.execute(
            "INSERT INTO custom_runtimes (id, name, command, provider)
             VALUES ('custom:test', 'Test', ?1, 'custom')",
            [&executable],
        )
        .unwrap();

        let agents = save(
            &mut db,
            AgentInput {
                id: Some("specialist".into()),
                name: "Specialist".into(),
                avatar: "🤖".into(),
                description: String::new(),
                instructions: String::new(),
                runtime_id: "custom:test".into(),
                model: "auto".into(),
                max_concurrent_tasks: 1,
                skill_ids: Vec::new(),
                mcp_ids: Vec::new(),
            },
            true,
        )
        .unwrap();

        assert_eq!(
            agents
                .iter()
                .find(|agent| agent.id == "specialist")
                .unwrap()
                .runtime_id,
            "custom:test"
        );
    }

    #[test]
    fn renamed_miku_is_still_recognized() {
        let mut db = db();
        save(
            &mut db,
            AgentInput {
                id: Some("miku".into()),
                name: "Jarvis".into(),
                avatar: crate::builtin_agents::MIKU_AVATAR.into(),
                description: "总管".into(),
                instructions: "主仓库是 acme。".into(),
                runtime_id: "cursor".into(),
                model: "auto".into(),
                max_concurrent_tasks: 3,
                skill_ids: Vec::new(),
                mcp_ids: Vec::new(),
            },
            false,
        )
        .unwrap();
        let agent = get(&db, "miku").unwrap();
        assert_eq!(agent.system_key.as_deref(), Some("miku"));
        assert_eq!(agent.name, "Jarvis");
        let system = agent.system_instructions.as_deref().unwrap();
        assert!(
            system.starts_with("你是 Jarvis，") || system.starts_with("You are Jarvis,"),
            "unexpected system instructions: {system}"
        );
        assert_eq!(agent.instructions, "主仓库是 acme。");
    }
}
