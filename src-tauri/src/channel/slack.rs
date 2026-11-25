//! Slack Socket Mode：App Token 收事件，Bot Token 发消息。

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::feishu::ws::WsError;
use super::link::{self, TextTarget};
use super::{ChatType, InboundEvent, InboundMessage, Installation};

#[derive(Deserialize)]
struct SlackSecret {
    bot: String,
    app: String,
}

fn parse_secret(raw: &str) -> Result<SlackSecret, String> {
    serde_json::from_str(raw).map_err(|_| tr!("Slack 凭据无效，请重新填写 Bot Token 和 App Token"))
}

pub fn encode_secret(bot: &str, app: &str) -> String {
    json!({ "bot": bot, "app": app }).to_string()
}

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
    let tokens = parse_secret(secret)?;
    let url = connections_open(&tokens.app)?;
    let identity = auth_test(&tokens.bot)?;
    remember_bot(app, installation, &identity);
    let mut conn = link::connect_ws(&url)?;
    *connected = true;
    link::mark_connected(app, installation);
    let events = link::spawn_workers(app, installation);
    let bot_user = identity.user_id;
    while !stop.load(Ordering::SeqCst) {
        match conn.read_binary() {
            Ok(data) => {
                let frame: Value = match serde_json::from_slice(&data) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };
                let kind = frame.get("type").and_then(Value::as_str).unwrap_or("");
                if kind == "disconnect" {
                    return Err(tr!("Slack 要求重新连接"));
                }
                if let Some(envelope) = frame.get("envelope_id").and_then(Value::as_str) {
                    let _ = conn.send_text(&json!({ "envelope_id": envelope }).to_string());
                }
                if kind != "events_api" {
                    continue;
                }
                let Some(message) = decode_event(
                    frame.get("payload").unwrap_or(&Value::Null),
                    &installation.id,
                    &bot_user,
                ) else {
                    continue;
                };
                if events.try_send(InboundEvent::Message(message)).is_err() {
                    crate::diag::write(&format!(
                        "CHANNEL slack {} 入站事件积压过多，已丢弃一条",
                        installation.id
                    ));
                }
            }
            Err(WsError::Timeout) => {}
            Err(WsError::Closed(reason)) => return Err(reason),
        }
    }
    conn.send_close();
    Ok(())
}

pub fn send(
    app: &AppHandle,
    installation: &Installation,
    target: &TextTarget<'_>,
) -> Result<String, String> {
    let (_, secret) = link::load(app, &installation.id)?;
    let tokens = parse_secret(&secret)?;
    let mut last = String::new();
    for chunk in link::chunk_text(target.text, link::TEXT_LIMIT) {
        let mut body = json!({ "channel": target.chat_id, "text": chunk });
        if let Some(thread) = target.thread_id.filter(|value| !value.is_empty()) {
            body["thread_ts"] = json!(thread);
        }
        let value = slack_call(&tokens.bot, "chat.postMessage", &body.to_string())?;
        last = value
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("slack")
            .to_string();
    }
    Ok(last)
}

pub fn probe(secret: &str) -> Result<(String, String), String> {
    let tokens = parse_secret(secret)?;
    let identity = auth_test(&tokens.bot)?;
    let _ = connections_open(&tokens.app)?;
    Ok((identity.user_id, identity.name))
}

struct Identity {
    user_id: String,
    name: String,
}

fn auth_test(bot: &str) -> Result<Identity, String> {
    let value = slack_call(bot, "auth.test", "{}")?;
    let user_id = value
        .get("user_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if user_id.is_empty() {
        return Err(tr!("Slack 没有返回机器人用户"));
    }
    let name = value
        .get("user")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(Identity { user_id, name })
}

fn connections_open(app_token: &str) -> Result<String, String> {
    let value = slack_call(app_token, "apps.connections.open", "{}")?;
    value
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with("wss://"))
        .map(str::to_string)
        .ok_or_else(|| tr!("Slack 没有返回长连接地址"))
}

fn slack_call(token: &str, method: &str, body: &str) -> Result<Value, String> {
    let url = format!("https://slack.com/api/{method}");
    let response = crate::http_client::request(
        "POST",
        &url,
        &[
            ("Authorization".to_string(), format!("Bearer {token}")),
            (
                "Content-Type".to_string(),
                "application/json; charset=utf-8".to_string(),
            ),
        ],
        Some(body),
        20,
    )
    .map_err(|error| link::scrub(error, token))?;
    let value: Value =
        serde_json::from_str(&response.body).map_err(|_| tr!("Slack 返回了无法解析的内容"))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return Err(tr!("Slack 拒绝了请求：{error}", error = error));
    }
    Ok(value)
}

fn remember_bot(app: &AppHandle, installation: &Installation, identity: &Identity) {
    let mut config = installation.config.clone();
    config.bot_open_id = Some(identity.user_id.clone());
    if !identity.name.is_empty() {
        config.bot_name = Some(identity.name.clone());
        config.bot_username = identity.name.clone();
    }
    let state = app.state::<crate::database::AppState>();
    if let Ok(db) = state.lock_db() {
        let _ = super::store::set_config(&db, &installation.id, &config);
    };
}

pub fn decode_event(
    payload: &Value,
    installation_id: &str,
    bot_user: &str,
) -> Option<InboundMessage> {
    let event = payload.get("event").unwrap_or(payload);
    let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
    if kind != "message" && kind != "app_mention" {
        return None;
    }
    if event.get("subtype").and_then(Value::as_str).is_some() || event.get("bot_id").is_some() {
        return None;
    }
    let user = event.get("user").and_then(Value::as_str).unwrap_or("");
    if user.is_empty() || user == bot_user {
        return None;
    }
    let text = event
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        return None;
    }
    let channel_type = event
        .get("channel_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    let chat_type = if channel_type == "im" {
        ChatType::P2p
    } else {
        ChatType::Group
    };
    let mention = format!("<@{bot_user}>");
    let addressed = chat_type == ChatType::P2p || kind == "app_mention" || text.contains(&mention);
    let cleaned = text.replace(&mention, "").trim().to_string();
    if cleaned.is_empty() {
        return None;
    }
    Some(InboundMessage {
        installation_id: installation_id.to_string(),
        event_id: event
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        message_id: event
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        chat_id: event
            .get("channel")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        chat_type,
        thread_id: event
            .get("thread_ts")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        root_id: None,
        parent_id: None,
        sender_id: user.to_string(),
        sender_name: None,
        text: cleaned,
        addressed_to_bot: addressed,
        media: Vec::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::decode_event;
    use crate::channel::ChatType;

    #[test]
    fn mention_in_channel_is_addressed_and_stripped() {
        let payload = serde_json::json!({
            "event": {
                "type": "message",
                "channel": "C1",
                "channel_type": "channel",
                "user": "U1",
                "text": "<@UBOT> 看一下",
                "ts": "1.2"
            }
        });
        let message = decode_event(&payload, "inst", "UBOT").unwrap();
        assert_eq!(message.text, "看一下");
        assert!(message.addressed_to_bot);
        assert_eq!(message.chat_type, ChatType::Group);
        assert_eq!(message.chat_id, "C1");
    }

    #[test]
    fn bot_message_is_dropped() {
        let payload = serde_json::json!({
            "event": { "type": "message", "bot_id": "B1", "text": "hi", "user": "U1", "ts": "1" }
        });
        assert!(decode_event(&payload, "inst", "UBOT").is_none());
    }
}
