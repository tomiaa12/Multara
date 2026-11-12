use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::{
    builtin_agents::{self, MIKU_SYSTEM_KEY},
    models::{Agent, AgentInput},
    resources,
    runtime,
};

pub fn list(db: &Connection) -> Result<Vec<Agent>, String> {
    grant_miku_defaults(db)?;
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

/// 把扫描到、且用户没有关掉的 Skill / MCP 补到 Miku 上。其他智能体不补。
pub fn grant_miku_defaults(db: &Connection) -> Result<(), String> {
    let resources: Vec<(String, String)> = resources::scan()
        .into_iter()
        .filter(|resource| resource.enabled && (resource.kind == "skill" || resource.kind == "mcp"))
        .map(|resource| (resource.id, resource.kind.to_string()))
        .collect();
    grant_miku_resources(db, &resources)
}

pub(crate) fn grant_miku_resources(
    db: &Connection,
    resources: &[(String, String)],
) -> Result<(), String> {
    let miku_id: Option<String> = db
        .query_row(
            "SELECT id FROM agents WHERE system_key = ?1 OR id = ?1 LIMIT 1",
            [MIKU_SYSTEM_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(miku_id) = miku_id else {
        return Ok(());
    };
    let mut statement = db
        .prepare("SELECT resource_id FROM miku_resource_off")
        .map_err(|error| error.to_string())?;
    let opted_out = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for (resource_id, kind) in resources {
        if kind != "skill" && kind != "mcp" {
            continue;
        }
        if opted_out.contains(resource_id) {
            continue;
        }
        db.execute(
            "INSERT OR IGNORE INTO agent_resources (agent_id, resource_id, kind) VALUES (?1, ?2, ?3)",
            params![miku_id, resource_id, kind],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 本次保存从 Miku 拿掉的资源记成关闭；重新勾上的从关闭名单里去掉。
fn remember_miku_opt_outs(
    db: &Connection,
    agent_id: &str,
    skill_ids: &[String],
    mcp_ids: &[String],
) -> Result<(), String> {
    let kept: HashSet<&str> = skill_ids
        .iter()
        .chain(mcp_ids.iter())
        .map(String::as_str)
        .collect();
    let mut statement = db
        .prepare("SELECT resource_id FROM agent_resources WHERE agent_id = ?1")
        .map_err(|error| error.to_string())?;
    let previous = statement
        .query_map([agent_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for resource_id in &previous {
        if kept.contains(resource_id.as_str()) {
            continue;
        }
        db.execute(
            "INSERT OR IGNORE INTO miku_resource_off (resource_id) VALUES (?1)",
            [resource_id],
        )
        .map_err(|error| error.to_string())?;
    }
    for resource_id in &kept {
        db.execute(
            "DELETE FROM miku_resource_off WHERE resource_id = ?1",
            [*resource_id],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
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
    if is_miku {
        remember_miku_opt_outs(&transaction, &id, &input.skill_ids, &input.mcp_ids)?;
    }
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

pub fn delete(db: &mut Connection, agent_id: &str) -> Result<Vec<Agent>, String> {
    let key = system_key(db, agent_id)?;
    if builtin_agents::is_miku_key(key.as_deref()) {
        return Err(tr!("Miku 是产品总管，不能删除"));
    }
    // 创建过程卡住时，智能体行已经落库，运行记录或队长引用还在。
    // 这两处外键没有级联，直接 DELETE agents 会被拒绝，界面上看起来像没反应。
    let transaction = db.transaction().map_err(|error| error.to_string())?;
    release_squads(&transaction, agent_id)?;
    transaction
        .execute(
            "UPDATE agent_runs SET parent_run_id = NULL
             WHERE parent_run_id IN (SELECT id FROM agent_runs WHERE agent_id = ?1)",
            [agent_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM run_events
             WHERE run_id IN (SELECT id FROM agent_runs WHERE agent_id = ?1)",
            [agent_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM agent_runs WHERE agent_id = ?1", [agent_id])
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM session_runtime_chats WHERE agent_id = ?1",
            [agent_id],
        )
        .map_err(|error| error.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    transaction
        .execute(
            "UPDATE tasks SET status = 'cancelled', updated_at = ?1
             WHERE agent_id = ?2
               AND status IN ('todo', 'in_progress')
               AND NOT EXISTS (
                 SELECT 1 FROM agent_runs
                 WHERE agent_runs.session_id = tasks.session_id
                   AND agent_runs.status IN ('queued', 'running')
               )",
            params![now, agent_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM agents WHERE id = ?1", [agent_id])
        .map_err(|error| tr!("该智能体仍被小队或历史任务使用，无法删除：{error}", error = error))?;
    transaction.commit().map_err(|error| error.to_string())?;
    list(db)
}

/// 队长外键没有级联。还有别的成员时把队长交给其中一人，否则整队删掉。
fn release_squads(db: &Connection, agent_id: &str) -> Result<(), String> {
    let mut statement = db
        .prepare("SELECT id FROM squads WHERE leader_agent_id = ?1")
        .map_err(|error| error.to_string())?;
    let squad_ids = statement
        .query_map([agent_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for squad_id in squad_ids {
        let replacement: Option<String> = db
            .query_row(
                "SELECT agent_id FROM squad_members
                 WHERE squad_id = ?1 AND agent_id != ?2
                 ORDER BY agent_id LIMIT 1",
                params![squad_id, agent_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(replacement) = replacement {
            db.execute(
                "UPDATE squads SET leader_agent_id = ?1 WHERE id = ?2",
                params![replacement, squad_id],
            )
            .map_err(|error| error.to_string())?;
        } else {
            db.execute("DELETE FROM squads WHERE id = ?1", [squad_id])
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn create(
    db: &mut Connection,
    name: &str,
    description: &str,
    instructions: &str,
    runtime_id: &str,
    avatar: &str,
    max_concurrent_tasks: u8,
    skill_ids: Vec<String>,
    mcp_ids: Vec<String>,
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
            skill_ids,
            mcp_ids,
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
        let mut db = db();
        assert!(delete(&mut db, "miku").is_err());
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM agents WHERE id='miku'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn can_delete_a_specialist() {
        let mut db = db();
        db.execute(
            "INSERT INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('spec', '前端', '🤖', '', '', 'cursor', 'auto', 1)",
            [],
        )
        .unwrap();
        delete(&mut db, "spec").unwrap();
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM agents WHERE id='spec'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn delete_clears_runs_and_cancels_open_tasks() {
        let mut db = db();
        db.execute(
            "INSERT INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('spec', '前端', '🤖', '', '', 'cursor', 'auto', 1),
                    ('other', '文档', '📄', '', '', 'cursor', 'auto', 1)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO chat_sessions (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('session', '建仓', 'agent', 'spec', 't', 't'),
                    ('other-session', '后续', 'agent', 'other', 't', 't')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO agent_runs (id, session_id, agent_id, runtime_id, assignment, status, created_at)
             VALUES ('run', 'session', 'spec', 'cursor', '创建智能体', 'queued', 't')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO agent_runs (id, session_id, agent_id, runtime_id, parent_run_id, assignment, status, created_at)
             VALUES ('child', 'other-session', 'other', 'cursor', 'run', '接着做', 'queued', 't')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO run_events (id, run_id, session_id, seq, event_type, created_at)
             VALUES ('event', 'run', 'session', 1, 'text', 't')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO tasks (id, session_id, title, description, agent_id, status, created_at, updated_at)
             VALUES ('task', 'session', '建仓', '', 'spec', 'in_progress', 't', 't')",
            [],
        )
        .unwrap();

        delete(&mut db, "spec").unwrap();

        let agents: u32 = db
            .query_row("SELECT COUNT(*) FROM agents WHERE id='spec'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(agents, 0);
        let runs: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE id='run'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(runs, 0);
        let events: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM run_events WHERE id='event'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(events, 0);
        let parent: Option<String> = db
            .query_row(
                "SELECT parent_run_id FROM agent_runs WHERE id='child'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(parent.is_none());
        let status: String = db
            .query_row("SELECT status FROM tasks WHERE id='task'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "cancelled");
    }

    #[test]
    fn delete_promotes_another_member_and_drops_a_solo_squad() {
        let mut db = db();
        db.execute(
            "INSERT INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('spec', '前端', '🤖', '', '', 'cursor', 'auto', 1),
                    ('other', '文档', '📄', '', '', 'cursor', 'auto', 1)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO squads (id, name, description, instructions, leader_agent_id)
             VALUES ('shared', '小队', '', '', 'spec'), ('solo', '单人', '', '', 'spec')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO squad_members (squad_id, agent_id)
             VALUES ('shared', 'spec'), ('shared', 'other'), ('solo', 'spec')",
            [],
        )
        .unwrap();

        delete(&mut db, "spec").unwrap();

        let leader: String = db
            .query_row(
                "SELECT leader_agent_id FROM squads WHERE id='shared'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(leader, "other");
        let solo: u32 = db
            .query_row("SELECT COUNT(*) FROM squads WHERE id='solo'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(solo, 0);
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

    #[test]
    fn miku_defaults_on_and_other_agents_stay_off() {
        let mut db = db();
        db.execute(
            "INSERT INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('spec', '前端', '🤖', '', '', 'cursor', 'auto', 1)",
            [],
        )
        .unwrap();
        let resources = [
            ("skill:demo:alpha".to_string(), "skill".to_string()),
            ("mcp:demo:beta".to_string(), "mcp".to_string()),
        ];
        grant_miku_resources(&db, &resources).unwrap();
        let miku = get(&db, "miku").unwrap();
        assert!(miku.skill_ids.iter().any(|id| id == "skill:demo:alpha"));
        assert!(miku.mcp_ids.iter().any(|id| id == "mcp:demo:beta"));
        let spec = get(&db, "spec").unwrap();
        assert!(spec.skill_ids.is_empty());
        assert!(spec.mcp_ids.is_empty());

        let skill_ids = miku
            .skill_ids
            .iter()
            .filter(|id| id.as_str() != "skill:demo:alpha")
            .cloned()
            .collect();
        save(
            &mut db,
            AgentInput {
                id: Some("miku".into()),
                name: miku.name.clone(),
                avatar: miku.avatar.clone(),
                description: miku.description.clone(),
                instructions: miku.instructions.clone(),
                runtime_id: miku.runtime_id.clone(),
                model: miku.model.clone(),
                max_concurrent_tasks: miku.max_concurrent_tasks,
                skill_ids,
                mcp_ids: miku.mcp_ids.clone(),
            },
            false,
        )
        .unwrap();
        grant_miku_resources(&db, &resources).unwrap();
        let miku = get(&db, "miku").unwrap();
        assert!(!miku.skill_ids.iter().any(|id| id == "skill:demo:alpha"));
        assert!(miku.mcp_ids.iter().any(|id| id == "mcp:demo:beta"));

        let mut skill_ids = miku.skill_ids.clone();
        skill_ids.push("skill:demo:alpha".into());
        save(
            &mut db,
            AgentInput {
                id: Some("miku".into()),
                name: miku.name.clone(),
                avatar: miku.avatar.clone(),
                description: miku.description.clone(),
                instructions: miku.instructions.clone(),
                runtime_id: miku.runtime_id.clone(),
                model: miku.model.clone(),
                max_concurrent_tasks: miku.max_concurrent_tasks,
                skill_ids,
                mcp_ids: miku.mcp_ids.clone(),
            },
            false,
        )
        .unwrap();
        let miku = get(&db, "miku").unwrap();
        assert!(miku.skill_ids.iter().any(|id| id == "skill:demo:alpha"));
    }
}
