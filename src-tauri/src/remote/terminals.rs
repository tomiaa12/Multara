use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock, PoisonError},
    thread,
    time::Duration,
};

use serde_json::json;
use tauri::AppHandle;

use super::{ipc, server};

/// 设备断开后保留它打开的终端这么久，超时仍未重连就关闭。
const OFFLINE_GRACE: Duration = Duration::from_secs(10 * 60);

/// 终端 id → 打开它的设备 id。只有打开者能看到、写入和关闭。
static OWNERS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn owners() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    OWNERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub fn claim(terminal_id: &str, device_id: &str) {
    owners().insert(terminal_id.to_string(), device_id.to_string());
}

pub fn owned_by(terminal_id: &str, device_id: &str) -> bool {
    owners().get(terminal_id).is_some_and(|owner| owner == device_id)
}

pub fn owner(terminal_id: &str) -> Option<String> {
    owners().get(terminal_id).cloned()
}

pub fn release(terminal_id: &str) {
    owners().remove(terminal_id);
}

fn device_terminals(device_id: &str) -> Vec<String> {
    owners()
        .iter()
        .filter(|(_, owner)| owner.as_str() == device_id)
        .map(|(id, _)| id.clone())
        .collect()
}

pub fn close_all(app: &AppHandle, device_id: &str) {
    for id in device_terminals(device_id) {
        let _ = ipc::dispatch(app, "terminal_close", json!({ "id": id }));
        release(&id);
    }
}

/// 设备掉线后延迟清理：宽限期内重连则保留。
pub fn schedule_cleanup(app: &AppHandle, device_id: &str) {
    if device_terminals(device_id).is_empty() {
        return;
    }
    let app = app.clone();
    let device_id = device_id.to_string();
    let _ = thread::Builder::new()
        .name("multara-remote-term-gc".into())
        .spawn(move || {
            thread::sleep(OFFLINE_GRACE);
            if !server::online_device_ids().contains(&device_id) {
                close_all(&app, &device_id);
            }
        });
}
