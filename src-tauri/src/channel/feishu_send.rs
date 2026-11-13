//! 主动给已配对的飞书用户发私聊。
//!
//! 收件人只来自触发运行的结构化芯片。命令用句柄换成 open_id，不接受调用方自己写的 ID。

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::database::AppState;
use crate::models::{ChatMessage, FeishuSendView, InboxInput};

use super::feishu::api::FeishuApi;
use super::outbound::OUTBOUND_CHUNK_CHARS;
use super::store::{self, record_outbound};
use super::{Installation, TYPE_FEISHU, TYPE_LARK};

const HANDLE_PREFIX: &str = "feishu:";

#[derive(Clone, Debug)]
pub struct ResolvedRecipient {
    pub installation_id: String,
    pub open_id: String,
    pub label: String,
    pub installation_name: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuRecipientInput {
    pub installation_id: String,
    pub open_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuContact {
    pub installation_id: String,
    pub installation_name: String,
    pub open_id: String,
    pub display_name: Option<String>,
    pub session_title: Option<String>,
    pub paired_at: String,
    pub active_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuContactPage {
    pub enabled: bool,
    pub contacts: Vec<FeishuContact>,
}

struct Allowance {
    handle: String,
    installation_id: String,
    open_id: String,
    label: String,
    installation_name: String,
}

#[derive(Debug)]
struct Target {
    root_run_id: String,
    actor_run_id: String,
    handle: String,
    installation_id: String,
    open_id: String,
    label: String,
    body: String,
}

pub fn resolve(
    db: &Connection,
    inputs: &[FeishuRecipientInput],
) -> Result<Vec<ResolvedRecipient>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut resolved = Vec::new();
    for input in inputs {
        let installation_id = input.installation_id.trim();
        let open_id = input.open_id.trim();
        if installation_id.is_empty() || open_id.is_empty() {
            return Err(tr!("这个飞书联系人现在不能发送"));
        }
        let key = format!("{installation_id}\n{open_id}");
        if !seen.insert(key) {
            continue;
        }
        let row: Option<(String, String, Option<String>)> = db
            .query_row(
                "SELECT i.name, i.channel_type, u.display_name
                 FROM channel_allowed_users u
                 JOIN channel_installations i ON i.id = u.installation_id
                 WHERE u.installation_id=?1 AND u.channel_user_id=?2
                   AND i.enabled != 0 AND i.channel_type IN ('feishu', 'lark')",
                params![installation_id, open_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((name, _channel_type, display_name)) = row else {
            return Err(tr!("这个飞书联系人现在不能发送"));
        };
        let label = display_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| unnamed(open_id));
        resolved.push(ResolvedRecipient {
            installation_id: installation_id.to_string(),
            open_id: open_id.to_string(),
            label,
            installation_name: name,
        });
    }
    Ok(resolved)
}

pub fn save_message_recipients(
    db: &Connection,
    message_id: &str,
    recipients: &[ResolvedRecipient],
) -> Result<(), String> {
    for (position, recipient) in recipients.iter().enumerate() {
        db.execute(
            "INSERT INTO message_feishu_recipients
             (message_id, position, installation_id, open_id, label)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message_id,
                position as i64,
                recipient.installation_id,
                recipient.open_id,
                recipient.label
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 每个根运行各有一份名单。规划模式不授予。
pub fn grant(
    db: &Connection,
    run_ids: &[String],
    recipients: &[ResolvedRecipient],
) -> Result<(), String> {
    if recipients.is_empty() {
        return Ok(());
    }
    for run_id in run_ids {
        if run_mode(db, run_id)?.as_deref() == Some("plan") {
            continue;
        }
        insert_allowances(db, run_id, recipients, 0)?;
    }
    Ok(())
}

/// 追加进正在跑的同一次运行。规划模式不加名单。返回要写进 stdin 的允许名单。
pub fn merge_running(
    db: &Connection,
    session_id: &str,
    agent_id: &str,
    recipients: &[ResolvedRecipient],
) -> Result<String, String> {
    if recipients.is_empty() {
        return Ok(String::new());
    }
    let row: Option<(String, Option<String>)> = db
        .query_row(
            "SELECT id, mode FROM agent_runs
             WHERE session_id=?1 AND agent_id=?2 AND status IN ('queued', 'running')
             ORDER BY created_at DESC LIMIT 1",
            params![session_id, agent_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((run_id, mode)) = row else {
        return Ok(String::new());
    };
    if mode.as_deref() == Some("plan") {
        return Ok(String::new());
    }
    let root = root_run_id(db, &run_id)?;
    let existing = load_allowances(db, &root)?;
    let fresh: Vec<ResolvedRecipient> = recipients
        .iter()
        .filter(|recipient| {
            !existing.iter().any(|item| {
                item.installation_id == recipient.installation_id && item.open_id == recipient.open_id
            })
        })
        .cloned()
        .collect();
    let start = existing
        .iter()
        .map(|item| handle_index(&item.handle))
        .max()
        .unwrap_or(0);
    insert_allowances(db, &root, &fresh, start)?;
    prompt_for_run(db, &run_id)
}

pub fn prompt_for_run(db: &Connection, run_id: &str) -> Result<String, String> {
    if run_mode(db, run_id)?.as_deref() == Some("plan") {
        return Ok(String::new());
    }
    let root = root_run_id(db, run_id)?;
    let rows = load_allowances(db, &root)?;
    if rows.is_empty() {
        return Ok(String::new());
    }
    let mut lines = vec![tr!(
        "本条消息允许发给以下飞书联系人。按用户的话决定发给谁，没有点名的不要发："
    )];
    for row in &rows {
        lines.push(format!(
            "- {} {}（{}）",
            row.handle, row.label, row.installation_name
        ));
    }
    lines.push(tr!(
        "发送：multara feishu send --to <句柄> --text \"<正文>\""
    ));
    lines.push(tr!("发送由本次运行执行。"));
    Ok(lines.join("\n"))
}

pub fn send(db: &Connection, run_id: &str, handle: &str, text: &str) -> Result<String, String> {
    let target = authorize(db, run_id, handle, text)?;
    let installation = installation(db, &target.installation_id)?;
    let secret = super::secret::load_cli(&installation.id)
        .ok_or_else(|| tr!("渠道凭据缺失，请重新填写 App Secret"))?;
    deliver(db, &target, &FeishuApi::new(&installation, &secret))
}

pub fn attach_sends(db: &Connection, messages: &mut [ChatMessage]) -> Result<(), String> {
    if messages.is_empty() {
        return Ok(());
    }
    let session_id = messages[0].session_id.clone();
    let sends = sends_for_session(db, &session_id)?;
    if sends.is_empty() {
        return Ok(());
    }
    for message in messages {
        let Some(run_id) = message.run_id.clone() else {
            continue;
        };
        message.feishu_sends = sends
            .iter()
            .filter(|item| item.actor_run_id == run_id)
            .cloned()
            .collect();
    }
    Ok(())
}

/// 收件箱里的代发消息没有 run，列表时补上发送人和收件人，旧记录也能对上。
pub fn fill_outbound_speakers(db: &Connection, messages: &mut [ChatMessage]) -> Result<(), String> {
    if !messages.iter().any(|message| message.message_kind == "channel_outbound") {
        return Ok(());
    }
    let session_id = messages[0].session_id.clone();
    let mut statement = db
        .prepare(
            "SELECT f.body, f.label, f.created_at, r.agent_id
             FROM feishu_sends f
             LEFT JOIN agent_runs r ON r.id = f.actor_run_id
             WHERE f.inbox_session_id=?1 AND f.status='sent'",
        )
        .map_err(|error| error.to_string())?;
    let hints = statement
        .query_map([&session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for message in messages.iter_mut() {
        if message.message_kind != "channel_outbound" {
            continue;
        }
        let Some((_, label, _, agent_id)) = closest_send(&hints, &message.content, &message.created_at) else {
            continue;
        };
        if message.agent_id.as_deref().unwrap_or("").is_empty() {
            message.agent_id = agent_id.clone();
        }
        let mut meta = match message.source_meta.take() {
            Some(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        };
        let missing = meta
            .get("recipientLabel")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().is_empty())
            .unwrap_or(true);
        if missing {
            meta.insert("recipientLabel".to_string(), serde_json::Value::String(label.clone()));
        }
        message.source_meta = Some(serde_json::Value::Object(meta));
    }
    Ok(())
}

fn closest_send<'a>(
    hints: &'a [(String, String, String, Option<String>)],
    body: &str,
    at: &str,
) -> Option<&'a (String, String, String, Option<String>)> {
    let when = chrono::DateTime::parse_from_rfc3339(at).ok();
    hints
        .iter()
        .filter(|hint| hint.0 == body)
        .min_by_key(|hint| {
            let other = chrono::DateTime::parse_from_rfc3339(&hint.2).ok();
            match (when, other) {
                (Some(left), Some(right)) => (left.timestamp() - right.timestamp()).unsigned_abs(),
                _ => u64::MAX,
            }
        })
}

pub fn notify_sends(app: &AppHandle, run_id: &str) {
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else {
        return;
    };
    let sessions: Vec<String> = db
        .prepare(
            "SELECT DISTINCT inbox_session_id FROM feishu_sends
             WHERE actor_run_id=?1 AND inbox_session_id IS NOT NULL",
        )
        .ok()
        .and_then(|mut statement| {
            statement
                .query_map([run_id], |row| row.get(0))
                .ok()
                .map(|rows| rows.filter_map(Result::ok).collect())
        })
        .unwrap_or_default();
    drop(db);
    for session_id in sessions {
        let _ = app.emit("session-messages-changed", session_id);
    }
    let _ = app.emit("inbox-changed", ());
}

#[tauri::command]
pub fn list_feishu_contacts(state: State<'_, AppState>) -> Result<FeishuContactPage, String> {
    let db = state.lock_db()?;
    let enabled: bool = db
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM channel_installations
               WHERE enabled != 0 AND channel_type IN ('feishu', 'lark')
             )",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !enabled {
        return Ok(FeishuContactPage {
            enabled: false,
            contacts: Vec::new(),
        });
    }
    let mut statement = db
        .prepare(
            "SELECT u.installation_id, i.name, u.channel_user_id, u.display_name, u.created_at
             FROM channel_allowed_users u
             JOIN channel_installations i ON i.id = u.installation_id
             WHERE i.enabled != 0 AND i.channel_type IN ('feishu', 'lark')",
        )
        .map_err(|error| error.to_string())?;
    let mut contacts = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut listed = Vec::new();
    for (installation_id, installation_name, open_id, display_name, paired_at) in contacts.drain(..)
    {
        let session = p2p_session(&db, &installation_id, &open_id)?;
        let sent_at = latest_send_at(&db, &installation_id, &open_id)?;
        let active_at = [session.as_ref().map(|item| item.1.clone()), sent_at]
            .into_iter()
            .flatten()
            .max();
        listed.push(FeishuContact {
            installation_id,
            installation_name,
            open_id,
            display_name: display_name
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            session_title: session.map(|item| item.0),
            paired_at,
            active_at,
        });
    }
    listed.sort_by(|left, right| match (&left.active_at, &right.active_at) {
        (Some(left_at), Some(right_at)) => right_at.cmp(left_at),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => right.paired_at.cmp(&left.paired_at),
    });
    Ok(FeishuContactPage {
        enabled: true,
        contacts: listed,
    })
}

#[tauri::command]
pub fn resend_feishu_send(id: String, app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    let target = authorize_resend(&db, &id)?;
    let installation = installation(&db, &target.installation_id)?;
    let secret = super::secret::load(&app, &installation.id)
        .ok_or_else(|| tr!("渠道凭据缺失，请重新填写 App Secret"))?;
    let actor = target.actor_run_id.clone();
    deliver(&db, &target, &FeishuApi::new(&installation, &secret))?;
    drop(db);
    notify_sends(&app, &actor);
    if let Ok(db) = app.state::<AppState>().lock_db() {
        if let Ok(session_id) = run_session_id(&db, &actor) {
            let _ = app.emit("session-messages-changed", session_id);
        }
    }
    Ok(())
}

fn deliver(db: &Connection, target: &Target, api: &FeishuApi) -> Result<String, String> {
    let payload = serde_json::json!({ "text": target.body }).to_string();
    match api.send_to("open_id", &target.open_id, "text", &payload) {
        Ok(sent) => {
            let inbox_session_id = mirror_outbound(db, target, &sent.chat_id).ok().flatten();
            write_send(db, target, "sent", None, Some(&sent.message_id), Some(&sent.chat_id), inbox_session_id.as_deref())?;
            if !sent.message_id.is_empty() {
                let _ = record_outbound(
                    db,
                    &target.installation_id,
                    &sent.message_id,
                    Some(&target.actor_run_id),
                    "proactive",
                );
            }
            Ok(tr!("已发给 {label}", label = target.label))
        }
        Err(error) => {
            write_send(db, target, "failed", Some(&error), None, None, None)?;
            Err(error)
        }
    }
}

fn authorize(db: &Connection, run_id: &str, handle: &str, text: &str) -> Result<Target, String> {
    let body = text.trim();
    if body.is_empty() {
        return Err(tr!("正文不能为空"));
    }
    if body.chars().count() > OUTBOUND_CHUNK_CHARS {
        return Err(tr!("正文超过单条飞书消息长度"));
    }
    if run_id.trim().is_empty() {
        return Err(tr!("没有可发送的飞书收件人"));
    }
    if run_mode(db, run_id)?.as_deref() == Some("plan") {
        return Err(tr!("规划模式下不能发飞书消息"));
    }
    let root = root_run_id(db, run_id)?;
    let allowance = load_allowances(db, &root)?
        .into_iter()
        .find(|item| item.handle == handle.trim());
    let Some(allowance) = allowance else {
        return Err(tr!("没有可发送的飞书收件人"));
    };
    ensure_still_allowed(db, &allowance.installation_id, &allowance.open_id)?;
    if already_sent(db, &root, &allowance.handle)? {
        return Err(tr!("这个收件人已经发过"));
    }
    Ok(Target {
        root_run_id: root,
        actor_run_id: run_id.to_string(),
        handle: allowance.handle,
        installation_id: allowance.installation_id,
        open_id: allowance.open_id,
        label: allowance.label,
        body: body.to_string(),
    })
}

fn authorize_resend(db: &Connection, id: &str) -> Result<Target, String> {
    let row: Option<(String, String, String, String, String, String, String)> = db
        .query_row(
            "SELECT root_run_id, actor_run_id, handle, installation_id, open_id, label, body
             FROM feishu_sends WHERE id=?1 AND status='failed'",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((root, actor, handle, installation_id, open_id, label, body)) = row else {
        return Err(tr!("没有可重发的飞书消息"));
    };
    ensure_still_allowed(db, &installation_id, &open_id)?;
    if already_sent(db, &root, &handle)? {
        return Err(tr!("这个收件人已经发过"));
    }
    Ok(Target {
        root_run_id: root,
        actor_run_id: actor,
        handle,
        installation_id,
        open_id,
        label,
        body,
    })
}

fn write_send(
    db: &Connection,
    target: &Target,
    status: &str,
    error: Option<&str>,
    message_id: Option<&str>,
    chat_id: Option<&str>,
    inbox_session_id: Option<&str>,
) -> Result<(), String> {
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM feishu_sends
             WHERE root_run_id=?1 AND handle=?2 AND status='failed'
             ORDER BY seq DESC LIMIT 1",
            params![target.root_run_id, target.handle],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if status == "sent" {
        if let Some(id) = existing {
            db.execute(
                "UPDATE feishu_sends
                 SET status='sent', error=NULL, feishu_message_id=?2, chat_id=?3,
                     inbox_session_id=COALESCE(?4, inbox_session_id), body=?5
                 WHERE id=?1",
                params![id, message_id, chat_id, inbox_session_id, target.body],
            )
            .map_err(|error| error.to_string())?;
            return Ok(());
        }
    }
    let seq: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM feishu_sends WHERE root_run_id=?1",
            [&target.root_run_id],
            |row| row.get(0),
        )
        .unwrap_or(1);
    db.execute(
        "INSERT INTO feishu_sends
         (id, root_run_id, actor_run_id, seq, handle, installation_id, open_id, label, body,
          status, error, feishu_message_id, chat_id, inbox_session_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            Uuid::new_v4().to_string(),
            target.root_run_id,
            target.actor_run_id,
            seq,
            target.handle,
            target.installation_id,
            target.open_id,
            target.label,
            target.body,
            status,
            error,
            message_id,
            chat_id,
            inbox_session_id,
            Utc::now().to_rfc3339()
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn mirror_outbound(db: &Connection, target: &Target, chat_id: &str) -> Result<Option<String>, String> {
    let chat_id = chat_id.trim();
    if chat_id.is_empty() {
        return Ok(None);
    }
    let installation = installation(db, &target.installation_id)?;
    if let Some(binding) = store::active_binding(db, &installation.id, chat_id)? {
        if binding.chat_type != super::ChatType::P2p {
            return Ok(None);
        }
        insert_outbound_message(db, &binding.session_id, target)?;
        let _ = db.execute(
            "UPDATE chat_sessions SET updated_at=?1 WHERE id=?2",
            params![Utc::now().to_rfc3339(), binding.session_id],
        );
        ensure_inbox_item(db, &binding.session_id, None, &target.body)?;
        return Ok(Some(binding.session_id));
    }
    let session_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let title = tr!("私聊 · {name}", name = target.label);
    db.execute(
        "INSERT INTO chat_sessions
         (id, project_id, title, target_type, target_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            session_id,
            installation.project_id,
            title,
            installation.target_type,
            installation.target_id,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    if let Some(project_id) = installation.project_id.as_deref() {
        crate::session::replace_linked_projects(db, &session_id, &[project_id.to_string()])?;
    }
    let message = super::InboundMessage {
        installation_id: installation.id.clone(),
        event_id: String::new(),
        message_id: String::new(),
        chat_id: chat_id.to_string(),
        chat_type: super::ChatType::P2p,
        thread_id: None,
        root_id: None,
        parent_id: None,
        sender_id: target.open_id.clone(),
        sender_name: None,
        text: String::new(),
        addressed_to_bot: true,
        media: Vec::new(),
        created_at: now,
    };
    store::create_binding(db, &installation.id, chat_id, &message, &session_id)?;
    insert_outbound_message(db, &session_id, target)?;
    ensure_inbox_item(db, &session_id, Some(&title), &target.body)?;
    Ok(Some(session_id))
}

fn insert_outbound_message(db: &Connection, session_id: &str, target: &Target) -> Result<(), String> {
    let agent_id: Option<String> = db
        .query_row(
            "SELECT agent_id FROM agent_runs WHERE id=?1",
            [&target.actor_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let meta = serde_json::json!({ "recipientLabel": target.label }).to_string();
    db.execute(
        "INSERT INTO messages
         (id, session_id, role, content, message_kind, agent_id, source_meta_json, created_at)
         VALUES (?1, ?2, 'assistant', ?3, 'channel_outbound', ?4, ?5, ?6)",
        params![
            Uuid::new_v4().to_string(),
            session_id,
            target.body,
            agent_id,
            meta,
            Utc::now().to_rfc3339()
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn ensure_inbox_item(
    db: &Connection,
    session_id: &str,
    title: Option<&str>,
    preview: &str,
) -> Result<(), String> {
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM inbox_items WHERE session_id=?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if existing.is_some() {
        return Ok(());
    }
    let title = match title {
        Some(title) => title.to_string(),
        None => db
            .query_row(
                "SELECT title FROM chat_sessions WHERE id=?1",
                [session_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| tr!("私聊")),
    };
    let project_id: Option<String> = db
        .query_row(
            "SELECT project_id FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    crate::ops::inbox::upsert(
        db,
        InboxInput {
            session_id: session_id.to_string(),
            project_id,
            task_id: None,
            title,
            preview: preview.to_string(),
            status: "completed".to_string(),
        },
    )?;
    Ok(())
}

fn sends_for_session(db: &Connection, session_id: &str) -> Result<Vec<FeishuSendView>, String> {
    let mut statement = db
        .prepare(
            "SELECT s.id, s.actor_run_id, s.root_run_id, s.handle, s.label, i.name, s.body,
                    s.status, s.error, s.inbox_session_id, s.installation_id, s.open_id
             FROM feishu_sends s
             JOIN agent_runs r ON r.id = s.actor_run_id
             JOIN channel_installations i ON i.id = s.installation_id
             WHERE r.session_id=?1
             ORDER BY s.created_at, s.seq",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut views = Vec::new();
    for row in rows {
        let (
            id,
            actor_run_id,
            root_run_id,
            handle,
            label,
            installation_name,
            text,
            status,
            error,
            inbox_session_id,
            installation_id,
            open_id,
        ) = row.map_err(|error| error.to_string())?;
        let revoked = ensure_still_allowed(db, &installation_id, &open_id).is_err();
        let sent = already_sent(db, &root_run_id, &handle)?;
        views.push(FeishuSendView {
            id,
            actor_run_id,
            label,
            installation_name,
            text,
            status,
            error,
            inbox_session_id,
            revoked,
            resendable: !revoked && !sent,
        });
    }
    Ok(views)
}

fn insert_allowances(
    db: &Connection,
    root_run_id: &str,
    recipients: &[ResolvedRecipient],
    start: u32,
) -> Result<(), String> {
    for (offset, recipient) in recipients.iter().enumerate() {
        let handle = format!("{HANDLE_PREFIX}{}", start + offset as u32 + 1);
        db.execute(
            "INSERT OR IGNORE INTO feishu_send_allowances
             (root_run_id, handle, installation_id, open_id, label)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                root_run_id,
                handle,
                recipient.installation_id,
                recipient.open_id,
                recipient.label
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn load_allowances(db: &Connection, root_run_id: &str) -> Result<Vec<Allowance>, String> {
    let mut statement = db
        .prepare(
            "SELECT a.handle, a.installation_id, a.open_id, a.label, i.name
             FROM feishu_send_allowances a
             JOIN channel_installations i ON i.id = a.installation_id
             WHERE a.root_run_id=?1",
        )
        .map_err(|error| error.to_string())?;
    let mut rows = statement
        .query_map([root_run_id], |row| {
            Ok(Allowance {
                handle: row.get(0)?,
                installation_id: row.get(1)?,
                open_id: row.get(2)?,
                label: row.get(3)?,
                installation_name: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    rows.sort_by_key(|item| handle_index(&item.handle));
    Ok(rows)
}

fn p2p_session(
    db: &Connection,
    installation_id: &str,
    open_id: &str,
) -> Result<Option<(String, String)>, String> {
    db.query_row(
        "SELECT s.title, s.updated_at
         FROM channel_session_bindings b
         JOIN chat_sessions s ON s.id = b.session_id
         WHERE b.installation_id=?1 AND b.chat_type='p2p' AND b.retired_at IS NULL
           AND (
             EXISTS (
               SELECT 1 FROM messages m
               WHERE m.session_id = b.session_id
                 AND json_extract(m.source_meta_json, '$.senderId') = ?2
             )
             OR EXISTS (
               SELECT 1 FROM feishu_sends f
               WHERE f.installation_id = b.installation_id
                 AND f.open_id = ?2 AND f.chat_id = b.chat_id AND f.status = 'sent'
             )
           )
         ORDER BY s.updated_at DESC LIMIT 1",
        params![installation_id, open_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn latest_send_at(db: &Connection, installation_id: &str, open_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT MAX(created_at) FROM feishu_sends
         WHERE installation_id=?1 AND open_id=?2 AND status='sent'",
        params![installation_id, open_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn installation(db: &Connection, id: &str) -> Result<Installation, String> {
    store::get_installation(db, id)?.ok_or_else(|| tr!("渠道连接不存在"))
}

fn ensure_still_allowed(db: &Connection, installation_id: &str, open_id: &str) -> Result<(), String> {
    let allowed: bool = db
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM channel_allowed_users u
               JOIN channel_installations i ON i.id = u.installation_id
               WHERE u.installation_id=?1 AND u.channel_user_id=?2
                 AND i.enabled != 0 AND i.channel_type IN (?3, ?4)
             )",
            params![installation_id, open_id, TYPE_FEISHU, TYPE_LARK],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if allowed {
        Ok(())
    } else {
        Err(tr!("这个飞书联系人现在不能发送"))
    }
}

fn already_sent(db: &Connection, root_run_id: &str, handle: &str) -> Result<bool, String> {
    db.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM feishu_sends
           WHERE root_run_id=?1 AND handle=?2 AND status='sent'
         )",
        params![root_run_id, handle],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn root_run_id(db: &Connection, run_id: &str) -> Result<String, String> {
    let mut current = run_id.to_string();
    for _ in 0..8 {
        let parent: Option<String> = db
            .query_row(
                "SELECT parent_run_id FROM agent_runs WHERE id=?1",
                [&current],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .flatten();
        match parent.filter(|value| !value.is_empty()) {
            Some(parent) => current = parent,
            None => return Ok(current),
        }
    }
    Ok(current)
}

fn run_mode(db: &Connection, run_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT mode FROM agent_runs WHERE id=?1",
        [run_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn run_session_id(db: &Connection, run_id: &str) -> Result<String, String> {
    db.query_row(
        "SELECT session_id FROM agent_runs WHERE id=?1",
        [run_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

fn handle_index(handle: &str) -> u32 {
    handle
        .rsplit(':')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn unnamed(open_id: &str) -> String {
    let tail: String = open_id
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    tr!("飞书用户 · {tail}", tail = tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        crate::database::migrate(&connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO agents
                 (id, name, avatar, description, instructions, runtime_id)
                 VALUES ('a1', 'Miku', 'M', '', '', 'claude');
                 INSERT INTO chat_sessions
                 (id, title, target_type, target_id, created_at, updated_at)
                 VALUES ('s1', 'chat', 'agent', 'a1', 't0', 't0');
                 INSERT INTO agent_runs
                 (id, session_id, agent_id, runtime_id, assignment, status, created_at, mode)
                 VALUES ('r1', 's1', 'a1', 'claude', 'hi', 'running', 't0', 'agent');
                 INSERT INTO channel_installations
                 (id, channel_type, name, target_type, target_id, config_json, enabled, status, created_at, updated_at)
                 VALUES ('inst', 'feishu', '机器人', 'agent', 'a1', '{}', 1, 'connected', 't0', 't0');
                 INSERT INTO channel_allowed_users
                 (installation_id, channel_user_id, display_name, role, created_at)
                 VALUES ('inst', 'ou_abcdef', '张三', 'owner', 't0');",
            )
            .unwrap();
        connection
    }

    fn person(open_id: &str) -> ResolvedRecipient {
        ResolvedRecipient {
            installation_id: "inst".into(),
            open_id: open_id.into(),
            label: open_id.into(),
            installation_name: "机器人".into(),
        }
    }

    #[test]
    fn grant_numbers_handles_and_child_uses_the_root() {
        let db = db();
        grant(&db, &["r1".into()], &[person("ou_abcdef")]).unwrap();
        db.execute(
            "INSERT INTO agent_runs
             (id, session_id, agent_id, runtime_id, parent_run_id, assignment, status, created_at, mode)
             VALUES ('r2', 's1', 'a1', 'claude', 'r1', 'do', 'running', 't1', 'agent')",
            [],
        )
        .unwrap();
        let error = authorize(&db, "r2", "feishu:1", "  ").unwrap_err();
        assert!(error.contains("正文不能为空"));
        let target = authorize(&db, "r2", "feishu:1", "明天见").unwrap();
        assert_eq!(target.root_run_id, "r1");
        assert_eq!(target.label, "ou_abcdef");
    }

    #[test]
    fn success_uses_the_slot_and_failure_does_not() {
        let db = db();
        grant(&db, &["r1".into()], &[person("ou_abcdef")]).unwrap();
        let target = authorize(&db, "r1", "feishu:1", "hello").unwrap();
        write_send(&db, &target, "failed", Some("nope"), None, None, None).unwrap();
        assert!(authorize(&db, "r1", "feishu:1", "hello again").is_ok());
        write_send(
            &db,
            &target,
            "sent",
            None,
            Some("om_1"),
            Some("oc_1"),
            None,
        )
        .unwrap();
        assert!(authorize(&db, "r1", "feishu:1", "third").is_err());
    }

    #[test]
    fn plan_run_and_unknown_handle_are_rejected() {
        let db = db();
        db.execute("UPDATE agent_runs SET mode='plan' WHERE id='r1'", [])
            .unwrap();
        grant(&db, &["r1".into()], &[person("ou_abcdef")]).unwrap();
        assert!(authorize(&db, "r1", "feishu:1", "hi").is_err());
        db.execute("UPDATE agent_runs SET mode='agent' WHERE id='r1'", [])
            .unwrap();
        grant(&db, &["r1".into()], &[person("ou_abcdef")]).unwrap();
        assert!(authorize(&db, "r1", "feishu:9", "hi").is_err());
        assert!(authorize(&db, "r1", "feishu:1", &"字".repeat(OUTBOUND_CHUNK_CHARS + 1)).is_err());
    }

    #[test]
    fn unnamed_label_uses_the_last_six_characters() {
        assert!(unnamed("ou_31e9df5247e460d967e01db7231754ad").ends_with("1754ad"));
    }

    #[test]
    fn outbound_message_picks_up_the_sender_and_recipient() {
        let db = db();
        grant(&db, &["r1".into()], &[person("ou_abcdef")]).unwrap();
        let target = authorize(&db, "r1", "feishu:1", "hello").unwrap();
        write_send(
            &db,
            &target,
            "sent",
            None,
            Some("om_1"),
            Some("oc_1"),
            Some("inbox"),
        )
        .unwrap();
        let mut messages = vec![ChatMessage {
            id: "m1".into(),
            session_id: "inbox".into(),
            turn_id: None,
            role: "assistant",
            content: "hello".into(),
            run_id: None,
            agent_id: None,
            audience_type: Some("user"),
            audience_id: None,
            target_type: None,
            target_id: None,
            failure_reason: None,
            elapsed_ms: None,
            message_kind: "channel_outbound",
            quick_actions: Vec::new(),
            timeline: Vec::new(),
            truncated_before: None,
            attachments: Vec::new(),
            created_at: Utc::now().to_rfc3339(),
            feedback: None,
            source_type: None,
            source_meta: None,
            requested_mode: None,
            run_mode: None,
            feishu_sends: Vec::new(),
        }];
        fill_outbound_speakers(&db, &mut messages).unwrap();
        assert_eq!(messages[0].agent_id.as_deref(), Some("a1"));
        assert_eq!(
            messages[0].source_meta.as_ref().and_then(|value| value.get("recipientLabel")).and_then(|value| value.as_str()),
            Some("ou_abcdef"),
        );
    }
}
