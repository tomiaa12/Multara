//! 钉钉 Stream 模式：AppKey / AppSecret 建长连接，回复走开放接口。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::feishu::ws::WsError;
use super::link::{self, TextTarget};
use super::{ChatType, InboundEvent, InboundMessage, Installation};

const API: &str = "https://api.dingtalk.com";

struct CachedToken {
    token: String,
    expires: Instant,
}

fn cache() -> &'static Mutex<HashMap<String, CachedToken>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedToken>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
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
    let url = open_connection(&installation.config.app_id, secret)?;
    let mut conn = link::connect_ws(&url)?;
    *connected = true;
    link::mark_connected(app, installation);
    refresh_identity(app, installation, secret);
    let events = link::spawn_workers(app, installation);
    while !stop.load(Ordering::SeqCst) {
        match conn.read_binary() {
            Ok(data) => {
                let frame: Value = match serde_json::from_slice(&data) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };
                let kind = frame.get("type").and_then(Value::as_str).unwrap_or("");
                let topic = frame
                    .pointer("/headers/topic")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let message_id = frame
                    .pointer("/headers/messageId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if kind == "SYSTEM" && topic == "disconnect" {
                    let _ = conn.send_text(&ack(message_id, "").to_string());
                    return Err(tr!("钉钉要求重新连接"));
                }
                if kind == "SYSTEM" && topic == "ping" {
                    let data = frame.get("data").and_then(Value::as_str).unwrap_or("");
                    let _ = conn.send_text(&pong(message_id, data).to_string());
                    continue;
                }
                let _ = conn.send_text(&ack(message_id, "").to_string());
                if topic != "/v1.0/im/bot/messages/get" {
                    continue;
                }
                let raw = frame.get("data").and_then(Value::as_str).unwrap_or("");
                let Ok(body) = serde_json::from_str::<Value>(raw) else {
                    continue;
                };
                let Some(message) = decode_callback(&body, &installation.id) else {
                    continue;
                };
                if let Some(bot) = body.get("chatbotUserId").and_then(Value::as_str) {
                    remember_bot(app, installation, bot);
                }
                if events.try_send(InboundEvent::Message(message)).is_err() {
                    crate::diag::write(&format!(
                        "CHANNEL dingtalk {} 入站事件积压过多，已丢弃一条",
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

fn ack(message_id: &str, data: &str) -> Value {
    json!({
        "code": 200,
        "message": "",
        "data": data,
        "headers": { "messageId": message_id, "contentType": "application/json" }
    })
}

fn pong(message_id: &str, data: &str) -> Value {
    json!({
        "code": 200,
        "message": "ok",
        "data": data,
        "headers": { "messageId": message_id, "contentType": "application/json" }
    })
}

pub fn send(
    app: &AppHandle,
    installation: &Installation,
    target: &TextTarget<'_>,
) -> Result<String, String> {
    let (_, secret) = link::load(app, &installation.id)?;
    let robot = installation.config.app_id.clone();
    let mut last = String::new();
    for chunk in link::chunk_text(target.text, link::TEXT_LIMIT) {
        let title: String = chunk.chars().take(20).collect();
        let param = json!({ "title": title, "text": chunk }).to_string();
        let (path, body) = if target.chat_type == ChatType::P2p {
            let user = target.sender_id.unwrap_or("").trim();
            if user.is_empty() {
                return Err(tr!("钉钉私聊缺少发送者"));
            }
            (
                "/v1.0/robot/oToMessages/batchSend",
                json!({
                    "robotCode": robot,
                    "userIds": [user],
                    "msgKey": "sampleMarkdown",
                    "msgParam": param,
                }),
            )
        } else {
            (
                "/v1.0/robot/groupMessages/send",
                json!({
                    "robotCode": robot,
                    "openConversationId": target.chat_id,
                    "msgKey": "sampleMarkdown",
                    "msgParam": param,
                }),
            )
        };
        last = post_robot(&installation.config.app_id, &secret, path, &body)?;
    }
    Ok(last)
}

pub fn probe(app_key: &str, secret: &str) -> Result<(String, String), String> {
    let _ = access_token(app_key, secret)?;
    match fetch_profile(app_key, secret) {
        Ok(Some(profile)) => {
            stash_avatar(app_key, &profile.avatar);
            Ok((app_key.to_string(), profile.name))
        }
        _ => Ok((app_key.to_string(), String::new())),
    }
}

pub fn cached_avatar(app_key: &str) -> String {
    avatar_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(app_key).cloned())
        .unwrap_or_default()
}

fn open_connection(app_key: &str, secret: &str) -> Result<String, String> {
    let body = json!({
        "clientId": app_key,
        "clientSecret": secret,
        "ua": "multara-dingtalk/1.0",
        "subscriptions": [
            { "type": "SYSTEM", "topic": "ping" },
            { "type": "SYSTEM", "topic": "disconnect" },
            { "type": "CALLBACK", "topic": "/v1.0/im/bot/messages/get" }
        ]
    });
    let value = post_json(&format!("{API}/v1.0/gateway/connections/open"), None, &body)?;
    let endpoint = value.get("endpoint").and_then(Value::as_str).unwrap_or("");
    let ticket = value.get("ticket").and_then(Value::as_str).unwrap_or("");
    if !endpoint.starts_with("wss://") || ticket.is_empty() {
        return Err(tr!("钉钉没有返回长连接地址"));
    }
    let joiner = if endpoint.contains('?') { '&' } else { '?' };
    Ok(format!("{endpoint}{joiner}ticket={ticket}"))
}

fn post_robot(app_key: &str, secret: &str, path: &str, body: &Value) -> Result<String, String> {
    let mut last_error = String::new();
    for attempt in 0..2 {
        let token = access_token(app_key, secret)?;
        match post_json(&format!("{API}{path}"), Some(&token), body) {
            Ok(value) => {
                return Ok(value
                    .get("processQueryKey")
                    .and_then(Value::as_str)
                    .unwrap_or("dingtalk")
                    .to_string());
            }
            Err(error) if attempt == 0 && error.contains("401") => {
                cache().lock().ok().map(|mut cache| cache.remove(app_key));
                last_error = error;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error)
}

fn access_token(app_key: &str, secret: &str) -> Result<String, String> {
    if let Some(hit) = cache().lock().ok().and_then(|cache| {
        cache
            .get(app_key)
            .and_then(|item| (item.expires > Instant::now()).then(|| item.token.clone()))
    }) {
        return Ok(hit);
    }
    let value = post_json(
        &format!("{API}/v1.0/oauth2/accessToken"),
        None,
        &json!({ "appKey": app_key, "appSecret": secret }),
    )?;
    let token = value
        .get("accessToken")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if token.is_empty() {
        return Err(tr!("钉钉没有返回访问令牌"));
    }
    let expire = value
        .get("expireIn")
        .and_then(Value::as_u64)
        .unwrap_or(7200);
    if let Ok(mut cache) = cache().lock() {
        cache.insert(
            app_key.to_string(),
            CachedToken {
                token: token.clone(),
                expires: Instant::now() + Duration::from_secs(expire.saturating_sub(300).max(60)),
            },
        );
    }
    Ok(token)
}

fn post_json(url: &str, token: Option<&str>, body: &Value) -> Result<Value, String> {
    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    if let Some(token) = token {
        headers.push(("x-acs-dingtalk-access-token".to_string(), token.to_string()));
    }
    let response = crate::http_client::request("POST", url, &headers, Some(&body.to_string()), 20)?;
    if response.status == 401 {
        return Err(tr!("钉钉拒绝了凭据（401）"));
    }
    if !(200..300).contains(&response.status) {
        return Err(tr!(
            "钉钉请求失败（{status}）",
            status = response.status.to_string()
        ));
    }
    serde_json::from_str(&response.body).map_err(|_| tr!("钉钉返回了无法解析的内容"))
}

struct Profile {
    name: String,
    avatar: String,
}

enum ProfileError {
    Permission,
    Other(String),
}

fn avatar_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn denial_cache() -> &'static Mutex<HashMap<String, Instant>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stash_avatar(app_key: &str, avatar: &str) {
    if avatar.is_empty() {
        return;
    }
    if let Ok(mut cache) = avatar_cache().lock() {
        cache.insert(app_key.to_string(), avatar.to_string());
    }
}

fn permission_cooled(app_key: &str) -> bool {
    denial_cache().lock().ok().is_some_and(|cache| {
        cache
            .get(app_key)
            .is_some_and(|until| *until > Instant::now())
    })
}

fn mark_permission(app_key: &str) {
    if let Ok(mut cache) = denial_cache().lock() {
        cache.insert(
            app_key.to_string(),
            Instant::now() + Duration::from_secs(30 * 60),
        );
    }
}

fn clear_permission(app_key: &str) {
    if let Ok(mut cache) = denial_cache().lock() {
        cache.remove(app_key);
    }
}

/// 钉钉没有按 robotCode 单独查资料的接口。群列表和群内机器人都要
/// `qyapi_chat_manage`，并且机器人已经在内部群里。缺权限或还没进群时连接照常。
fn refresh_identity(app: &AppHandle, installation: &Installation, secret: &str) {
    let named = installation
        .config
        .bot_name
        .as_deref()
        .is_some_and(|name| !name.is_empty());
    let pictured = installation
        .config
        .bot_avatar
        .as_deref()
        .is_some_and(|avatar| avatar.starts_with("https://"));
    if (named && pictured) || permission_cooled(&installation.config.app_id) {
        return;
    }
    match fetch_profile(&installation.config.app_id, secret) {
        Ok(Some(profile)) => {
            clear_permission(&installation.config.app_id);
            stash_avatar(&installation.config.app_id, &profile.avatar);
            save_profile(app, &installation.id, &profile);
        }
        Err(ProfileError::Permission) => mark_permission(&installation.config.app_id),
        Ok(None) => {}
        Err(ProfileError::Other(error)) => {
            crate::diag::write(&format!(
                "CHANNEL dingtalk {} 没有读到机器人资料：{error}",
                installation.id
            ));
        }
    }
}

fn fetch_profile(app_key: &str, secret: &str) -> Result<Option<Profile>, ProfileError> {
    let groups = post_profile(
        app_key,
        secret,
        "/v1.0/robot/groups/query",
        &json!({ "robotCode": app_key, "pageNumber": 1, "pageSize": 20 }),
    )?;
    let ids = groups
        .get("openConversationIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for id in ids.iter().filter_map(Value::as_str).take(5) {
        let bots = post_profile(
            app_key,
            secret,
            "/v1.0/robot/groups/robots/query",
            &json!({ "openConversationId": id }),
        )?;
        if let Some(profile) = parse_group_bots(&bots, app_key) {
            return Ok(Some(profile));
        }
    }
    Ok(None)
}

fn post_profile(
    app_key: &str,
    secret: &str,
    path: &str,
    body: &Value,
) -> Result<Value, ProfileError> {
    let mut last = ProfileError::Other(String::new());
    for attempt in 0..2 {
        let token = access_token(app_key, secret).map_err(ProfileError::Other)?;
        let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        headers.push(("x-acs-dingtalk-access-token".to_string(), token));
        let response = crate::http_client::request(
            "POST",
            &format!("{API}{path}"),
            &headers,
            Some(&body.to_string()),
            20,
        )
        .map_err(ProfileError::Other)?;
        if response.status == 401 && attempt == 0 {
            cache().lock().ok().map(|mut cache| cache.remove(app_key));
            last = ProfileError::Other(tr!("钉钉拒绝了凭据（401）"));
            continue;
        }
        if let Some(scope) = permission_scope(&response.body) {
            if scope == "qyapi_chat_manage" {
                return Err(ProfileError::Permission);
            }
        }
        if !(200..300).contains(&response.status) {
            return Err(ProfileError::Other(tr!(
                "钉钉请求失败（{status}）",
                status = response.status.to_string()
            )));
        }
        return serde_json::from_str(&response.body)
            .map_err(|_| ProfileError::Other(tr!("钉钉返回了无法解析的内容")));
    }
    Err(last)
}

fn permission_scope(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let code = value.get("code").and_then(Value::as_str).unwrap_or("");
    if !code.contains("PermissionDenied") {
        return None;
    }
    value
        .pointer("/accessdenieddetail/requiredScopes")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let message = value.get("message").and_then(Value::as_str).unwrap_or("");
            message
                .contains("qyapi_chat_manage")
                .then(|| "qyapi_chat_manage".to_string())
        })
}

fn parse_group_bots(value: &Value, robot_code: &str) -> Option<Profile> {
    let list = value.get("chatbotInstanceVOList")?.as_array()?;
    for bot in list {
        let code = bot.get("robotCode").and_then(Value::as_str).unwrap_or("");
        if code != robot_code {
            continue;
        }
        let name = bot
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let avatar = https_avatar(
            bot.get("downloadIconURL")
                .and_then(Value::as_str)
                .unwrap_or(""),
        );
        if name.is_empty() && avatar.is_empty() {
            return None;
        }
        return Some(Profile { name, avatar });
    }
    None
}

fn https_avatar(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("https://") && !raw.chars().any(char::is_whitespace) {
        raw.to_string()
    } else {
        String::new()
    }
}

fn save_profile(app: &AppHandle, installation_id: &str, profile: &Profile) {
    let state = app.state::<crate::database::AppState>();
    let saved = {
        let Ok(db) = state.lock_db() else {
            return;
        };
        let Ok(Some(current)) = super::store::get_installation(&db, installation_id) else {
            return;
        };
        let mut config = current.config;
        if !profile.name.is_empty() {
            config.bot_name = Some(profile.name.clone());
        }
        if !profile.avatar.is_empty() {
            config.bot_avatar = Some(profile.avatar.clone());
        }
        let name = config.bot_name.clone().unwrap_or_default();
        let avatar = config.bot_avatar.clone().unwrap_or_default();
        if super::store::set_config(&db, installation_id, &config).is_err() {
            return;
        }
        (name, avatar)
    };
    super::emit_profile(app, installation_id, &saved.0, &saved.1);
}

fn remember_bot(app: &AppHandle, installation: &Installation, bot_user: &str) {
    let state = app.state::<crate::database::AppState>();
    let Ok(db) = state.lock_db() else {
        return;
    };
    let Ok(Some(current)) = super::store::get_installation(&db, &installation.id) else {
        return;
    };
    if current.config.bot_open_id.as_deref() == Some(bot_user) {
        return;
    }
    let mut config = current.config;
    config.bot_open_id = Some(bot_user.to_string());
    let _ = super::store::set_config(&db, &installation.id, &config);
}

pub fn decode_callback(body: &Value, installation_id: &str) -> Option<InboundMessage> {
    let sender = body
        .get("senderStaffId")
        .and_then(Value::as_str)
        .unwrap_or("");
    if sender.is_empty() {
        return None;
    }
    let kind = body.get("msgtype").and_then(Value::as_str).unwrap_or("");
    if kind != "text" {
        return None;
    }
    let text = body
        .pointer("/text/content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return None;
    }
    let conversation = body
        .get("conversationType")
        .and_then(Value::as_str)
        .unwrap_or("1");
    let chat_type = if conversation == "2" {
        ChatType::Group
    } else {
        ChatType::P2p
    };
    let addressed = chat_type == ChatType::P2p
        || body
            .get("isInAtList")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let text = if chat_type == ChatType::Group {
        strip_leading_mention(&text)
    } else {
        text
    };
    if text.is_empty() {
        return None;
    }
    let message_id = body
        .get("msgId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(InboundMessage {
        installation_id: installation_id.to_string(),
        event_id: message_id.clone(),
        message_id,
        chat_id: body
            .get("conversationId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        chat_type,
        thread_id: None,
        root_id: None,
        parent_id: None,
        sender_id: sender.to_string(),
        sender_name: body
            .get("senderNick")
            .and_then(Value::as_str)
            .map(str::to_string),
        text,
        addressed_to_bot: addressed,
        media: Vec::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn strip_leading_mention(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix('@') {
        rest.split_once(|ch: char| ch.is_whitespace())
            .map(|(_, rest)| rest.trim().to_string())
            .unwrap_or_default()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_callback, parse_group_bots, permission_scope};
    use crate::channel::ChatType;

    #[test]
    fn group_mention_is_stripped() {
        let body = serde_json::json!({
            "conversationId": "cid",
            "conversationType": "2",
            "senderStaffId": "staff",
            "senderNick": "张三",
            "msgId": "m1",
            "msgtype": "text",
            "isInAtList": true,
            "text": { "content": "@机器人 看一下" }
        });
        let message = decode_callback(&body, "inst").unwrap();
        assert_eq!(message.text, "看一下");
        assert_eq!(message.chat_type, ChatType::Group);
        assert!(message.addressed_to_bot);
        assert_eq!(message.sender_name.as_deref(), Some("张三"));
    }

    #[test]
    fn reads_only_the_matching_robot() {
        let value = serde_json::json!({
            "chatbotInstanceVOList": [
                { "robotCode": "other", "name": "别人", "downloadIconURL": "https://example.com/a.png" },
                { "robotCode": "ding", "name": "发布助手", "downloadIconURL": "https://example.com/b.png" }
            ]
        });
        let profile = parse_group_bots(&value, "ding").unwrap();
        assert_eq!(profile.name, "发布助手");
        assert_eq!(profile.avatar, "https://example.com/b.png");
        assert!(parse_group_bots(&value, "missing").is_none());
    }

    #[test]
    fn drops_non_https_icons() {
        let value = serde_json::json!({
            "chatbotInstanceVOList": [
                { "robotCode": "ding", "name": "发布助手", "downloadIconURL": "http://example.com/a.png" }
            ]
        });
        let profile = parse_group_bots(&value, "ding").unwrap();
        assert_eq!(profile.avatar, "");
    }

    #[test]
    fn permission_error_names_the_scope() {
        let body = r#"{"code":"Forbidden.AccessDenied.AccessTokenPermissionDenied","message":"应用尚未开通所需的权限：[qyapi_chat_manage]","accessdenieddetail":{"requiredScopes":["qyapi_chat_manage"]}}"#;
        assert_eq!(permission_scope(body).as_deref(), Some("qyapi_chat_manage"));
        assert!(permission_scope(r#"{"code":"InvalidAction.NotFound"}"#).is_none());
    }
}
