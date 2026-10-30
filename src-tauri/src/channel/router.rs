//! 平台无关的入站管线：去重、白名单、群聊过滤、指令、会话绑定、防抖合并。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::database::AppState;
use crate::models::MessageAttachmentInput;

use super::feishu::api_for;
use super::pairing;
use super::store::{self, Binding};
use super::{CardAction, ChatType, InboundEvent, InboundMessage, Installation, MediaRef};

struct PendingBatch {
    installation: Installation,
    binding: Binding,
    message: InboundMessage,
    texts: Vec<String>,
    attachments: Vec<MessageAttachmentInput>,
}

static PENDING: OnceLock<Mutex<HashMap<String, PendingBatch>>> = OnceLock::new();

fn pending() -> &'static Mutex<HashMap<String, PendingBatch>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn handle_event(app: &AppHandle, installation: &Installation, event: InboundEvent) {
    match event {
        InboundEvent::Message(message) => handle_message(app, installation, message),
        InboundEvent::CardAction(action) => handle_card_action(app, installation, action),
    }
}

fn handle_message(app: &AppHandle, installation: &Installation, mut message: InboundMessage) {
    let deduped = {
        let state = app.state::<AppState>();
        let db = state.lock_db();
        match db {
            Ok(db) => store::claim_inbound(&db, &installation.id, &message.message_id),
            Err(_) => return,
        }
    };
    if !deduped {
        return;
    }
    message.installation_id = installation.id.clone();

    if message.chat_type == ChatType::Group {
        if !installation.config.group_enabled {
            return;
        }
        if !message.addressed_to_bot && !is_reply_to_bot(app, installation, &message) {
            return;
        }
    }

    let allowed = {
        let state = app.state::<AppState>();
        let db = state.lock_db();
        match db {
            Ok(db) => store::allowed_user(&db, &installation.id, &message.sender_id),
            Err(_) => None,
        }
    };
    if allowed.is_none() {
        if try_pair(app, installation, &message) {
            return;
        }
        if pairing::allow_notice(&installation.id, &message.sender_id) {
            send_notice(
                app,
                installation,
                &message,
                "你还没有绑定到 Multara。请在 Multara 设置的「消息渠道」里查看配对码，然后直接发送这串数字。",
            );
        }
        return;
    }

    if handle_command(app, installation, &message) {
        return;
    }

    let binding = match ensure_binding(app, installation, &message) {
        Ok(binding) => binding,
        Err(error) => {
            send_notice(app, installation, &message, &format!("无法创建会话：{error}"));
            return;
        }
    };

    let attachments = download_media(app, installation, &message);
    if message.text.is_empty() && attachments.is_empty() {
        return;
    }

    enqueue(app, installation, binding, message, attachments);
}

fn is_reply_to_bot(app: &AppHandle, installation: &Installation, message: &InboundMessage) -> bool {
    let Some(parent) = message.parent_id.as_deref().or(message.root_id.as_deref()) else {
        return false;
    };
    let state = app.state::<AppState>();
    state
        .lock_db()
        .map(|db| store::is_outbound_message(&db, &installation.id, parent))
        .unwrap_or(false)
}

fn try_pair(app: &AppHandle, installation: &Installation, message: &InboundMessage) -> bool {
    let code = installation.config.pairing_code.trim();
    if code.is_empty() {
        return false;
    }
    let sender = message.sender_id.as_str();
    if pairing::is_locked(&installation.id, sender) {
        if pairing::allow_notice(&installation.id, sender) {
            send_notice(app, installation, message, &pairing::locked_notice());
        }
        return true;
    }
    let text = message.text.trim();
    let candidate = text
        .strip_prefix("/pair ")
        .map(str::trim)
        .unwrap_or(text);
    if candidate != code {
        // 只把看起来像在输配对码的消息记为失败，正常聊天不会把人锁住
        let attempted = text.starts_with("/pair ") || candidate.chars().count() == code.chars().count();
        if attempted && pairing::record_failure(&installation.id, sender) {
            send_notice(app, installation, message, &pairing::locked_notice());
            return true;
        }
        return false;
    }
    pairing::clear(&installation.id, sender);
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else {
        return false;
    };
    if store::add_allowed_user(
        &db,
        &installation.id,
        &message.sender_id,
        message.sender_name.as_deref(),
        "owner",
    )
    .is_err()
    {
        return false;
    }
    send_notice(
        app,
        installation,
        message,
        "配对成功。现在可以直接给这个机器人发消息，也可以发送 /help 查看指令。",
    );
    true
}

fn handle_command(
    app: &AppHandle,
    installation: &Installation,
    message: &InboundMessage,
) -> bool {
    let text = message.text.trim();
    if !text.starts_with('/') {
        return false;
    }
    let mut parts = text.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default().to_lowercase();
    let argument = parts.next().unwrap_or_default().trim();

    match command.as_str() {
        "/help" => send_notice(
            app,
            installation,
            message,
            "可用指令：\n/new 开启新会话\n/stop 中止当前运行\n/status 查看当前会话状态\n/project <需求名> 切换默认需求\n/help 显示这段说明",
        ),
        "/new" => {
            let route_key = route_key(message);
            let state = app.state::<AppState>();
            if let Ok(db) = state.lock_db() {
                if let Ok(Some(binding)) =
                    store::active_binding(&db, &installation.id, &route_key)
                {
                    let _ = store::retire_binding(&db, &binding.id);
                }
            }
            send_notice(app, installation, message, "已开启新会话，下一条消息会创建新的对话。");
        }
        "/stop" => {
            let route_key = route_key(message);
            let session_id = {
                let state = app.state::<AppState>();
                let Ok(db) = state.lock_db() else {
                    return true;
                };
                store::active_binding(&db, &installation.id, &route_key)
                    .ok()
                    .flatten()
                    .map(|binding| binding.session_id)
            };
            let Some(session_id) = session_id else {
                send_notice(app, installation, message, "当前没有正在运行的会话。");
                return true;
            };
            let run_ids: Vec<String> = {
                let state = app.state::<AppState>();
                let Ok(db) = state.lock_db() else {
                    return true;
                };
                let mut statement = db
                    .prepare(
                        "SELECT id FROM agent_runs
                         WHERE session_id=?1 AND status IN ('queued', 'running')",
                    )
                    .ok();
                statement
                    .as_mut()
                    .and_then(|statement| {
                        statement
                            .query_map([&session_id], |row| row.get(0))
                            .ok()
                            .map(|rows| rows.filter_map(Result::ok).collect())
                    })
                    .unwrap_or_default()
            };
            if run_ids.is_empty() {
                send_notice(app, installation, message, "当前没有正在运行的智能体。");
                return true;
            }
            for run_id in &run_ids {
                let _ = crate::executor::stop(app, run_id);
            }
            send_notice(app, installation, message, "已请求停止当前运行。");
        }
        "/status" => {
            let route_key = route_key(message);
            let detail = {
                let state = app.state::<AppState>();
                let Ok(db) = state.lock_db() else {
                    return true;
                };
                store::active_binding(&db, &installation.id, &route_key)
                    .ok()
                    .flatten()
                    .map(|binding| {
                        let running: i64 = db
                            .query_row(
                                "SELECT COUNT(*) FROM agent_runs
                                 WHERE session_id=?1 AND status='running'",
                                [&binding.session_id],
                                |row| row.get(0),
                            )
                            .unwrap_or(0);
                        let queued: i64 = db
                            .query_row(
                                "SELECT COUNT(*) FROM agent_runs
                                 WHERE session_id=?1 AND status='queued'",
                                [&binding.session_id],
                                |row| row.get(0),
                            )
                            .unwrap_or(0);
                        (binding.session_id, running, queued)
                    })
            };
            match detail {
                Some((session_id, running, queued)) => send_notice(
                    app,
                    installation,
                    message,
                    &format!(
                        "目标：{}\n会话：{}\n运行中 {running} · 排队 {queued}",
                        target_label(installation),
                        &session_id[..session_id.len().min(8)]
                    ),
                ),
                None => send_notice(app, installation, message, "当前聊天还没有会话，直接发一条消息即可开始。"),
            }
        }
        "/project" => {
            if argument.is_empty() {
                send_notice(app, installation, message, "用法：/project 需求名");
                return true;
            }
            let project = {
                let state = app.state::<AppState>();
                let Ok(db) = state.lock_db() else {
                    return true;
                };
                db.query_row(
                    "SELECT id, name FROM projects WHERE name=?1 AND archived_at IS NULL LIMIT 1",
                    [argument],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .ok()
            };
            match project {
                Some((project_id, name)) => {
                    let mut updated = installation.clone();
                    updated.project_id = Some(project_id);
                    let state = app.state::<AppState>();
                    if let Ok(db) = state.lock_db() {
                        let _ = store::save_installation(&db, &updated);
                    }
                    send_notice(
                        app,
                        installation,
                        message,
                        &format!("默认需求已切换为「{name}」。新会话会挂到这个需求下。"),
                    );
                }
                None => send_notice(app, installation, message, "没有找到这个需求。"),
            }
        }
        _ => send_notice(
            app,
            installation,
            message,
            "未知指令。发送 /help 查看可用指令。",
        ),
    }
    true
}

fn target_label(installation: &Installation) -> String {
    let state = format!(
        "{} {}",
        if installation.target_type == "squad" {
            "小队"
        } else {
            "智能体"
        },
        installation.name
    );
    state
}

fn route_key(message: &InboundMessage) -> String {
    match message.thread_id.as_deref().filter(|value| !value.is_empty()) {
        Some(thread) => format!("{}:{}", message.chat_id, thread),
        None => message.chat_id.clone(),
    }
}

fn ensure_binding(
    app: &AppHandle,
    installation: &Installation,
    message: &InboundMessage,
) -> Result<Binding, String> {
    let key = route_key(message);
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    if let Some(binding) = store::active_binding(&db, &installation.id, &key)? {
        return Ok(binding);
    }
    let target_name: String = db
        .query_row(
            if installation.target_type == "squad" {
                "SELECT name FROM squads WHERE id=?1"
            } else {
                "SELECT name FROM agents WHERE id=?1"
            },
            [&installation.target_id],
            |row| row.get(0),
        )
        .map_err(|_| "绑定的智能体或小队不存在".to_string())?;
    let now = Utc::now().to_rfc3339();
    let session_id = uuid::Uuid::new_v4().to_string();
    let first_line: String = message
        .text
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(20)
        .collect();
    let chat_label = if message.chat_type == ChatType::P2p {
        "私聊".to_string()
    } else {
        "群聊".to_string()
    };
    let title = if first_line.trim().is_empty() {
        format!("{chat_label} · {target_name}")
    } else {
        format!("{chat_label} · {first_line}")
    };
    db.execute(
        "INSERT INTO chat_sessions
         (id, project_id, title, target_type, target_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        rusqlite::params![
            session_id,
            installation.project_id,
            title,
            installation.target_type,
            installation.target_id,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    let project_ids: Vec<String> = installation
        .project_id
        .clone()
        .into_iter()
        .collect();
    crate::session::replace_linked_projects(&db, &session_id, &project_ids)?;
    store::create_binding(&db, &installation.id, &key, message, &session_id)
}

fn download_media(
    app: &AppHandle,
    installation: &Installation,
    message: &InboundMessage,
) -> Vec<MessageAttachmentInput> {
    if message.media.is_empty() {
        return Vec::new();
    }
    let Ok((_, api)) = api_for(app, &installation.id) else {
        return Vec::new();
    };
    let root = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("channel-media")
        .join(&installation.id)
        .join(&message.message_id);
    message
        .media
        .iter()
        .take(10)
        .filter_map(|media: &MediaRef| {
            let name = sanitize_file_name(&media.name);
            let suffix: String = media.file_key.chars().rev().take(6).collect::<String>().chars().rev().collect();
            let dest = root.join(format!("{suffix}-{name}"));
            api.download_resource(&message.message_id, &media.file_key, &media.kind, &dest)
                .ok()?;
            let size = dest.metadata().ok()?.len();
            Some(MessageAttachmentInput {
                name,
                path: dest.to_string_lossy().to_string(),
                mime_type: guess_mime(&media.name),
                size,
            })
        })
        .collect()
}

fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_' | ' ' | '(' | ')') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "attachment".to_string()
    } else {
        cleaned.chars().take(120).collect()
    }
}

fn guess_mime(name: &str) -> String {
    let lower = name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or_default();
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" | "md" => "text/plain",
        "json" => "application/json",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn enqueue(
    app: &AppHandle,
    installation: &Installation,
    binding: Binding,
    message: InboundMessage,
    attachments: Vec<MessageAttachmentInput>,
) {
    let key = format!("{}:{}", installation.id, binding.id);
    let mut guard = match pending().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let entry = guard.entry(key.clone()).or_insert_with(|| {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            flush(&app, &key);
        });
        PendingBatch {
            installation: installation.clone(),
            binding: binding.clone(),
            message: message.clone(),
            texts: Vec::new(),
            attachments: Vec::new(),
        }
    });
    if !message.text.trim().is_empty() {
        entry.texts.push(message.text.trim().to_string());
    }
    entry.attachments.extend(attachments);
    if entry.attachments.len() > 10 {
        entry.attachments.truncate(10);
    }
}

fn flush(app: &AppHandle, key: &str) {
    let batch = pending().lock().ok().and_then(|mut map| map.remove(key));
    let Some(mut batch) = batch else {
        return;
    };
    if batch.texts.is_empty() && batch.attachments.is_empty() {
        return;
    }
    let content = batch.texts.join("\n\n");
    let meta = json!({
        "installationId": batch.installation.id,
        "chatId": batch.message.chat_id,
        "messageId": batch.message.message_id,
        "senderId": batch.message.sender_id,
        "senderName": batch.message.sender_name,
    });
    let (_, api) = match api_for(app, &batch.installation.id) {
        Ok(value) => value,
        Err(_) => return,
    };
    let typing = api.add_typing(&batch.message.message_id).ok().flatten();
    let result = crate::commands::send_channel_message(
        app,
        &batch.binding.session_id,
        &content,
        std::mem::take(&mut batch.attachments),
        &batch.installation.channel_type,
        meta,
    );
    match result {
        Ok(outcome) => {
            let state = app.state::<AppState>();
            if let Ok(db) = state.lock_db() {
                for run_id in &outcome.run_ids {
                    let _ = store::insert_delivery(
                        &db,
                        &store::Delivery {
                            run_id: run_id.clone(),
                            session_id: batch.binding.session_id.clone(),
                            binding_id: batch.binding.id.clone(),
                            installation_id: batch.installation.id.clone(),
                            chat_id: batch.message.chat_id.clone(),
                            chat_type: batch.message.chat_type,
                            reply_to_message_id: Some(batch.message.message_id.clone()),
                            thread_id: batch.message.thread_id.clone(),
                            sender_id: Some(batch.message.sender_id.clone()),
                            typing_reaction_id: typing.clone(),
                        },
                    );
                }
            }
            if outcome.steered {
                // 追加进正在跑的 run：把表情挂到已有的 delivery 上，结束时统一删除。
                let state = app.state::<AppState>();
                if let Ok(db) = state.lock_db() {
                    let attached = db
                        .execute(
                            "UPDATE channel_run_deliveries
                             SET typing_reaction_id=COALESCE(typing_reaction_id, ?1)
                             WHERE session_id=?2 AND state='pending'",
                            rusqlite::params![typing, batch.binding.session_id],
                        )
                        .unwrap_or(0);
                    if attached == 0 {
                        if let Some(reaction) = typing.as_deref() {
                            api.remove_typing(&batch.message.message_id, reaction);
                        }
                    }
                };
                return;
            }
        }
        Err(error) => {
            if let Some(reaction) = typing.as_deref() {
                api.remove_typing(&batch.message.message_id, reaction);
            }
            send_notice(
                app,
                &batch.installation,
                &batch.message,
                &format!("没有把消息交给智能体：{error}"),
            );
        }
    }
}

fn send_notice(
    app: &AppHandle,
    installation: &Installation,
    message: &InboundMessage,
    text: &str,
) {
    let Ok((_, api)) = api_for(app, &installation.id) else {
        return;
    };
    let (msg_type, content) = if looks_like_markdown(text) {
        ("interactive", crate::channel::feishu::card::markdown_card(text).to_string())
    } else {
        ("text", json!({ "text": text }).to_string())
    };
    let message_id = if message.chat_type == ChatType::P2p && message.thread_id.is_none() {
        api.send_message_raw(&message.chat_id, msg_type, &content)
    } else {
        api.reply(&message.message_id, msg_type, &content, message.thread_id.is_some())
    };
    if let Ok(sent) = message_id {
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            store::record_outbound(&db, &installation.id, &sent, None, "notice");
        };
    }
}

pub fn looks_like_markdown(text: &str) -> bool {
    let markers = ["```", "**", "## ", "- ", "* ", "[", "`", "|"];
    markers.iter().any(|marker| text.contains(marker))
}

fn handle_card_action(app: &AppHandle, installation: &Installation, action: CardAction) {
    let allowed = {
        let state = app.state::<AppState>();
        matches!(
            state
                .lock_db()
                .ok()
                .and_then(|db| store::allowed_user(&db, &installation.id, &action.operator_id)),
            Some(user) if user.role == "owner"
        )
    };
    if !allowed {
        return;
    }
    let kind = action
        .value
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if kind == "permission" {
        let run_id = action
            .value
            .get("runId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let request_id = action
            .value
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let option_id = action
            .value
            .get("optionId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if !request_id.is_empty() && !option_id.is_empty() {
            let run = (!run_id.is_empty()).then_some(run_id);
            let _ = crate::executor::resolve_run_permission(app, run, request_id, option_id);
        }
    } else if kind == "question" {
        answer_question(app, installation, &action);
    }
}

/// 点击提问卡片 = 发一条普通用户消息，走同一条执行路径。
fn answer_question(app: &AppHandle, installation: &Installation, action: &CardAction) {
    let Some(label) = action
        .value
        .get("label")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return;
    };
    let (run_id, delivery) = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return;
        };
        let Some(run_id) = store::outbound_run(&db, &installation.id, &action.message_id) else {
            return;
        };
        let Some(delivery) = store::delivery_by_run(&db, &run_id) else {
            return;
        };
        (run_id, delivery)
    };
    let meta = json!({
        "installationId": installation.id,
        "chatId": delivery.chat_id,
        "messageId": action.message_id,
        "senderId": action.operator_id,
        "senderName": null,
        "answerToRunId": run_id,
    });
    let Ok(outcome) = crate::commands::send_channel_message(
        app,
        &delivery.session_id,
        label,
        Vec::new(),
        &installation.channel_type,
        meta,
    ) else {
        return;
    };
    let state = app.state::<AppState>();
    if let Ok(db) = state.lock_db() {
        for new_run in &outcome.run_ids {
            let _ = store::insert_delivery(
                &db,
                &store::Delivery {
                    run_id: new_run.clone(),
                    session_id: delivery.session_id.clone(),
                    binding_id: delivery.binding_id.clone(),
                    installation_id: installation.id.clone(),
                    chat_id: delivery.chat_id.clone(),
                    chat_type: delivery.chat_type,
                    reply_to_message_id: Some(action.message_id.clone()),
                    thread_id: delivery.thread_id.clone(),
                    sender_id: Some(action.operator_id.clone()),
                    typing_reaction_id: None,
                },
            );
        }
    };
}
