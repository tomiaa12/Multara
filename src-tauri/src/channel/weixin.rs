//! 微信 iLink：扫码拿到 bot token，再用长轮询收私聊。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::link::{self, TextTarget};
use super::{
    secret, store, supervisor, ChatType, InboundEvent, InboundMessage, Installation, TYPE_WEIXIN,
};

const DEFAULT_BASE: &str = "https://ilinkai.weixin.qq.com";
const CLIENT_VERSION: &str = "132106";

struct Login {
    qrcode: String,
    image: String,
    base_url: String,
    name: String,
    project_id: Option<String>,
    verify_code: String,
    started: Instant,
}

fn logins() -> &'static Mutex<std::collections::HashMap<String, Login>> {
    static LOGINS: OnceLock<Mutex<std::collections::HashMap<String, Login>>> = OnceLock::new();
    LOGINS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginView {
    pub session_id: String,
    pub status: String,
    pub image_url: String,
    pub installation_id: Option<String>,
}

pub fn begin(name: &str, project_id: Option<String>) -> Result<LoginView, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(tr!("请填写名称"));
    }
    let qr = qr_code(DEFAULT_BASE, &[])?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let view = LoginView {
        session_id: session_id.clone(),
        status: "wait".to_string(),
        image_url: qr.image.clone(),
        installation_id: None,
    };
    logins()
        .lock()
        .map_err(|_| tr!("微信登录状态不可用"))?
        .insert(
            session_id,
            Login {
                qrcode: qr.code,
                image: qr.image,
                base_url: DEFAULT_BASE.to_string(),
                name: name.to_string(),
                project_id: project_id.filter(|value| !value.is_empty()),
                verify_code: String::new(),
                started: Instant::now(),
            },
        );
    Ok(view)
}

pub fn submit_code(session_id: &str, code: &str) -> Result<(), String> {
    let code = code.trim();
    if code.is_empty() || !code.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(tr!("请输入手机上显示的数字验证码"));
    }
    let mut logins = logins().lock().map_err(|_| tr!("微信登录状态不可用"))?;
    let login = logins
        .get_mut(session_id)
        .ok_or_else(|| tr!("二维码已失效，请重新生成"))?;
    login.verify_code = code.to_string();
    Ok(())
}

pub fn poll(app: &AppHandle, session_id: &str) -> Result<LoginView, String> {
    let (qrcode, image, mut base, verify, name, project_id, started) = {
        let logins = logins().lock().map_err(|_| tr!("微信登录状态不可用"))?;
        let login = logins
            .get(session_id)
            .ok_or_else(|| tr!("二维码已失效，请重新生成"))?;
        if login.started.elapsed() > Duration::from_secs(300) {
            return Ok(LoginView {
                session_id: session_id.to_string(),
                status: "expired".to_string(),
                image_url: login.image.clone(),
                installation_id: None,
            });
        }
        (
            login.qrcode.clone(),
            login.image.clone(),
            login.base_url.clone(),
            login.verify_code.clone(),
            login.name.clone(),
            login.project_id.clone(),
            login.started,
        )
    };
    let _ = started;
    let status = qr_status(&base, &qrcode, &verify)?;
    if status.status == "scaned_but_redirect" {
        if let Some(host) = status
            .redirect_host
            .filter(|host| host.ends_with(".weixin.qq.com"))
        {
            base = format!("https://{host}");
            if let Ok(mut logins) = logins().lock() {
                if let Some(login) = logins.get_mut(session_id) {
                    login.base_url = base;
                }
            }
        }
        return Ok(LoginView {
            session_id: session_id.to_string(),
            status: "scanned".to_string(),
            image_url: image,
            installation_id: None,
        });
    }
    let public_status = match status.status.as_str() {
        "wait" => "wait",
        "scaned" => "scanned",
        "confirmed" => "confirmed",
        "expired" => "expired",
        other if other.contains("need_verifycode") => "need_verifycode",
        _ => "wait",
    };
    if public_status != "confirmed" {
        return Ok(LoginView {
            session_id: session_id.to_string(),
            status: public_status.to_string(),
            image_url: image,
            installation_id: None,
        });
    }
    let token = status.token.ok_or_else(|| tr!("微信没有返回登录凭据"))?;
    let bot_id = status.bot_id.ok_or_else(|| tr!("微信没有返回机器人"))?;
    let user_id = status.user_id.ok_or_else(|| tr!("微信没有返回账号"))?;
    let base_url = validate_base(status.base_url.as_deref().unwrap_or(&base))?;
    let installation_id = save_account(
        app,
        &name,
        project_id.as_deref(),
        &bot_id,
        &user_id,
        &base_url,
        &token,
    )?;
    if let Ok(mut logins) = logins().lock() {
        logins.remove(session_id);
    }
    supervisor::reload(app);
    Ok(LoginView {
        session_id: session_id.to_string(),
        status: "confirmed".to_string(),
        image_url: image,
        installation_id: Some(installation_id),
    })
}

fn save_account(
    app: &AppHandle,
    name: &str,
    project_id: Option<&str>,
    bot_id: &str,
    user_id: &str,
    base_url: &str,
    token: &str,
) -> Result<String, String> {
    let state = app.state::<crate::database::AppState>();
    let db = state.lock_db()?;
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM channel_installations
             WHERE channel_type=?1 AND json_extract(config_json, '$.app_id')=?2",
            [TYPE_WEIXIN, bot_id],
            |row| row.get(0),
        )
        .ok();
    let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let previous = store::get_installation(&db, &id)?;
    let previous_target_type = previous
        .as_ref()
        .map(|item| item.target_type.clone())
        .unwrap_or_else(|| "agent".to_string());
    let previous_target_id = previous
        .as_ref()
        .map(|item| item.target_id.clone())
        .unwrap_or_default();
    let previous_project = previous.as_ref().and_then(|item| item.project_id.clone());
    let mut config = previous
        .as_ref()
        .map(|item| item.config.clone())
        .unwrap_or_default();
    config.app_id = bot_id.to_string();
    config.bot_open_id = Some(user_id.to_string());
    config.bot_name = Some(tr!("微信"));
    config.api_base = Some(base_url.to_string());
    config.group_enabled = false;
    config.credential_rev = config.credential_rev.saturating_add(1);
    if config.pairing_code.is_empty() {
        config.pairing_code = super::commands::generate_pairing_code();
    }
    let installation = Installation {
        id: id.clone(),
        channel_type: TYPE_WEIXIN.to_string(),
        name: name.to_string(),
        target_type: previous_target_type,
        target_id: previous_target_id,
        project_id: project_id.map(str::to_string).or(previous_project),
        config,
        enabled: true,
        status: "disconnected".to_string(),
        last_error: None,
    };
    drop(db);
    secret::store(app, &id, token)?;
    let db = state.lock_db()?;
    store::save_installation(&db, &installation)?;
    store::add_allowed_user(&db, &id, user_id, Some(&tr!("微信")), "owner")?;
    Ok(id)
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
    let base = installation
        .config
        .api_base
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE.to_string());
    let base = validate_base(&base)?;
    let _ = notify(&base, secret, true);
    *connected = true;
    link::mark_connected(app, installation);
    let events = link::spawn_workers(app, installation);
    let mut cursor = installation.config.update_cursor.clone();
    let result = poll_loop(app, installation, &base, secret, &mut cursor, &events, stop);
    let _ = notify(&base, secret, false);
    result
}

fn poll_loop(
    app: &AppHandle,
    installation: &Installation,
    base: &str,
    token: &str,
    cursor: &mut String,
    events: &std::sync::mpsc::SyncSender<InboundEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    while !stop.load(Ordering::SeqCst) {
        let batch = match updates(base, token, cursor) {
            Ok(batch) => batch,
            Err(error) if error == "expired" => {
                disable(app, &installation.id);
                return Err(tr!("微信授权已过期，请重新扫码"));
            }
            Err(error) => return Err(error),
        };
        if !batch.cursor.is_empty() && batch.cursor != *cursor {
            *cursor = batch.cursor.clone();
            save_cursor(app, &installation.id, cursor);
        }
        for (message, context) in batch.messages {
            if let Some(context) = context {
                save_context(app, &installation.id, &context);
            }
            if events.try_send(InboundEvent::Message(message)).is_err() {
                crate::diag::write(&format!(
                    "CHANNEL weixin {} 入站事件积压过多，已丢弃一条",
                    installation.id
                ));
            }
        }
    }
    Ok(())
}

struct UpdateBatch {
    messages: Vec<(InboundMessage, Option<String>)>,
    cursor: String,
}

fn updates(base: &str, token: &str, cursor: &str) -> Result<UpdateBatch, String> {
    let value = request(
        base,
        "POST",
        "getupdates",
        Some(token),
        Some(&json!({
            "get_updates_buf": cursor,
            "base_info": base_info()
        })),
        40,
    )?;
    check_result(&value)?;
    let cursor = value
        .get("get_updates_buf")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let messages = value
        .get("msgs")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let context = item
                        .get("context_token")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string);
                    Some((decode_message(item)?, context))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(UpdateBatch { messages, cursor })
}

pub fn send(
    app: &AppHandle,
    installation: &Installation,
    target: &TextTarget<'_>,
) -> Result<String, String> {
    let (fresh, token) = link::load(app, &installation.id)?;
    let context = fresh.config.context_token.trim();
    if context.is_empty() {
        return Err(tr!("请先在微信里再发一条消息，然后再回复"));
    }
    let base = validate_base(fresh.config.api_base.as_deref().unwrap_or(DEFAULT_BASE))?;
    let mut last = String::new();
    for chunk in link::chunk_text(target.text, link::TEXT_LIMIT) {
        let client_id = uuid::Uuid::new_v4().to_string();
        let value = request(
            &base,
            "POST",
            "sendmessage",
            Some(&token),
            Some(&json!({
                "msg": {
                    "to_user_id": target.chat_id,
                    "client_id": client_id,
                    "message_type": 2,
                    "message_state": 2,
                    "context_token": context,
                    "item_list": [{ "type": 1, "text_item": { "text": chunk } }]
                },
                "base_info": base_info()
            })),
            20,
        )?;
        check_result(&value)?;
        last = value
            .get("message_id")
            .map(value_id)
            .filter(|id| !id.is_empty())
            .unwrap_or(client_id);
    }
    Ok(last)
}

pub fn probe(installation: &Installation, secret: &str) -> Result<(String, String), String> {
    if secret.is_empty() {
        return Err(tr!("微信凭据缺失，请重新扫码"));
    }
    let name = installation
        .config
        .bot_name
        .clone()
        .unwrap_or_else(|| tr!("微信"));
    Ok((
        installation.config.bot_open_id.clone().unwrap_or_default(),
        name,
    ))
}

fn disable(app: &AppHandle, installation_id: &str) {
    let state = app.state::<crate::database::AppState>();
    if let Ok(db) = state.lock_db() {
        let _ = db.execute(
            "UPDATE channel_installations SET enabled=0, status='error', last_error=?1, updated_at=?2 WHERE id=?3",
            [
                tr!("微信授权已过期，请重新扫码"),
                chrono::Utc::now().to_rfc3339(),
                installation_id.to_string(),
            ],
        );
    };
}

fn save_cursor(app: &AppHandle, installation_id: &str, cursor: &str) {
    let state = app.state::<crate::database::AppState>();
    let Ok(db) = state.lock_db() else {
        return;
    };
    let Ok(Some(current)) = store::get_installation(&db, installation_id) else {
        return;
    };
    let mut config = current.config;
    config.update_cursor = cursor.to_string();
    let _ = store::set_config(&db, installation_id, &config);
}

fn save_context(app: &AppHandle, installation_id: &str, context: &str) {
    let state = app.state::<crate::database::AppState>();
    let Ok(db) = state.lock_db() else {
        return;
    };
    let Ok(Some(current)) = store::get_installation(&db, installation_id) else {
        return;
    };
    if current.config.context_token == context {
        return;
    }
    let mut config = current.config;
    config.context_token = context.to_string();
    let _ = store::set_config(&db, installation_id, &config);
}

struct Qr {
    code: String,
    image: String,
}

fn qr_code(base: &str, tokens: &[String]) -> Result<Qr, String> {
    let value = request(
        base,
        "POST",
        "get_bot_qrcode?bot_type=3",
        None,
        Some(&json!({ "local_token_list": tokens })),
        20,
    )?;
    check_result(&value)?;
    let code = value
        .get("qrcode")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let content = value
        .get("qrcode_img_content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if code.is_empty() || content.is_empty() {
        return Err(tr!("微信没有返回二维码"));
    }
    // 这是扫码内容，不是图片。直接放进 img 会显示成问号。
    let image = qr_image_data_url(&content)?;
    Ok(Qr { code, image })
}

fn qr_image_data_url(content: &str) -> Result<String, String> {
    if content.starts_with("data:image/") {
        return Ok(content.to_string());
    }
    let code = qrcode::QrCode::new(content.as_bytes()).map_err(|_| tr!("微信没有返回二维码"))?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(180, 180)
        .quiet_zone(true)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    let mut url = String::from("data:image/svg+xml;charset=utf-8,");
    for byte in svg.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                url.push(byte as char);
            }
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    Ok(url)
}

struct QrStatus {
    status: String,
    token: Option<String>,
    bot_id: Option<String>,
    user_id: Option<String>,
    base_url: Option<String>,
    redirect_host: Option<String>,
}

fn qr_status(base: &str, qrcode: &str, verify: &str) -> Result<QrStatus, String> {
    let mut query = format!("get_qrcode_status?qrcode={}", urlencoding(qrcode));
    if !verify.is_empty() {
        query.push_str("&verify_code=");
        query.push_str(&urlencoding(verify));
    }
    let value = request(base, "GET", &query, None, None, 40)?;
    check_result(&value)?;
    Ok(QrStatus {
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        token: value
            .get("bot_token")
            .and_then(Value::as_str)
            .map(str::to_string),
        bot_id: value
            .get("ilink_bot_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        user_id: value
            .get("ilink_user_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        base_url: value
            .get("baseurl")
            .and_then(Value::as_str)
            .map(str::to_string),
        redirect_host: value
            .get("redirect_host")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn notify(base: &str, token: &str, start: bool) -> Result<(), String> {
    let endpoint = if start {
        "msg/notifystart"
    } else {
        "msg/notifystop"
    };
    let value = request(
        base,
        "POST",
        endpoint,
        Some(token),
        Some(&json!({ "base_info": base_info() })),
        15,
    )?;
    check_result(&value)
}

fn request(
    base: &str,
    method: &str,
    endpoint: &str,
    token: Option<&str>,
    body: Option<&Value>,
    timeout: u32,
) -> Result<Value, String> {
    let base = validate_base(base)?;
    let url = format!("{base}/ilink/bot/{endpoint}");
    let mut headers = vec![
        ("iLink-App-Id".to_string(), "bot".to_string()),
        (
            "iLink-App-ClientVersion".to_string(),
            CLIENT_VERSION.to_string(),
        ),
    ];
    if method == "POST" {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
        headers.push((
            "AuthorizationType".to_string(),
            "ilink_bot_token".to_string(),
        ));
        headers.push(("X-WECHAT-UIN".to_string(), random_uin()));
    }
    if let Some(token) = token {
        headers.push(("Authorization".to_string(), format!("Bearer {token}")));
    }
    let payload = body.map(|value| value.to_string());
    let response = crate::http_client::request(method, &url, &headers, payload.as_deref(), timeout)
        .map_err(|error| link::scrub(error, token.unwrap_or("")))?;
    if !(200..300).contains(&response.status) {
        return Err(tr!(
            "微信请求失败（{status}）",
            status = response.status.to_string()
        ));
    }
    serde_json::from_str(&response.body).map_err(|_| tr!("微信返回了无法解析的内容"))
}

fn check_result(value: &Value) -> Result<(), String> {
    let code = value
        .get("errcode")
        .and_then(Value::as_i64)
        .or_else(|| value.get("ret").and_then(Value::as_i64))
        .unwrap_or(0);
    if code == -14 {
        return Err("expired".to_string());
    }
    if code != 0 {
        return Err(tr!("微信拒绝了请求（{code}）", code = code.to_string()));
    }
    Ok(())
}

fn base_info() -> Value {
    json!({ "channel_version": "2.4.10", "bot_agent": "Multara" })
}

fn validate_base(raw: &str) -> Result<String, String> {
    let url = url::Url::parse(raw).map_err(|_| tr!("微信服务地址无效"))?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if url.scheme() != "https" || url.port().is_some() || !host.ends_with(".weixin.qq.com") {
        return Err(tr!("微信服务地址无效"));
    }
    Ok(format!("https://{host}"))
}

fn random_uin() -> String {
    let raw = uuid::Uuid::new_v4().as_u128() as u32;
    base64::engine::general_purpose::STANDARD.encode(raw.to_string())
}

fn urlencoding(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub fn decode_message(value: &Value) -> Option<InboundMessage> {
    if value
        .get("group_id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty())
    {
        return None;
    }
    let sender = value
        .get("from_user_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let to = value
        .get("to_user_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    if sender.is_empty() || sender == to {
        return None;
    }
    let mut text = String::new();
    if let Some(items) = value.get("item_list").and_then(Value::as_array) {
        for item in items {
            if let Some(piece) = item.pointer("/text_item/text").and_then(Value::as_str) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(piece);
            } else if let Some(piece) = item.pointer("/voice_item/text").and_then(Value::as_str) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(piece);
            }
        }
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let message_id = value
        .get("message_id")
        .map(value_id)
        .filter(|id| !id.is_empty())?;
    Some(InboundMessage {
        installation_id: String::new(),
        event_id: message_id.clone(),
        message_id,
        chat_id: sender.to_string(),
        chat_type: ChatType::P2p,
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

fn value_id(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|id| id.to_string()))
        .or_else(|| value.as_u64().map(|id| id.to_string()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{decode_message, qr_image_data_url};

    #[test]
    fn login_content_becomes_an_svg_qr() {
        let url = qr_image_data_url("https://weixin.qq.com/bot/login?ticket=abc").unwrap();
        assert!(url.starts_with("data:image/svg+xml"));
        assert!(url.contains("svg"));
    }

    #[test]
    fn private_text() {
        let value = serde_json::json!({
            "message_id": "99",
            "from_user_id": "user",
            "to_user_id": "bot",
            "item_list": [{ "type": 1, "text_item": { "text": "在吗" } }]
        });
        let message = decode_message(&value).unwrap();
        assert_eq!(message.text, "在吗");
        assert_eq!(message.sender_id, "user");
    }

    #[test]
    fn group_is_ignored() {
        let value = serde_json::json!({
            "message_id": "1",
            "group_id": "g",
            "from_user_id": "user",
            "to_user_id": "bot",
            "item_list": [{ "text_item": { "text": "群消息" } }]
        });
        assert!(decode_message(&value).is_none());
    }
}
