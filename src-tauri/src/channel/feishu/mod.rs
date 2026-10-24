pub mod api;
pub mod card;
pub mod decode;
pub mod pb;
pub mod ws;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::database::AppState;

use super::{router, store, Installation};
use api::FeishuApi;
use pb::Frame;

pub fn api_for(app: &AppHandle, installation_id: &str) -> Result<(Installation, FeishuApi), String> {
    let installation = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::get_installation(&db, installation_id)?
    }
    .ok_or_else(|| "渠道连接不存在".to_string())?;
    let secret = super::secret::load(app, installation_id)
        .ok_or_else(|| "渠道凭据缺失，请重新填写 App Secret".to_string())?;
    let api = FeishuApi::new(&installation, &secret);
    Ok((installation, api))
}

pub fn emit_status(app: &AppHandle, installation: &Installation, status: &str, error: Option<&str>) {
    let _ = app.emit(
        "channel-status",
        json!({
            "installationId": installation.id,
            "status": status,
            "lastError": error,
        }),
    );
}

pub fn run(app: &AppHandle, installation_id: &str, stop: &AtomicBool) {
    let result = run_inner(app, installation_id, stop);
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
}

fn run_inner(app: &AppHandle, installation_id: &str, stop: &AtomicBool) -> Result<(), String> {
    let (installation, api) = api_for(app, installation_id)?;
    if installation.channel_type != super::TYPE_FEISHU && installation.channel_type != super::TYPE_LARK {
        return Err("暂不支持这个渠道类型".to_string());
    }
    if installation.target_id.is_empty() {
        return Err("渠道未绑定智能体".to_string());
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
    let mut chunks: HashMap<String, (usize, Vec<Option<Vec<u8>>>)> = HashMap::new();

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
                    let app = app.clone();
                    let installation = installation.clone();
                    std::thread::spawn(move || {
                        router::handle_event(&app, &installation, event);
                    });
                }
            }
            Err(ws::WsError::Timeout) => {}
            Err(ws::WsError::Closed(reason)) => return Err(reason),
        }
    }
    conn.send_close();
    Ok(())
}

fn reassemble(
    frame: &Frame,
    chunks: &mut HashMap<String, (usize, Vec<Option<Vec<u8>>>)>,
) -> Option<Vec<u8>> {
    let sum = frame
        .header_value(pb::HEADER_SUM)
        .parse::<usize>()
        .unwrap_or(0);
    if sum <= 1 {
        return frame.payload.clone();
    }
    let seq = frame
        .header_value(pb::HEADER_SEQ)
        .parse::<usize>()
        .unwrap_or(0);
    let message_id = frame.header_value(pb::HEADER_MESSAGE_ID).to_string();
    if message_id.is_empty() {
        return None;
    }
    let entry = chunks
        .entry(message_id.clone())
        .or_insert_with(|| (sum, vec![None; sum]));
    if entry.0 != sum {
        *entry = (sum, vec![None; sum]);
    }
    if seq < sum {
        entry.1[seq] = frame.payload.clone();
    }
    if entry.1.iter().all(Option::is_some) {
        let (_, parts) = chunks.remove(&message_id)?;
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
