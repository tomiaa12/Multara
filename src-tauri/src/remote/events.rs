use std::{
    collections::HashMap,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;
use tauri::{AppHandle, Listener, Manager};

use super::{
    frame::Frame,
    permission::DeviceCtx,
    rules::arg,
    scope::{self, Access},
    server, terminals,
};
use crate::database::AppState;

/// 转发给远程设备的主机事件。其余事件（菜单、LSP、Canvas、工作流）只对本机界面有意义。
const FORWARDED: &[&str] = &[
    "run-event",
    "session-messages-changed",
    "inbox-changed",
    "terminal-data",
    "terminal-exit",
    "explorer-fs-changed",
];
/// 可见性判定的缓存时长：流式输出时 run-event 很密，不能每条都查库；权限变更最多延迟这么久生效。
const ACCESS_TTL: Duration = Duration::from_secs(5);

#[derive(Hash, PartialEq, Eq)]
enum Key {
    Session(String, String),
    Workspace(String, String),
}

#[derive(Default)]
struct Cache {
    access: HashMap<Key, (bool, Instant)>,
    devices: HashMap<String, (Option<DeviceCtx>, Instant)>,
}

impl Cache {
    fn device(&mut self, app: &AppHandle, device_id: &str) -> Option<DeviceCtx> {
        if let Some((ctx, at)) = self.devices.get(device_id) {
            if at.elapsed() < ACCESS_TTL {
                return ctx.clone();
            }
        }
        let ctx = app
            .state::<AppState>()
            .lock_db()
            .ok()
            .and_then(|db| DeviceCtx::load(&db, device_id).ok().flatten());
        self.devices
            .insert(device_id.to_string(), (ctx.clone(), Instant::now()));
        ctx
    }

    fn visible(&mut self, app: &AppHandle, ctx: &DeviceCtx, key: Key) -> bool {
        if let Some((visible, at)) = self.access.get(&key) {
            if at.elapsed() < ACCESS_TTL {
                return *visible;
            }
        }
        let visible = app
            .state::<AppState>()
            .lock_db()
            .ok()
            .and_then(|db| match &key {
                Key::Session(_, id) => scope::session_access(&db, ctx, id).ok(),
                Key::Workspace(_, id) => scope::workspace_access(&db, ctx, id).ok(),
            })
            .is_some_and(|access| access != Access::Hidden);
        self.access.insert(key, (visible, Instant::now()));
        visible
    }
}

pub fn install(app: &AppHandle) {
    let (sender, receiver) = mpsc::channel::<(&'static str, String)>();
    for name in FORWARDED.iter().copied() {
        let sender = sender.clone();
        app.listen_any(name, move |event| {
            if !server::paired_connections().is_empty() {
                let _ = sender.send((name, event.payload().to_string()));
            }
        });
    }
    let app = app.clone();
    let _ = thread::Builder::new()
        .name("multara-remote-events".into())
        .spawn(move || forward(&app, receiver));
}

fn forward(app: &AppHandle, receiver: mpsc::Receiver<(&'static str, String)>) {
    let mut cache = Cache::default();
    while let Ok((name, raw)) = receiver.recv() {
        let payload: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        for (device_id, conn) in server::paired_connections() {
            let Some(ctx) = cache.device(app, &device_id) else {
                continue;
            };
            let allowed = match name {
                "terminal-data" | "terminal-exit" => arg(&payload, "id")
                    .and_then(terminals::owner)
                    .is_some_and(|owner| owner == device_id),
                "inbox-changed" => true,
                "explorer-fs-changed" => arg(&payload, "workspaceId").is_some_and(|id| {
                    cache.visible(app, &ctx, Key::Workspace(device_id.clone(), id.to_string()))
                }),
                _ => payload
                    .as_str()
                    .or_else(|| arg(&payload, "sessionId"))
                    .is_some_and(|id| cache.visible(app, &ctx, Key::Session(device_id.clone(), id.to_string()))),
            };
            if !allowed {
                continue;
            }
            // inbox-changed 只是"请刷新"信号，不带具体会话。
            let payload = if name == "inbox-changed" {
                Value::Null
            } else {
                payload.clone()
            };
            conn.send(Frame::Evt {
                event: name.to_string(),
                payload,
            });
        }
    }
}
