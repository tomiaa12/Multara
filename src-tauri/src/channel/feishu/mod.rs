pub mod api;
pub mod card;
pub mod decode;
pub mod pb;
pub mod ws;

use std::collections::HashMap;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::database::AppState;

use super::{router, store, Installation};
use api::FeishuApi;
use pb::Frame;

/// 每个连接处理入站事件的线程数与排队上限。
const EVENT_WORKERS: usize = 4;
const EVENT_QUEUE_CAPACITY: usize = 256;
/// 分片消息的上限与过期时间，收不全的分片不会一直占着内存。
const MAX_CHUNKS: usize = 64;
const CHUNK_TTL: Duration = Duration::from_secs(60);

pub fn api_for(
    app: &AppHandle,
    installation_id: &str,
) -> Result<(Installation, FeishuApi), String> {
    let installation = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::get_installation(&db, installation_id)?
    }
    .ok_or_else(|| tr!("渠道连接不存在"))?;
    let secret = super::secret::load(app, installation_id)
        .ok_or_else(|| tr!("渠道凭据缺失，请重新填写 App Secret"))?;
    let api = FeishuApi::new(&installation, &secret);
    Ok((installation, api))
}

pub fn emit_status(
    app: &AppHandle,
    installation: &Installation,
    status: &str,
    error: Option<&str>,
) {
    let _ = app.emit(
        "channel-status",
        json!({
            "installationId": installation.id,
            "status": status,
            "lastError": error,
        }),
    );
}

/// 返回这一轮是否连上过，供守护线程在成功连接后重置退避。
pub fn run(app: &AppHandle, installation_id: &str, stop: &AtomicBool) -> bool {
    let mut connected = false;
    let result = run_inner(app, installation_id, stop, &mut connected);
    if let Ok((installation, _)) = api_for(app, installation_id) {
        match &result {
            Ok(()) if stop.load(Ordering::SeqCst) => {
                let state = app.state::<AppState>();
                if let Ok(db) = state.lock_db() {
                    let _ = store::set_status(&db, installation_id, "disconnected", None);
                }
                emit_status(app, &installation, "disconnected", None);
            }
            Ok(()) => {}
            Err(error) => {
                let state = app.state::<AppState>();
                if let Ok(db) = state.lock_db() {
                    let _ = store::set_status(&db, installation_id, "error", Some(error));
                }
                emit_status(app, &installation, "error", Some(error));
            }
        }
    }
    connected
}

fn run_inner(
    app: &AppHandle,
    installation_id: &str,
    stop: &AtomicBool,
    connected: &mut bool,
) -> Result<(), String> {
    let (installation, api) = api_for(app, installation_id)?;
    if installation.channel_type != super::TYPE_FEISHU
        && installation.channel_type != super::TYPE_LARK
    {
        return Err(tr!("暂不支持这个渠道类型"));
    }
    if installation.target_id.is_empty() {
        return Err(tr!("渠道未绑定智能体"));
    }

    let (url, ping_secs) = api.ws_endpoint()?;
    let (bot_open_id, bot_name) = api.bot_info().unwrap_or_default();
    let mut config = installation.config.clone();
    if !bot_open_id.is_empty() {
        config.bot_open_id = Some(bot_open_id.clone());
        config.bot_name = (!bot_name.is_empty()).then(|| bot_name.clone());
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            let _ = store::set_config(&db, installation_id, &config);
        };
    }

    let mut conn = ws::WsConn::connect(&url, Duration::from_secs(15))?;
    *connected = true;
    {
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            let _ = store::set_status(&db, installation_id, "connected", None);
            store::prune_inbound_dedup(&db);
        };
    }
    emit_status(app, &installation, "connected", None);

    let ping_interval = Duration::from_secs(ping_secs.clamp(20, 300));
    let mut next_ping = Instant::now() + ping_interval;
    let service = service_id_of(&url);
    let mut chunks: HashMap<String, PendingChunks> = HashMap::new();
    let events = spawn_event_workers(app, &installation);

    while !stop.load(Ordering::SeqCst) {
        if Instant::now() >= next_ping {
            conn.send_binary(&Frame::ping(service).marshal())?;
            next_ping = Instant::now() + ping_interval;
        }
        match conn.read_binary() {
            Ok(data) => {
                let frame = match Frame::unmarshal(&data) {
                    Ok(frame) => frame,
                    Err(error) => {
                        crate::diag::write(&format!(
                            "CHANNEL feishu {installation_id} 帧解析失败：{error}"
                        ));
                        continue;
                    }
                };
                if frame.method == pb::METHOD_CONTROL {
                    if frame.header_value(pb::HEADER_TYPE) == "ping" {
                        let _ = conn.send_binary(&Frame::pong(service).marshal());
                    }
                    continue;
                }

                let payload = match reassemble(&frame, &mut chunks) {
                    Some(payload) => payload,
                    None => continue,
                };

                let event = match decode::decode(
                    &payload,
                    installation_id,
                    config.bot_open_id.as_deref(),
                    config.bot_name.as_deref(),
                ) {
                    Ok(event) => event,
                    Err(error) => {
                        crate::diag::write(&format!(
                            "CHANNEL feishu {installation_id} 事件解码失败：{error}"
                        ));
                        let _ = conn.send_binary(&Frame::ack(&frame, true).marshal());
                        continue;
                    }
                };

                // 先确认收到，再处理：飞书要求 3 秒内 ACK。
                let event = match event {
                    Some(event) => {
                        let _ = conn.send_binary(&Frame::ack(&frame, true).marshal());
                        Some(event)
                    }
                    None => {
                        let _ = conn.send_binary(&Frame::ack(&frame, true).marshal());
                        None
                    }
                };
                if let Some(event) = event {
                    if events.try_send(event).is_err() {
                        crate::diag::write(&format!(
                            "CHANNEL feishu {installation_id} 入站事件积压过多，已丢弃一条"
                        ));
                    }
                }
            }
            Err(ws::WsError::Timeout) => {}
            Err(ws::WsError::Closed(reason)) => return Err(reason),
        }
    }
    conn.send_close();
    Ok(())
}

/// 入站事件交给固定数量的工作线程处理，队列满了就丢弃，陌生人刷消息不会起无上限的线程。
/// 连接结束时发送端被丢弃，工作线程处理完队列里剩下的事件后退出。
fn spawn_event_workers(
    app: &AppHandle,
    installation: &Installation,
) -> mpsc::SyncSender<super::InboundEvent> {
    let (sender, receiver) = mpsc::sync_channel::<super::InboundEvent>(EVENT_QUEUE_CAPACITY);
    let receiver = Arc::new(Mutex::new(receiver));
    for _ in 0..EVENT_WORKERS {
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
            let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                router::handle_event(&app, &installation, event)
            }));
        });
    }
    sender
}

struct PendingChunks {
    sum: usize,
    parts: Vec<Option<Vec<u8>>>,
    started: Instant,
}

fn reassemble(frame: &Frame, chunks: &mut HashMap<String, PendingChunks>) -> Option<Vec<u8>> {
    let sum = frame
        .header_value(pb::HEADER_SUM)
        .parse::<usize>()
        .unwrap_or(0);
    if sum <= 1 {
        return frame.payload.clone();
    }
    // 分片数来自服务端帧头，不能直接拿来分配
    if sum > MAX_CHUNKS {
        return None;
    }
    chunks.retain(|_, pending| pending.started.elapsed() < CHUNK_TTL);
    let seq = frame
        .header_value(pb::HEADER_SEQ)
        .parse::<usize>()
        .unwrap_or(0);
    let message_id = frame.header_value(pb::HEADER_MESSAGE_ID).to_string();
    if message_id.is_empty() {
        return None;
    }
    let fresh = || PendingChunks {
        sum,
        parts: vec![None; sum],
        started: Instant::now(),
    };
    let entry = chunks.entry(message_id.clone()).or_insert_with(fresh);
    if entry.sum != sum {
        *entry = fresh();
    }
    if seq < sum {
        entry.parts[seq] = frame.payload.clone();
    }
    if entry.parts.iter().all(Option::is_some) {
        let parts = chunks.remove(&message_id)?.parts;
        let mut out = Vec::new();
        for part in parts {
            out.extend_from_slice(&part.unwrap_or_default());
        }
        return Some(out);
    }
    None
}

fn service_id_of(url: &str) -> i32 {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .query_pairs()
                .find(|(key, _)| key == "service_id")
                .and_then(|(_, value)| value.parse::<i32>().ok())
        })
        .unwrap_or(0)
}
