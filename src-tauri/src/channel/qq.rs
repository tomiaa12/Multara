//! QQ 官方机器人：WebSocket 收单聊和群 @，HTTP 回复。不需要公网地址。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::feishu::ws::WsError;
use super::link::{self, TextTarget};
use super::{store, ChatType, InboundEvent, InboundMessage, Installation};

const API: &str = "https://api.bot.qq.com";
/// 单聊和群 @ 共用的事件订阅位。
const INTENT_GROUP_AND_C2C: u64 = 1 << 25;

struct CachedToken {
    token: String,
    expires: Instant,
}

struct GatewaySession {
    session_id: String,
    seq: i64,
}

fn cache() -> &'static Mutex<HashMap<String, CachedToken>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedToken>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sessions() -> &'static Mutex<HashMap<String, GatewaySession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, GatewaySession>>> = OnceLock::new();
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
    let token = access_token(&installation.config.app_id, secret)?;
    let url = gateway(&token)?;
    let mut conn = link::connect_ws(&url)?;
    let events = link::spawn_workers(app, installation);
    let mut identified = false;
    let mut seq: Option<i64> = None;
    let mut heartbeat_every = Duration::from_secs(45);
    let mut next_heartbeat = Instant::now() + heartbeat_every;

    while !stop.load(Ordering::SeqCst) {
        if identified && Instant::now() >= next_heartbeat {
            send_heartbeat(&mut conn, seq)?;
            next_heartbeat = Instant::now() + heartbeat_every;
        }
        match conn.read_binary() {
            Ok(data) => {
                let frame: Value = match serde_json::from_slice(&data) {
                    Ok(frame) => frame,
                    Err(_) => {
                        crate::diag::write(&format!(
                            "QQ {} 收到无法解析的帧，长度 {}",
                            installation.id,
                            data.len()
                        ));
                        continue;
                    }
                };
                if let Some(latest) = frame.get("s").and_then(Value::as_i64) {
                    seq = Some(latest);
                    remember_seq(&installation.id, latest);
                }
                let op = frame.get("op").and_then(Value::as_i64).unwrap_or(-1);
                let kind = frame.get("t").and_then(Value::as_str).unwrap_or("");
                if op != 11 {
                    crate::diag::write(&format!(
                        "QQ {} op={op} t={kind}",
                        installation.id
                    ));
                }
                match op {
                    10 => {
                        let millis = frame
                            .pointer("/d/heartbeat_interval")
                            .and_then(Value::as_u64)
                            .or_else(|| {
                                frame
                                    .pointer("/d/heartbeat_interval")
                                    .and_then(Value::as_str)
                                    .and_then(|text| text.parse().ok())
                            })
                            .unwrap_or(45_000)
                            .clamp(5_000, 120_000);
                        heartbeat_every = Duration::from_millis(millis);
                        next_heartbeat = Instant::now() + heartbeat_every;
                        let payload = resume_or_identify(&installation.id, &token);
                        conn.send_text(&payload.to_string())?;
                    }
                    1 => {
                        send_heartbeat(&mut conn, seq)?;
                        next_heartbeat = Instant::now() + heartbeat_every;
                    }
                    0 => {
                        if kind == "READY" || kind == "RESUMED" {
                            identified = true;
                            *connected = true;
                            if let Some(data) = frame.get("d") {
                                if kind == "READY" {
                                    let profile = if has_avatar(&installation.config.bot_avatar) {
                                        None
                                    } else {
                                        get_json(&token, "/users/@me").ok()
                                    };
                                    remember_bot(
                                        app,
                                        &installation.id,
                                        profile.as_ref().unwrap_or(data),
                                    );
                                    if let Some(session_id) =
                                        data.get("session_id").and_then(Value::as_str)
                                    {
                                        remember_session(
                                            &installation.id,
                                            session_id,
                                            seq.unwrap_or(0),
                                        );
                                    }
                                }
                            }
                            link::mark_connected(app, installation);
                            send_heartbeat(&mut conn, seq)?;
                            next_heartbeat = Instant::now() + heartbeat_every;
                            continue;
                        }
                        if let Some(message) = decode_event(&frame, &installation.id) {
                            if events.try_send(InboundEvent::Message(message)).is_err() {
                                crate::diag::write(&format!(
                                    "CHANNEL qq {} 入站事件积压过多，已丢弃一条",
                                    installation.id
                                ));
                            }
                        } else if !kind.is_empty() {
                            crate::diag::write(&format!(
                                "QQ {} 忽略事件 {kind}",
                                installation.id
                            ));
                        }
                    }
                    9 => {
                        clear_session(&installation.id);
                        return Err(tr!("QQ 要求重新连接"));
                    }
                    7 => return Err(tr!("QQ 要求重新连接")),
                    11 => {}
                    _ => {}
                }
            }
            Err(WsError::Timeout) => {}
            Err(WsError::Closed(reason)) => {
                if !identified || !can_resume(close_code(&reason)) {
                    clear_session(&installation.id);
                }
                return Err(explain_close(&reason, identified));
            }
        }
    }
    conn.send_close();
    Ok(())
}

fn identify(token: &str) -> Value {
    json!({
        "op": 2,
        "d": {
            "token": format!("QQBot {token}"),
            "intents": INTENT_GROUP_AND_C2C,
            "shard": [0, 1],
            "properties": { "$os": "macos", "$browser": "multara", "$device": "multara" }
        }
    })
}

fn resume_or_identify(installation_id: &str, token: &str) -> Value {
    let saved = sessions()
        .lock()
        .ok()
        .and_then(|sessions| sessions.get(installation_id).map(|item| (item.session_id.clone(), item.seq)));
    if let Some((session_id, seq)) = saved {
        json!({
            "op": 6,
            "d": {
                "token": format!("QQBot {token}"),
                "session_id": session_id,
                "seq": seq
            }
        })
    } else {
        identify(token)
    }
}

fn send_heartbeat(conn: &mut super::feishu::ws::WsConn, seq: Option<i64>) -> Result<(), String> {
    conn.send_text(&json!({ "op": 1, "d": seq }).to_string())
}

fn remember_session(installation_id: &str, session_id: &str, seq: i64) {
    if session_id.is_empty() {
        return;
    }
    if let Ok(mut sessions) = sessions().lock() {
        sessions.insert(
            installation_id.to_string(),
            GatewaySession {
                session_id: session_id.to_string(),
                seq,
            },
        );
    }
}

fn remember_seq(installation_id: &str, seq: i64) {
    if let Ok(mut sessions) = sessions().lock() {
        if let Some(session) = sessions.get_mut(installation_id) {
            session.seq = seq;
        }
    }
}

fn clear_session(installation_id: &str) {
    if let Ok(mut sessions) = sessions().lock() {
        sessions.remove(installation_id);
    }
}

fn close_code(reason: &str) -> Option<u16> {
    let start = reason.find('（')? + '（'.len_utf8();
    let digits: String = reason[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn can_resume(code: Option<u16>) -> bool {
    matches!(code, Some(4008 | 4009)) || code.is_none()
}

fn explain_close(reason: &str, identified: bool) -> String {
    match close_code(reason) {
        Some(4014) => tr!("QQ 没有开通单聊或群消息事件，连接已被关闭"),
        Some(4013) => tr!("QQ 拒绝了当前的事件订阅"),
        Some(4004) => tr!("QQ 拒绝了机器人凭据"),
        Some(4914) => tr!("这个 QQ 机器人还没上线，平台断开了连接"),
        Some(4915) => tr!("这个 QQ 机器人已被封禁"),
        Some(4009) => tr!("QQ 会话过期，正在重新连接"),
        _ if identified => tr!("QQ 连接已断开：{reason}", reason = reason),
        _ => tr!("QQ 登录还没完成连接就断了：{reason}", reason = reason),
    }
}

pub fn send(
    app: &AppHandle,
    installation: &Installation,
    target: &TextTarget<'_>,
) -> Result<String, String> {
    let (_, secret) = link::load(app, &installation.id)?;
    let token = access_token(&installation.config.app_id, &secret)?;
    let path = if target.chat_type == ChatType::Group {
        format!("/v2/groups/{}/messages", encode_path(target.chat_id))
    } else {
        format!("/v2/users/{}/messages", encode_path(target.chat_id))
    };
    let mut last = String::new();
    let mut seq = 1u32;
    for chunk in link::chunk_text(target.text, link::TEXT_LIMIT) {
        let mut body = json!({ "msg_type": 0, "content": chunk, "msg_seq": seq });
        if let Some(message_id) = target.reply_to.filter(|value| !value.is_empty()) {
            body["msg_id"] = json!(message_id);
        }
        seq += 1;
        let value = post(&token, &path, &body)?;
        last = value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("qq")
            .to_string();
    }
    Ok(last)
}

pub fn probe(app_id: &str, secret: &str) -> Result<(String, String), String> {
    let token = access_token(app_id, secret)?;
    let me = get_json(&token, "/users/@me")?;
    let (id, name, avatar) = bot_fields(&me, app_id);
    stash_avatar(app_id, &avatar);
    Ok((id, if name.is_empty() { "QQ".to_string() } else { name }))
}

pub fn cached_avatar(app_id: &str) -> String {
    avatar_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(app_id).cloned())
        .unwrap_or_default()
}

fn gateway(token: &str) -> Result<String, String> {
    let value = get_json(token, "/gateway/bot")?;
    value
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with("wss://"))
        .map(str::to_string)
        .ok_or_else(|| tr!("QQ 没有返回长连接地址"))
}

fn access_token(app_id: &str, secret: &str) -> Result<String, String> {
    if let Some(hit) = cache().lock().ok().and_then(|cache| {
        cache
            .get(app_id)
            .and_then(|item| (item.expires > Instant::now()).then(|| item.token.clone()))
    }) {
        return Ok(hit);
    }
    let response = crate::http_client::request(
        "POST",
        &format!("{API}/app/getAppAccessToken"),
        &[("Content-Type".to_string(), "application/json".to_string())],
        Some(&json!({ "appId": app_id, "clientSecret": secret }).to_string()),
        20,
    )
    .map_err(|error| link::scrub(error, secret))?;
    let value: Value = serde_json::from_str(&response.body)
        .map_err(|_| tr!("QQ 返回了无法解析的内容"))?;
    if let Some(code) = value.get("code").and_then(Value::as_i64).filter(|code| *code != 0) {
        return Err(tr!("QQ 拒绝了凭据（{code}）", code = code.to_string()));
    }
    let token = value
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if token.is_empty() {
        return Err(tr!("QQ 没有返回访问凭证"));
    }
    let expire = value
        .get("expires_in")
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .get("expires_in")
                .and_then(Value::as_str)
                .and_then(|text| text.parse().ok())
        })
        .unwrap_or(7200);
    if let Ok(mut cache) = cache().lock() {
        cache.insert(
            app_id.to_string(),
            CachedToken {
                token: token.clone(),
                expires: Instant::now() + Duration::from_secs(expire.saturating_sub(120).max(60)),
            },
        );
    }
    Ok(token)
}

fn get_json(token: &str, path: &str) -> Result<Value, String> {
    let response = crate::http_client::request(
        "GET",
        &format!("{API}{path}"),
        &[("Authorization".to_string(), format!("QQBot {token}"))],
        None,
        20,
    )
    .map_err(|error| link::scrub(error, token))?;
    parse_api(response.status, &response.body)
}

fn post(token: &str, path: &str, body: &Value) -> Result<Value, String> {
    let response = crate::http_client::request(
        "POST",
        &format!("{API}{path}"),
        &[
            ("Authorization".to_string(), format!("QQBot {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ],
        Some(&body.to_string()),
        20,
    )
    .map_err(|error| link::scrub(error, token))?;
    parse_api(response.status, &response.body)
}

fn parse_api(status: u16, body: &str) -> Result<Value, String> {
    if !(200..300).contains(&status) {
        return Err(tr!("QQ 请求失败（{status}）", status = status.to_string()));
    }
    let value: Value =
        serde_json::from_str(body).map_err(|_| tr!("QQ 返回了无法解析的内容"))?;
    let code = value
        .get("err_code")
        .or_else(|| value.get("code"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if code != 0 {
        return Err(tr!("QQ 拒绝了请求（{code}）", code = code.to_string()));
    }
    Ok(value)
}

fn remember_bot(app: &AppHandle, installation_id: &str, user: &Value) {
    let (id, name, avatar) = bot_fields(user, "");
    if id.is_empty() && name.is_empty() && avatar.is_empty() {
        return;
    }
    let state = app.state::<crate::database::AppState>();
    let saved = {
        let Ok(db) = state.lock_db() else {
            return;
        };
        let Ok(Some(current)) = store::get_installation(&db, installation_id) else {
            return;
        };
        let mut config = current.config;
        if !id.is_empty() {
            config.bot_open_id = Some(id);
        }
        if !name.is_empty() {
            config.bot_name = Some(name);
        }
        if !avatar.is_empty() {
            config.bot_avatar = Some(avatar);
        }
        let name = config.bot_name.clone().unwrap_or_default();
        let avatar = config.bot_avatar.clone().unwrap_or_default();
        if store::set_config(&db, installation_id, &config).is_err() {
            return;
        }
        (name, avatar)
    };
    super::emit_profile(app, installation_id, &saved.0, &saved.1);
}

fn bot_fields(value: &Value, fallback_id: &str) -> (String, String, String) {
    let bot = value.get("user").unwrap_or(value);
    let id = bot
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_id)
        .to_string();
    let name = bot
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let avatar = https_avatar(bot.get("avatar").and_then(Value::as_str).unwrap_or(""));
    (id, name, avatar)
}

fn has_avatar(avatar: &Option<String>) -> bool {
    avatar.as_deref().is_some_and(|value| value.starts_with("https://"))
}

fn https_avatar(raw: &str) -> String {
    let raw = raw.trim();
    if raw.chars().any(char::is_whitespace) {
        return String::new();
    }
    let rest = if let Some(rest) = raw.strip_prefix("https://") {
        rest
    } else if let Some(rest) = raw.strip_prefix("http://") {
        rest
    } else {
        return String::new();
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let qq_image = host == "thirdqq.qlogo.cn"
        || host.ends_with(".qlogo.cn")
        || host.ends_with(".gtimg.cn");
    if raw.starts_with("https://") || qq_image {
        format!("https://{rest}")
    } else {
        String::new()
    }
}

fn avatar_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stash_avatar(app_id: &str, avatar: &str) {
    if avatar.is_empty() {
        return;
    }
    if let Ok(mut cache) = avatar_cache().lock() {
        cache.insert(app_id.to_string(), avatar.to_string());
    }
}

fn encode_path(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn decode_event(frame: &Value, installation_id: &str) -> Option<InboundMessage> {
    if frame.get("op").and_then(Value::as_i64) != Some(0) {
        return None;
    }
    let data = frame.get("d")?;
    if data.pointer("/author/bot").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let kind = frame.get("t").and_then(Value::as_str).unwrap_or("");
    let (chat_type, chat_id, sender_id) = match kind {
        "C2C_MESSAGE_CREATE" => {
            let sender = data
                .pointer("/author/user_openid")
                .or_else(|| data.pointer("/author/id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            (ChatType::P2p, sender, sender)
        }
        "GROUP_AT_MESSAGE_CREATE" => {
            let chat = data.get("group_openid").and_then(Value::as_str).unwrap_or("");
            let sender = data
                .pointer("/author/member_openid")
                .or_else(|| data.pointer("/author/id"))
                .and_then(Value::as_str)
                .unwrap_or("");
            (ChatType::Group, chat, sender)
        }
        _ => return None,
    };
    if chat_id.is_empty() || sender_id.is_empty() {
        return None;
    }
    let mut text = data.get("content").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if text.is_empty() {
        text = data
            .pointer("/attachments/0/asr_refer_text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
    }
    if text.is_empty() {
        return None;
    }
    let message_id = data.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    if message_id.is_empty() {
        return None;
    }
    let name = data
        .pointer("/author/member_nickname")
        .or_else(|| data.pointer("/author/username"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(InboundMessage {
        installation_id: installation_id.to_string(),
        event_id: message_id.clone(),
        message_id,
        chat_id: chat_id.to_string(),
        chat_type,
        thread_id: None,
        root_id: None,
        parent_id: None,
        sender_id: sender_id.to_string(),
        sender_name: name,
        text,
        addressed_to_bot: true,
        media: Vec::new(),
        created_at: data
            .get("timestamp")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
    })
}

#[cfg(test)]
mod tests {
    use super::decode_event;
    use crate::channel::ChatType;

    #[test]
    fn private_text() {
        let frame = serde_json::json!({
            "op": 0,
            "t": "C2C_MESSAGE_CREATE",
            "d": {
                "id": "m1",
                "content": "你好",
                "author": { "user_openid": "user", "username": "小明", "bot": false },
                "timestamp": "2026-07-21T10:00:00+08:00"
            }
        });
        let message = decode_event(&frame, "inst").unwrap();
        assert_eq!(message.text, "你好");
        assert_eq!(message.chat_type, ChatType::P2p);
        assert_eq!(message.chat_id, "user");
        assert_eq!(message.sender_name.as_deref(), Some("小明"));
    }

    #[test]
    fn group_mention_is_addressed() {
        let frame = serde_json::json!({
            "op": 0,
            "t": "GROUP_AT_MESSAGE_CREATE",
            "d": {
                "id": "m2",
                "content": "看一下",
                "group_openid": "group",
                "author": { "member_openid": "member", "bot": false }
            }
        });
        let message = decode_event(&frame, "inst").unwrap();
        assert!(message.addressed_to_bot);
        assert_eq!(message.chat_type, ChatType::Group);
        assert_eq!(message.chat_id, "group");
        assert_eq!(message.sender_id, "member");
    }

    #[test]
    fn bot_profile_keeps_https_avatar() {
        let user = serde_json::json!({
            "id": "1",
            "username": "multara机器人",
            "avatar": "https://thirdqq.qlogo.cn/g?b=oidb&k=abc&s=100"
        });
        let (id, name, avatar) = super::bot_fields(&user, "app");
        assert_eq!(id, "1");
        assert_eq!(name, "multara机器人");
        assert!(avatar.starts_with("https://thirdqq.qlogo.cn/"));
        let http = serde_json::json!({
            "avatar": "http://thirdqq.qlogo.cn/g?b=oidb&k=abc&s=100"
        });
        assert_eq!(
            super::bot_fields(&http, "app").2,
            "https://thirdqq.qlogo.cn/g?b=oidb&k=abc&s=100"
        );
        let nested = serde_json::json!({ "user": { "avatar": "http://insecure.example/a.png" } });
        assert!(super::bot_fields(&nested, "app").2.is_empty());
    }

    #[test]
    fn close_code_reads_the_gateway_number() {
        assert_eq!(super::close_code("对端关闭了长连接（4014）"), Some(4014));
        assert_eq!(super::close_code("长连接被对端关闭"), None);
        assert!(!super::can_resume(Some(4014)));
        assert!(super::can_resume(Some(4009)));
    }

    #[test]
    fn bot_message_is_dropped() {
        let frame = serde_json::json!({
            "op": 0,
            "t": "C2C_MESSAGE_CREATE",
            "d": { "id": "m3", "content": "hi", "author": { "id": "bot", "bot": true } }
        });
        assert!(decode_event(&frame, "inst").is_none());
    }
}
