//! Telegram：`getUpdates` 长轮询收消息，`sendMessage` 回复。

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::link::{self, TextTarget};
use super::{store, ChatType, InboundEvent, InboundMessage, Installation};

pub fn run(app: &AppHandle, installation_id: &str, stop: &AtomicBool) -> bool {
    link::run_socket(app, installation_id, stop, run_once)
}

fn run_once(
    app: &AppHandle,
    installation: &Installation,
    secret: &str,
    stop: &AtomicBool,
    connected: &mut bool,
) -> Result<(), String> {
    let me = get_me(secret)?;
    remember_bot(app, installation, &me);
    *connected = true;
    link::mark_connected(app, installation);
    let events = link::spawn_workers(app, installation);
    let mut offset = installation
        .config
        .update_cursor
        .parse::<i64>()
        .unwrap_or(0);
    while !stop.load(Ordering::SeqCst) {
        let updates = match get_updates(secret, offset) {
            Ok(updates) => updates,
            Err(error) if stop.load(Ordering::SeqCst) => return Err(error),
            Err(error) => return Err(error),
        };
        for update in updates {
            let update_id = update.get("update_id").and_then(Value::as_i64).unwrap_or(offset);
            if update_id >= offset {
                offset = update_id + 1;
            }
            if let Some(message) = decode_update(&update, &installation.id, &me) {
                let _ = events.try_send(InboundEvent::Message(message));
            }
        }
        save_cursor(app, installation, offset);
    }
    Ok(())
}

pub fn send(app: &AppHandle, installation: &Installation, target: &TextTarget<'_>) -> Result<String, String> {
    let (_, secret) = link::load(app, &installation.id)?;
    let mut last = String::new();
    for chunk in link::chunk_text(target.text, link::TEXT_LIMIT) {
        let mut body = json!({ "chat_id": target.chat_id, "text": chunk });
        if let Some(thread) = target.thread_id.filter(|value| !value.is_empty()) {
            if let Ok(id) = thread.parse::<i64>() {
                body["message_thread_id"] = json!(id);
            }
        }
        if let Some(reply) = target.reply_to.filter(|value| !value.is_empty()) {
            if let Ok(id) = reply.parse::<i64>() {
                body["reply_to_message_id"] = json!(id);
            }
        }
        let value = call(&secret, "sendMessage", &body, 20)?;
        last = value
            .pointer("/result/message_id")
            .and_then(Value::as_i64)
            .map(|id| id.to_string())
            .unwrap_or_else(|| "telegram".to_string());
    }
    Ok(last)
}

pub fn probe(secret: &str) -> Result<(String, String), String> {
    let me = get_me(secret)?;
    Ok((me.id.clone(), me.name))
}

struct Bot {
    id: String,
    username: String,
    name: String,
}

fn get_me(secret: &str) -> Result<Bot, String> {
    let value = call(secret, "getMe", &json!({}), 20)?;
    let id = value
        .pointer("/result/id")
        .and_then(Value::as_i64)
        .map(|id| id.to_string())
        .unwrap_or_default();
    if id.is_empty() {
        return Err(tr!("Telegram 没有返回机器人"));
    }
    let username = value
        .pointer("/result/username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let name = value
        .pointer("/result/first_name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(username.as_str())
        .to_string();
    Ok(Bot { id, username, name })
}

fn get_updates(secret: &str, offset: i64) -> Result<Vec<Value>, String> {
    let mut body = json!({ "timeout": 25, "allowed_updates": ["message"] });
    if offset > 0 {
        body["offset"] = json!(offset);
    }
    let value = call(secret, "getUpdates", &body, 40)?;
    Ok(value
        .get("result")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn call(secret: &str, method: &str, body: &Value, timeout: u32) -> Result<Value, String> {
    let url = format!("https://api.telegram.org/bot{secret}/{method}");
    let response = crate::http_client::request(
        "POST",
        &url,
        &[("Content-Type".to_string(), "application/json".to_string())],
        Some(&body.to_string()),
        timeout,
    )
    .map_err(|error| link::scrub(error, secret))?;
    if response.status == 409 {
        return Err(tr!("这个 Bot Token 正在被另一个程序轮询，请先停掉那边"));
    }
    let value: Value = serde_json::from_str(&response.body)
        .map_err(|_| tr!("Telegram 返回了无法解析的内容"))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let description = value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return Err(tr!("Telegram 拒绝了请求：{error}", error = description));
    }
    Ok(value)
}

fn remember_bot(app: &AppHandle, installation: &Installation, bot: &Bot) {
    let mut config = installation.config.clone();
    config.bot_open_id = Some(bot.id.clone());
    config.bot_username = bot.username.clone();
    config.bot_name = Some(bot.name.clone());
    let state = app.state::<crate::database::AppState>();
    if let Ok(db) = state.lock_db() {
        let _ = store::set_config(&db, &installation.id, &config);
    };
}

fn save_cursor(app: &AppHandle, installation: &Installation, offset: i64) {
    let state = app.state::<crate::database::AppState>();
    let Ok(db) = state.lock_db() else {
        return;
    };
    let Ok(Some(current)) = store::get_installation(&db, &installation.id) else {
        return;
    };
    let mut config = current.config;
    let next = offset.to_string();
    if config.update_cursor == next {
        return;
    }
    config.update_cursor = next;
    let _ = store::set_config(&db, &installation.id, &config);
}

fn decode_update(update: &Value, installation_id: &str, bot: &Bot) -> Option<InboundMessage> {
    let message = update.get("message")?;
    if message.pointer("/from/is_bot").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let sender = message.pointer("/from/id").and_then(Value::as_i64)?;
    let text = message
        .get("text")
        .or_else(|| message.get("caption"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return None;
    }
    let chat_kind = message.pointer("/chat/type").and_then(Value::as_str).unwrap_or("");
    let chat_type = if chat_kind == "private" {
        ChatType::P2p
    } else {
        ChatType::Group
    };
    let mention = format!("@{}", bot.username);
    let reply_to_bot = message
        .pointer("/reply_to_message/from/id")
        .and_then(Value::as_i64)
        .map(|id| id.to_string())
        == Some(bot.id.clone());
    let addressed = chat_type == ChatType::P2p
        || reply_to_bot
        || (!bot.username.is_empty() && text.to_ascii_lowercase().contains(&mention.to_ascii_lowercase()));
    let cleaned = if bot.username.is_empty() {
        text
    } else {
        text.replace(&mention, "").trim().to_string()
    };
    if cleaned.is_empty() {
        return None;
    }
    let chat_id = message.pointer("/chat/id").and_then(Value::as_i64)?.to_string();
    let message_id = message.get("message_id").and_then(Value::as_i64)?.to_string();
    let name = {
        let first = message.pointer("/from/first_name").and_then(Value::as_str).unwrap_or("");
        let last = message.pointer("/from/last_name").and_then(Value::as_str).unwrap_or("");
        format!("{first} {last}").trim().to_string()
    };
    Some(InboundMessage {
        installation_id: installation_id.to_string(),
        event_id: update.get("update_id").and_then(Value::as_i64).unwrap_or(0).to_string(),
        message_id,
        chat_id,
        chat_type,
        thread_id: message
            .get("message_thread_id")
            .and_then(Value::as_i64)
            .map(|id| id.to_string()),
        root_id: None,
        parent_id: None,
        sender_id: sender.to_string(),
        sender_name: (!name.is_empty()).then_some(name),
        text: cleaned,
        addressed_to_bot: addressed,
        media: Vec::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::{decode_update, Bot};
    use crate::channel::ChatType;

    fn bot() -> Bot {
        Bot { id: "9".to_string(), username: "multara_bot".to_string(), name: "Multara".to_string() }
    }

    #[test]
    fn group_mention_is_addressed() {
        let update = serde_json::json!({
            "update_id": 3,
            "message": {
                "message_id": 8,
                "text": "@multara_bot 你好",
                "from": { "id": 1, "is_bot": false, "first_name": "Ada" },
                "chat": { "id": -100, "type": "supergroup" }
            }
        });
        let message = decode_update(&update, "inst", &bot()).unwrap();
        assert_eq!(message.text, "你好");
        assert!(message.addressed_to_bot);
        assert_eq!(message.chat_type, ChatType::Group);
        assert_eq!(message.chat_id, "-100");
    }
}
