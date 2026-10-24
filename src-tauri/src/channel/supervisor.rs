//! 渠道连接的守护：按数据库里的配置启停接收线程，断线后指数退避重连。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::database::AppState;

use super::{feishu, store, Installation};

type Registry = HashMap<String, (String, Arc<AtomicBool>)>;

static ACTIVE: OnceLock<Mutex<Registry>> = OnceLock::new();

fn active() -> &'static Mutex<Registry> {
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        loop {
            sync(&app);
            thread::sleep(Duration::from_secs(3));
        }
    });
}

fn fingerprint(installation: &Installation) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        installation.channel_type,
        installation.config.app_id,
        installation.config.region(),
        installation.target_type,
        installation.target_id,
        installation.project_id.clone().unwrap_or_default(),
        installation.enabled as u8
    )
}

fn sync(app: &AppHandle) {
    let installations = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return;
        };
        store::list_installations(&db).unwrap_or_default()
    };

    let mut wanted: HashMap<String, (String, Installation)> = HashMap::new();
    for installation in installations {
        if installation.enabled {
            wanted.insert(installation.id.clone(), (fingerprint(&installation), installation));
        }
    }

    let mut stop_ids = Vec::new();
    {
        let Ok(mut registry) = active().lock() else {
            return;
        };
        for (id, (fingerprint, stop)) in registry.iter() {
            match wanted.get(id) {
                Some((wanted_fingerprint, _)) if wanted_fingerprint == fingerprint => {}
                _ => {
                    stop.store(true, Ordering::SeqCst);
                    stop_ids.push(id.clone());
                }
            }
        }
        for id in &stop_ids {
            registry.remove(id);
        }
        for (id, (fingerprint, installation)) in wanted {
            if registry.contains_key(&id) {
                continue;
            }
            let stop = Arc::new(AtomicBool::new(false));
            registry.insert(id.clone(), (fingerprint, stop.clone()));
            let app = app.clone();
            let installation = installation.clone();
            thread::spawn(move || run_loop(app, installation, stop));
        }
    }
}

/// 设置变更后立刻重新同步，不必等下一轮轮询。
pub fn reload(app: &AppHandle) {
    sync(app);
}

fn run_loop(app: AppHandle, installation: Installation, stop: Arc<AtomicBool>) {
    let mut backoff = 1u64;
    while !stop.load(Ordering::SeqCst) {
        feishu::run(&app, &installation.id, &stop);
        if stop.load(Ordering::SeqCst) {
            break;
        }
        // 退避期间也要能立刻停止。
        let mut waited = 0u64;
        while waited < backoff && !stop.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_secs(1));
            waited += 1;
        }
        backoff = (backoff * 2).min(60);
    }
    {
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            let _ = store::set_status(&db, &installation.id, "disconnected", None);
        };
    }
    feishu::emit_status(&app, &installation, "disconnected", None);
}
