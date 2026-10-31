use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use super::{ChannelConfig, ChatType, InboundMessage, Installation};

#[derive(Clone, Debug)]
pub struct AllowedUser {
    pub installation_id: String,
    pub channel_user_id: String,
    pub display_name: Option<String>,
    pub role: String,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub id: String,
    pub installation_id: String,
    pub route_key: String,
    pub chat_id: String,
    pub chat_type: ChatType,
    pub thread_id: Option<String>,
    pub session_id: String,
}

#[derive(Clone, Debug)]
pub struct Delivery {
    pub run_id: String,
    pub session_id: String,
    pub binding_id: String,
    pub installation_id: String,
    pub chat_id: String,
    pub chat_type: ChatType,
    pub reply_to_message_id: Option<String>,
    pub thread_id: Option<String>,
    pub sender_id: Option<String>,
    pub typing_reaction_id: Option<String>,
}

fn row_to_installation(row: &rusqlite::Row) -> rusqlite::Result<Installation> {
    let raw: String = row.get(6)?;
    let config = serde_json::from_str::<ChannelConfig>(&raw).unwrap_or_default();
    Ok(Installation {
        id: row.get(0)?,
        channel_type: row.get(1)?,
        name: row.get(2)?,
        target_type: row.get(3)?,
        target_id: row.get(4)?,
        project_id: row.get(5)?,
        config,
        enabled: row.get::<_, i64>(7)? != 0,
        status: row.get(8)?,
        last_error: row.get(9)?,
    })
}

const INSTALLATION_COLUMNS: &str =
    "id, channel_type, name, target_type, target_id, project_id, config_json, enabled, status, last_error";

pub fn list_installations(db: &Connection) -> Result<Vec<Installation>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {INSTALLATION_COLUMNS} FROM channel_installations ORDER BY created_at"
        ))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], row_to_installation)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

pub fn get_installation(db: &Connection, id: &str) -> Result<Option<Installation>, String> {
    db.query_row(
        &format!("SELECT {INSTALLATION_COLUMNS} FROM channel_installations WHERE id=?1"),
        [id],
        row_to_installation,
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn save_installation(db: &Connection, installation: &Installation) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    let config = serde_json::to_string(&installation.config).map_err(|error| error.to_string())?;
    db.execute(
        "INSERT INTO channel_installations
         (id, channel_type, name, target_type, target_id, project_id, config_json, secret_ref,
          enabled, status, last_error, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?1, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(id) DO UPDATE SET
           channel_type=excluded.channel_type, name=excluded.name,
           target_type=excluded.target_type, target_id=excluded.target_id,
           project_id=excluded.project_id, config_json=excluded.config_json,
           enabled=excluded.enabled, updated_at=excluded.updated_at",
        params![
            installation.id,
            installation.channel_type,
            installation.name,
            installation.target_type,
            installation.target_id,
            installation.project_id,
            config,
            installation.enabled as i64,
            installation.status,
            installation.last_error,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_installation(db: &Connection, id: &str) -> Result<(), String> {
    db.execute("DELETE FROM channel_installations WHERE id=?1", [id])
        .map_err(|error| error.to_string())?;
    for table in [
        "channel_allowed_users",
        "channel_session_bindings",
        "channel_inbound_dedup",
        "channel_run_deliveries",
        "channel_outbound_messages",
    ] {
        let _ = db.execute(
            &format!("DELETE FROM {table} WHERE installation_id=?1"),
            [id],
        );
    }
    Ok(())
}

pub fn set_status(
    db: &Connection,
    id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    db.execute(
        "UPDATE channel_installations SET status=?1, last_error=?2, updated_at=?3 WHERE id=?4",
        params![status, error, Utc::now().to_rfc3339(), id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn set_config(db: &Connection, id: &str, config: &ChannelConfig) -> Result<(), String> {
    let value = serde_json::to_string(config).map_err(|error| error.to_string())?;
    db.execute(
        "UPDATE channel_installations SET config_json=?1, updated_at=?2 WHERE id=?3",
        params![value, Utc::now().to_rfc3339(), id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_allowed_users(
    db: &Connection,
    installation_id: &str,
) -> Result<Vec<AllowedUser>, String> {
    let mut statement = db
        .prepare(
            "SELECT installation_id, channel_user_id, display_name, role
             FROM channel_allowed_users WHERE installation_id=?1 ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([installation_id], |row| {
            Ok(AllowedUser {
                installation_id: row.get(0)?,
                channel_user_id: row.get(1)?,
                display_name: row.get(2)?,
                role: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

pub fn allowed_user(db: &Connection, installation_id: &str, user_id: &str) -> Option<AllowedUser> {
    db.query_row(
        "SELECT installation_id, channel_user_id, display_name, role
         FROM channel_allowed_users WHERE installation_id=?1 AND channel_user_id=?2",
        params![installation_id, user_id],
        |row| {
            Ok(AllowedUser {
                installation_id: row.get(0)?,
                channel_user_id: row.get(1)?,
                display_name: row.get(2)?,
                role: row.get(3)?,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

pub fn add_allowed_user(
    db: &Connection,
    installation_id: &str,
    user_id: &str,
    display_name: Option<&str>,
    role: &str,
) -> Result<(), String> {
    db.execute(
        "INSERT INTO channel_allowed_users
         (installation_id, channel_user_id, display_name, role, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(installation_id, channel_user_id) DO UPDATE SET
           display_name=COALESCE(excluded.display_name, channel_allowed_users.display_name),
           role=excluded.role",
        params![
            installation_id,
            user_id,
            display_name,
            role,
            Utc::now().to_rfc3339()
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn remove_allowed_user(
    db: &Connection,
    installation_id: &str,
    user_id: &str,
) -> Result<(), String> {
    db.execute(
        "DELETE FROM channel_allowed_users WHERE installation_id=?1 AND channel_user_id=?2",
        params![installation_id, user_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// 返回 true 表示这是第一次见到这条消息。
pub fn claim_inbound(db: &Connection, installation_id: &str, message_id: &str) -> bool {
    db.execute(
        "INSERT OR IGNORE INTO channel_inbound_dedup (installation_id, message_id, received_at)
         VALUES (?1, ?2, ?3)",
        params![installation_id, message_id, Utc::now().to_rfc3339()],
    )
    .map(|changed| changed > 0)
    .unwrap_or(false)
}

pub fn prune_inbound_dedup(db: &Connection) {
    let cutoff = (Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let _ = db.execute(
        "DELETE FROM channel_inbound_dedup WHERE received_at < ?1",
        [cutoff],
    );
}

pub fn active_binding(
    db: &Connection,
    installation_id: &str,
    route_key: &str,
) -> Result<Option<Binding>, String> {
    db.query_row(
        "SELECT id, installation_id, route_key, chat_id, chat_type, thread_id, session_id
         FROM channel_session_bindings
         WHERE installation_id=?1 AND route_key=?2 AND retired_at IS NULL",
        params![installation_id, route_key],
        |row| {
            Ok(Binding {
                id: row.get(0)?,
                installation_id: row.get(1)?,
                route_key: row.get(2)?,
                chat_id: row.get(3)?,
                chat_type: ChatType::parse(&row.get::<_, String>(4)?),
                thread_id: row.get(5)?,
                session_id: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn create_binding(
    db: &Connection,
    installation_id: &str,
    route_key: &str,
    msg: &InboundMessage,
    session_id: &str,
) -> Result<Binding, String> {
    let binding = Binding {
        id: uuid::Uuid::new_v4().to_string(),
        installation_id: installation_id.to_string(),
        route_key: route_key.to_string(),
        chat_id: msg.chat_id.clone(),
        chat_type: msg.chat_type,
        thread_id: msg.thread_id.clone(),
        session_id: session_id.to_string(),
    };
    db.execute(
        "INSERT INTO channel_session_bindings
         (id, installation_id, route_key, chat_id, chat_type, thread_id, session_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            binding.id,
            binding.installation_id,
            binding.route_key,
            binding.chat_id,
            binding.chat_type.as_str(),
            binding.thread_id,
            binding.session_id,
            Utc::now().to_rfc3339()
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(binding)
}

pub fn retire_binding(db: &Connection, binding_id: &str) -> Result<(), String> {
    db.execute(
        "UPDATE channel_session_bindings SET retired_at=?1 WHERE id=?2",
        params![Utc::now().to_rfc3339(), binding_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn insert_delivery(db: &Connection, delivery: &Delivery) -> Result<(), String> {
    db.execute(
        "INSERT OR REPLACE INTO channel_run_deliveries
         (run_id, session_id, binding_id, installation_id, chat_id, chat_type,
          reply_to_message_id, thread_id, sender_id, typing_reaction_id, state, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', ?11)",
        params![
            delivery.run_id,
            delivery.session_id,
            delivery.binding_id,
            delivery.installation_id,
            delivery.chat_id,
            delivery.chat_type.as_str(),
            delivery.reply_to_message_id,
            delivery.thread_id,
            delivery.sender_id,
            delivery.typing_reaction_id,
            Utc::now().to_rfc3339()
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn pending_deliveries(db: &Connection, session_id: &str) -> Result<Vec<Delivery>, String> {
    let mut statement = db
        .prepare(
            "SELECT run_id, session_id, binding_id, installation_id, chat_id, chat_type,
                    reply_to_message_id, thread_id, sender_id, typing_reaction_id
             FROM channel_run_deliveries
             WHERE session_id=?1 AND state='pending' ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok(Delivery {
                run_id: row.get(0)?,
                session_id: row.get(1)?,
                binding_id: row.get(2)?,
                installation_id: row.get(3)?,
                chat_id: row.get(4)?,
                chat_type: ChatType::parse(&row.get::<_, String>(5)?),
                reply_to_message_id: row.get(6)?,
                thread_id: row.get(7)?,
                sender_id: row.get(8)?,
                typing_reaction_id: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

pub fn mark_delivery(
    db: &Connection,
    run_id: &str,
    state: &str,
    error: Option<&str>,
) -> Result<(), String> {
    db.execute(
        "UPDATE channel_run_deliveries
         SET state=?1, attempts=attempts+1, last_error=?2, delivered_at=?3
         WHERE run_id=?4",
        params![
            state,
            error,
            (state == "sent").then(|| Utc::now().to_rfc3339()),
            run_id
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn record_outbound(
    db: &Connection,
    installation_id: &str,
    message_id: &str,
    run_id: Option<&str>,
    kind: &str,
) {
    let _ = db.execute(
        "INSERT OR REPLACE INTO channel_outbound_messages
         (installation_id, message_id, run_id, kind, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            installation_id,
            message_id,
            run_id,
            kind,
            Utc::now().to_rfc3339()
        ],
    );
}

pub fn is_outbound_message(db: &Connection, installation_id: &str, message_id: &str) -> bool {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM channel_outbound_messages
                       WHERE installation_id=?1 AND message_id=?2)",
        params![installation_id, message_id],
        |row| row.get(0),
    )
    .unwrap_or(false)
}

/// 卡片回调靠出站消息找回它属于哪次运行。
pub fn outbound_run(db: &Connection, installation_id: &str, message_id: &str) -> Option<String> {
    db.query_row(
        "SELECT run_id FROM channel_outbound_messages
         WHERE installation_id=?1 AND message_id=?2 AND run_id IS NOT NULL",
        params![installation_id, message_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

pub fn delivery_by_run(db: &Connection, run_id: &str) -> Option<Delivery> {
    db.query_row(
        "SELECT run_id, session_id, binding_id, installation_id, chat_id, chat_type,
                reply_to_message_id, thread_id, sender_id, typing_reaction_id
         FROM channel_run_deliveries WHERE run_id=?1",
        [run_id],
        |row| {
            Ok(Delivery {
                run_id: row.get(0)?,
                session_id: row.get(1)?,
                binding_id: row.get(2)?,
                installation_id: row.get(3)?,
                chat_id: row.get(4)?,
                chat_type: ChatType::parse(&row.get::<_, String>(5)?),
                reply_to_message_id: row.get(6)?,
                thread_id: row.get(7)?,
                sender_id: row.get(8)?,
                typing_reaction_id: row.get(9)?,
            })
        },
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        crate::database::migrate(&connection).unwrap();
        connection
    }

    #[test]
    fn inbound_dedup_only_claims_once() {
        let db = db();
        assert!(claim_inbound(&db, "inst", "om_1"));
        assert!(!claim_inbound(&db, "inst", "om_1"));
        assert!(claim_inbound(&db, "inst", "om_2"));
    }

    #[test]
    fn retire_binding_frees_the_route_key() {
        let db = db();
        db.execute_batch(
            "INSERT INTO chat_sessions
             (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('s1', 'chat', 'agent', 'a1', 'now', 'now'),
                    ('s2', 'chat', 'agent', 'a1', 'now', 'now');",
        )
        .unwrap();
        let message = InboundMessage {
            installation_id: "inst".into(),
            event_id: "e1".into(),
            message_id: "m1".into(),
            chat_id: "c1".into(),
            chat_type: ChatType::P2p,
            thread_id: None,
            root_id: None,
            parent_id: None,
            sender_id: "u1".into(),
            sender_name: None,
            text: "hi".into(),
            addressed_to_bot: true,
            media: Vec::new(),
            created_at: "now".into(),
        };
        let first = create_binding(&db, "inst", "c1", &message, "s1").unwrap();
        assert_eq!(
            active_binding(&db, "inst", "c1")
                .unwrap()
                .unwrap()
                .session_id,
            "s1"
        );
        retire_binding(&db, &first.id).unwrap();
        assert!(active_binding(&db, "inst", "c1").unwrap().is_none());
        create_binding(&db, "inst", "c1", &message, "s2").unwrap();
        assert_eq!(
            active_binding(&db, "inst", "c1")
                .unwrap()
                .unwrap()
                .session_id,
            "s2"
        );
    }
}
