//! 按渠道类型分发连接、探测和文字回复。飞书卡片仍走 `feishu`。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};

use tauri::{AppHandle, Manager};

use crate::database::AppState;

use super::feishu::ws::WsConn;
use super::store::{self, Delivery};
use super::{
    dingtalk, feishu, secret, slack, telegram, wecom, weixin, ChatType, InboundEvent, Installation,
    TYPE_DINGTALK, TYPE_FEISHU, TYPE_LARK, TYPE_SLACK, TYPE_TELEGRAM, TYPE_WECOM, TYPE_WEIXIN,
};

pub const TEXT_LIMIT: usize = 3500;

pub struct TextTarget<'a> {
    pub chat_id: &'a str,
    pub chat_type: ChatType,
    pub thread_id: Option<&'a str>,
    pub reply_to: Option<&'a str>,
    pub sender_id: Option<&'a str>,
    pub text: &'a str,
}

pub fn load(app: &AppHandle, installation_id: &str) -> Result<(Installation, String), String> {
    let installation = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::get_installation(&db, installation_id)?
    }
    .ok_or_else(|| tr!("渠道连接不存在"))?;
    let secret =
        secret::load(app, installation_id).ok_or_else(|| tr!("渠道凭据缺失，请重新填写"))?;
    Ok((installation, secret))
}

pub fn run(app: &AppHandle, installation: &Installation, stop: &AtomicBool) -> bool {
    match installation.channel_type.as_str() {
        TYPE_FEISHU | TYPE_LARK => feishu::run(app, &installation.id, stop),
        TYPE_SLACK => slack::run(app, &installation.id, stop),
        TYPE_DINGTALK => dingtalk::run(app, &installation.id, stop),
        TYPE_WECOM => wecom::run(app, &installation.id, stop),
        TYPE_TELEGRAM => telegram::run(app, &installation.id, stop),
        TYPE_WEIXIN => weixin::run(app, &installation.id, stop),
        _ => {
            mark(
                app,
                installation,
                "error",
                Some(&tr!("暂不支持这个渠道类型")),
            );
            false
        }
    }
}

pub fn mark_connected(app: &AppHandle, installation: &Installation) {
    mark(app, installation, "connected", None);
}

fn mark(app: &AppHandle, installation: &Installation, status: &str, error: Option<&str>) {
    let state = app.state::<AppState>();
    if let Ok(db) = state.lock_db() {
        let _ = store::set_status(&db, &installation.id, status, error);
    }
    super::emit_status(app, installation, status, error);
}

pub fn spawn_workers(app: &AppHandle, installation: &Installation) -> SyncSender<InboundEvent> {
    let (sender, receiver) = mpsc::sync_channel(256);
    let receiver = Arc::new(Mutex::new(receiver));
    for _ in 0..4 {
        let app = app.clone();
        let installation = installation.clone();
        let receiver = receiver.clone();
        std::thread::spawn(move || loop {
            let Ok(event) = receiver
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .recv()
            else {
                break;
            };
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::router::handle_event(&app, &installation, event);
            }));
        });
    }
    sender
}

pub fn send_text(
    app: &AppHandle,
    installation: &Installation,
    target: TextTarget<'_>,
) -> Result<String, String> {
    let text = target.text.trim();
    if text.is_empty() {
        return Err(tr!("没有可发送的文本"));
    }
    match installation.channel_type.as_str() {
        TYPE_SLACK => slack::send(app, installation, &target),
        TYPE_DINGTALK => dingtalk::send(app, installation, &target),
        TYPE_WECOM => wecom::send(installation, &target),
        TYPE_TELEGRAM => telegram::send(app, installation, &target),
        TYPE_WEIXIN => weixin::send(app, installation, &target),
        _ => Err(tr!("这个渠道不能按纯文本回复")),
    }
}

pub fn send_delivery(
    app: &AppHandle,
    installation: &Installation,
    delivery: &Delivery,
    text: &str,
) -> Result<String, String> {
    send_text(
        app,
        installation,
        TextTarget {
            chat_id: &delivery.chat_id,
            chat_type: delivery.chat_type,
            thread_id: delivery.thread_id.as_deref(),
            reply_to: delivery.reply_to_message_id.as_deref(),
            sender_id: delivery.sender_id.as_deref(),
            text,
        },
    )
}

pub fn probe(app: &AppHandle, installation_id: &str) -> Result<(String, String), String> {
    let (installation, secret) = load(app, installation_id)?;
    match installation.channel_type.as_str() {
        TYPE_FEISHU | TYPE_LARK => {
            let api = feishu::api::FeishuApi::new(&installation, &secret);
            let bot = api.bot_info()?;
            Ok((bot.open_id, bot.name))
        }
        TYPE_SLACK => slack::probe(&secret),
        TYPE_DINGTALK => dingtalk::probe(&installation.config.app_id, &secret),
        TYPE_WECOM => wecom::probe(&installation.config.app_id, &secret),
        TYPE_TELEGRAM => telegram::probe(&secret),
        TYPE_WEIXIN => weixin::probe(&installation, &secret),
        _ => Err(tr!("暂不支持这个渠道类型")),
    }
}

pub fn chunk_text(text: &str, limit: usize) -> Vec<String> {
    let limit = limit.max(1);
    if text.chars().count() <= limit {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        if !current.is_empty() && current.chars().count() + line.chars().count() > limit {
            out.push(std::mem::take(&mut current));
        }
        if line.chars().count() > limit {
            for ch in line.chars() {
                current.push(ch);
                if current.chars().count() >= limit {
                    out.push(std::mem::take(&mut current));
                }
            }
        } else {
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

pub fn scrub(error: String, secret: &str) -> String {
    if secret.is_empty() {
        error
    } else {
        error.replace(secret, "***")
    }
}

pub fn connect_ws(url: &str) -> Result<WsConn, String> {
    WsConn::connect(url, std::time::Duration::from_secs(15))
}

/// `attempt` 需要知道中途是否连上。用这个包装避免每个平台再写一遍状态机。
pub fn run_socket(
    app: &AppHandle,
    installation_id: &str,
    stop: &AtomicBool,
    body: impl FnOnce(&AppHandle, &Installation, &str, &AtomicBool, &mut bool) -> Result<(), String>,
) -> bool {
    let mut connected = false;
    let result: Result<Installation, String> = (|| {
        let (installation, secret) = load(app, installation_id)?;
        body(app, &installation, &secret, stop, &mut connected)?;
        Ok(installation)
    })();
    match result {
        Ok(installation) => {
            if stop.load(Ordering::SeqCst) {
                mark(app, &installation, "disconnected", None);
            }
        }
        Err(error) => {
            if let Ok((installation, _)) = load(app, installation_id) {
                mark(app, &installation, "error", Some(&error));
            }
        }
    }
    connected
}
