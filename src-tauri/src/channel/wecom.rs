//! 企业微信智能机器人长连接。收发都在同一条 WebSocket 上。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde_json::{json, Value};
use tauri::AppHandle;

use super::feishu::ws::{WsConn, WsError};
use super::link::{self, TextTarget};
use super::{ChatType, InboundEvent, InboundMessage, Installation};

const WS_URL: &str = "wss://openws.work.weixin.qq.com";

fn sessions() -> &'static Mutex<HashMap<String, SyncSender<String>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, SyncSender<String>>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
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
    let mut conn = link::connect_ws(WS_URL)?;
    let req_id = subscribe(&mut conn, &installation.config.app_id, secret)?;
    let _ = req_id;
    *connected = true;
    link::mark_connected(app, installation);
    let (sender, receiver) = mpsc::sync_channel(32);
    register(&installation.id, sender);
    let unregister = Unregister(installation.id.clone());
    let events = link::spawn_workers(app, installation);
    let mut last_ping = Instant::now();
    let result = read_loop(
        installation,
        &mut conn,
        &receiver,
        &events,
        stop,
        &mut last_ping,
    );
    drop(unregister);
    conn.send_close();
    result
}

struct Unregister(String);

impl Drop for Unregister {
    fn drop(&mut self) {
        if let Ok(mut sessions) = sessions().lock() {
            sessions.remove(&self.0);
        }
    }
}

fn register(id: &str, sender: SyncSender<String>) {
    if let Ok(mut sessions) = sessions().lock() {
        sessions.insert(id.to_string(), sender);
    }
}

fn read_loop(
    installation: &Installation,
    conn: &mut WsConn,
    outbound: &Receiver<String>,
    events: &SyncSender<InboundEvent>,
    stop: &AtomicBool,
    last_ping: &mut Instant,
) -> Result<(), String> {
    while !stop.load(Ordering::SeqCst) {
        flush(conn, outbound)?;
        if last_ping.elapsed().as_secs() >= 30 {
            let _ = conn.send_text(
                &json!({ "cmd": "ping", "headers": { "req_id": uuid::Uuid::new_v4().to_string() } })
                    .to_string(),
            );
            *last_ping = Instant::now();
        }
        match conn.read_binary() {
            Ok(data) => {
                let frame: Value = match serde_json::from_slice(&data) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };
                let cmd = frame.get("cmd").and_then(Value::as_str).unwrap_or("");
                if cmd == "ping" {
                    let req = frame.pointer("/headers/req_id").and_then(Value::as_str).unwrap_or("");
                    let _ = conn.send_text(&json!({ "cmd": "pong", "headers": { "req_id": req } }).to_string());
                    continue;
                }
                if cmd != "aibot_msg_callback" {
                    continue;
                }
                let body = frame.get("body").cloned().unwrap_or(Value::Null);
                let Some(message) = decode_callback(&body, &installation.id) else {
                    if let Some(notice) = unsupported_target(&body) {
                        let _ = write_markdown(conn, &notice.0, notice.1, &tr!("目前只处理文字消息。"));
                    }
                    continue;
                };
                if events.try_send(InboundEvent::Message(message)).is_err() {
                    crate::diag::write(&format!(
                        "CHANNEL wecom {} 入站事件积压过多，已丢弃一条",
                        installation.id
                    ));
                }
            }
            Err(WsError::Timeout) => {}
            Err(WsError::Closed(reason)) => return Err(reason),
        }
    }
    Ok(())
}

fn flush(conn: &mut WsConn, outbound: &Receiver<String>) -> Result<(), String> {
    loop {
        match outbound.try_recv() {
            Ok(frame) => conn.send_text(&frame)?,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => return Ok(()),
        }
    }
}

fn subscribe(conn: &mut WsConn, bot_id: &str, secret: &str) -> Result<String, String> {
    let req_id = uuid::Uuid::new_v4().to_string();
    conn.send_text(
        &json!({
            "cmd": "aibot_subscribe",
            "headers": { "req_id": req_id },
            "body": { "bot_id": bot_id, "secret": secret }
        })
        .to_string(),
    )?;
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.read_binary() {
            Ok(data) => {
                let frame: Value = serde_json::from_slice(&data).unwrap_or(Value::Null);
                let echoed = frame.pointer("/headers/req_id").and_then(Value::as_str).unwrap_or("");
                if echoed != req_id {
                    continue;
                }
                let code = frame.get("errcode").and_then(Value::as_i64).unwrap_or(0);
                if code != 0 {
                    return Err(tr!("企业微信拒绝了凭据（{code}）", code = code.to_string()));
                }
                return Ok(req_id);
            }
            Err(WsError::Timeout) => {}
            Err(WsError::Closed(reason)) => return Err(reason),
        }
    }
    Err(tr!("企业微信订阅没有响应"))
}

pub fn send(installation: &Installation, target: &TextTarget<'_>) -> Result<String, String> {
    let chat_type = if target.chat_type == ChatType::Group { 2 } else { 1 };
    let mut last = String::new();
    for chunk in link::chunk_text(target.text, link::TEXT_LIMIT) {
        let frame = json!({
            "cmd": "aibot_send_msg",
            "headers": { "req_id": uuid::Uuid::new_v4().to_string() },
            "body": {
                "chatid": target.chat_id,
                "chat_type": chat_type,
                "msgtype": "markdown",
                "markdown": { "content": chunk }
            }
        });
        push(&installation.id, &frame.to_string())?;
        last = format!("wecom-{}", uuid::Uuid::new_v4());
    }
    Ok(last)
}

fn push(installation_id: &str, frame: &str) -> Result<(), String> {
    let sessions = sessions().lock().map_err(|_| tr!("企业微信发送失败"))?;
    let sender = sessions
        .get(installation_id)
        .ok_or_else(|| tr!("企业微信还没连上"))?;
    sender
        .try_send(frame.to_string())
        .map_err(|_| tr!("企业微信发送队列已满"))
}

fn write_markdown(conn: &mut WsConn, chat_id: &str, chat_type: i64, text: &str) -> Result<(), String> {
    conn.send_text(
        &json!({
            "cmd": "aibot_send_msg",
            "headers": { "req_id": uuid::Uuid::new_v4().to_string() },
            "body": {
                "chatid": chat_id,
                "chat_type": chat_type,
                "msgtype": "markdown",
                "markdown": { "content": text }
            }
        })
        .to_string(),
    )
}

pub fn probe(bot_id: &str, secret: &str) -> Result<(String, String), String> {
    let mut conn = link::connect_ws(WS_URL)?;
    subscribe(&mut conn, bot_id, secret)?;
    conn.send_close();
    Ok((bot_id.to_string(), bot_id.to_string()))
}

pub fn decode_callback(body: &Value, installation_id: &str) -> Option<InboundMessage> {
    let kind = body.get("msgtype").and_then(Value::as_str).unwrap_or("");
    let text = match kind {
        "text" => body.pointer("/text/content").and_then(Value::as_str).unwrap_or(""),
        "voice" => body.pointer("/voice/content").and_then(Value::as_str).unwrap_or(""),
        _ => return None,
    }
    .trim()
    .to_string();
    if text.is_empty() {
        return None;
    }
    let sender = body.pointer("/from/userid").and_then(Value::as_str).unwrap_or("");
    if sender.is_empty() {
        return None;
    }
    let chat_type_raw = body.get("chattype").and_then(Value::as_str).unwrap_or("single");
    let chat_type = if chat_type_raw == "group" {
        ChatType::Group
    } else {
        ChatType::P2p
    };
    let chat_id = body
        .get("chatid")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(sender)
        .to_string();
    let message_id = body.get("msgid").and_then(Value::as_str).unwrap_or("").to_string();
    Some(InboundMessage {
        installation_id: installation_id.to_string(),
        event_id: message_id.clone(),
        message_id,
        chat_id,
        chat_type,
        thread_id: None,
        root_id: None,
        parent_id: None,
        sender_id: sender.to_string(),
        sender_name: None,
        text,
        addressed_to_bot: true,
        media: Vec::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn unsupported_target(body: &Value) -> Option<(String, i64)> {
    let kind = body.get("msgtype").and_then(Value::as_str).unwrap_or("");
    if kind.is_empty() || kind == "text" || kind == "voice" {
        return None;
    }
    let sender = body.pointer("/from/userid").and_then(Value::as_str).unwrap_or("");
    let chat_id = body
        .get("chatid")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(sender);
    if chat_id.is_empty() {
        return None;
    }
    let chat_type = if body.get("chattype").and_then(Value::as_str) == Some("group") {
        2
    } else {
        1
    };
    Some((chat_id.to_string(), chat_type))
}

#[cfg(test)]
mod tests {
    use super::decode_callback;

    #[test]
    fn text_and_voice_transcript() {
        let text = serde_json::json!({
            "msgid": "m1",
            "chatid": "c1",
            "chattype": "single",
            "from": { "userid": "u1" },
            "msgtype": "text",
            "text": { "content": "你好" }
        });
        assert_eq!(decode_callback(&text, "inst").unwrap().text, "你好");
        let voice = serde_json::json!({
            "msgid": "m2",
            "chattype": "group",
            "from": { "userid": "u1" },
            "msgtype": "voice",
            "voice": { "content": "语音转写" }
        });
        let message = decode_callback(&voice, "inst").unwrap();
        assert_eq!(message.text, "语音转写");
        assert_eq!(message.chat_id, "u1");
    }
}
